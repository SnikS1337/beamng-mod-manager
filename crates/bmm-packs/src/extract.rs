use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::inspect::is_unsafe_entry_path;
use crate::zip_index::normalize_name;

const TAR_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractBudget {
    pub limit: u64,
    pub used: u64,
}

impl ExtractBudget {
    pub fn new(limit: u64) -> Self {
        Self { limit, used: 0 }
    }

    pub fn remaining(&self) -> u64 {
        self.limit.saturating_sub(self.used)
    }

    pub(crate) fn add(&mut self, bytes: u64) -> Result<(), ExtractError> {
        self.used = self.used.saturating_add(bytes);
        if self.used > self.limit {
            return Err(ExtractError::Message(
                "Архив слишком большой после распаковки".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ExtractError {
    #[error("{0}")]
    Message(String),
    #[error("отменено")]
    Cancelled,
}

pub fn is_archive_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    (lower.ends_with(".zip") || lower.ends_with(".rar") || lower.ends_with(".7z"))
        && !name.starts_with('.')
}

pub fn archive_kind(path: &Path) -> Option<&'static str> {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_lowercase())?;
    match extension.as_str() {
        "zip" => Some("zip"),
        "rar" => Some("rar"),
        "7z" => Some("7z"),
        _ => None,
    }
}

pub fn extract_archive(
    archive: &Path,
    destination: &Path,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
) -> Result<(), ExtractError> {
    fs::create_dir_all(destination).map_err(io_error)?;
    match archive_kind(archive) {
        Some("zip") => extract_zip(archive, destination, budget, cancel),
        Some(_) => extract_with_tar(archive, destination, budget, cancel),
        None => Err(ExtractError::Message(
            "RAR и 7z здесь не открываются.".to_string(),
        )),
    }
}

pub fn extract_zip(
    archive: &Path,
    destination: &Path,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
) -> Result<(), ExtractError> {
    let file = File::open(archive).map_err(io_error)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|_| ExtractError::Message("Архив повреждён и не читается.".to_string()))?;

    for index in 0..zip.len() {
        if cancel.load(Ordering::Relaxed) {
            return Err(ExtractError::Cancelled);
        }
        let mut entry = zip
            .by_index(index)
            .map_err(|_| ExtractError::Message("Архив повреждён и не читается.".to_string()))?;
        if entry.is_dir() {
            continue;
        }
        let raw = entry
            .name()
            .map_err(|_| ExtractError::Message("Архив повреждён и не читается.".to_string()))?;
        let Some(name) = normalize_name(&raw) else {
            return Err(ExtractError::Message(
                "В архиве есть файл с опасным путём.".to_string(),
            ));
        };
        if is_unsafe_entry_path(&name) {
            return Err(ExtractError::Message(
                "В архиве есть файл с опасным путём.".to_string(),
            ));
        }
        if entry
            .unix_mode()
            .map(|mode| mode & 0o170000 == 0o120000)
            .unwrap_or(false)
        {
            return Err(ExtractError::Message(
                "В архиве есть символическая ссылка.".to_string(),
            ));
        }

        let target = destination.join(&name);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(io_error)?;
        }
        let mut output = File::create(&target).map_err(io_error)?;
        let mut buffer = [0u8; 81920];
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err(ExtractError::Cancelled);
            }
            let read = entry.read(&mut buffer).map_err(io_error)?;
            if read == 0 {
                break;
            }
            budget.add(read as u64)?;
            output.write_all(&buffer[..read]).map_err(io_error)?;
        }
    }
    Ok(())
}

fn extract_with_tar(
    archive: &Path,
    destination: &Path,
    budget: &mut ExtractBudget,
    cancel: &AtomicBool,
) -> Result<(), ExtractError> {
    let tar = tar_path().ok_or_else(|| {
        ExtractError::Message(
            "В этой Windows нет распаковщика RAR и 7z. Перепакуйте архив в zip".to_string(),
        )
    })?;

    let mut child = Command::new(tar)
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(destination)
        .current_dir(destination)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            ExtractError::Message(format!("Не удалось запустить распаковщик: {error}"))
        })?;

    let started = Instant::now();
    loop {
        if cancel.load(Ordering::Relaxed) {
            kill_tree(&mut child);
            return Err(ExtractError::Cancelled);
        }
        if started.elapsed() > TAR_TIMEOUT {
            kill_tree(&mut child);
            return Err(ExtractError::Message(
                "Распаковка идёт слишком долго".to_string(),
            ));
        }
        if dir_size(destination) > budget.limit {
            kill_tree(&mut child);
            return Err(ExtractError::Message(
                "Распакованный архив слишком большой".to_string(),
            ));
        }
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => thread::sleep(Duration::from_millis(250)),
            Err(error) => {
                kill_tree(&mut child);
                return Err(io_error(error));
            }
        }
    }

    let output = child.wait_with_output().map_err(io_error)?;
    let size = dir_size(destination);
    if size > budget.limit {
        return Err(ExtractError::Message(
            "Распакованный архив слишком большой".to_string(),
        ));
    }
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
        let extension = archive
            .extension()
            .map(|value| value.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let message = if stderr.contains("encrypt")
            || stderr.contains("passphrase")
            || stderr.contains("password")
        {
            "Архив под паролем".to_string()
        } else if extension == "rar"
            && (stderr.contains("rar5")
                || stderr.contains("unrecognized archive format")
                || stderr.contains("not supported")
                || stderr.contains("unsupported")
                || stderr.contains("lzma")
                || stderr.contains("bzip2")
                || stderr.contains("ppmd"))
        {
            "Этот RAR новой версии, а распаковщик Windows его не читает. Перепакуйте архив в zip"
                .to_string()
        } else if stderr.contains("unrecognized archive format")
            || stderr.contains("unavailable")
            || stderr.contains("not supported")
            || stderr.contains("unsupported")
        {
            "Распаковщик Windows этот архив не читает (так бывает на Windows 10). Перепакуйте его в zip"
                .to_string()
        } else {
            "Архив не распаковался: он повреждён или недокачан".to_string()
        };
        return Err(ExtractError::Message(message));
    }
    if contains_reparse(destination) {
        return Err(ExtractError::Message(
            "В архиве есть символическая ссылка.".to_string(),
        ));
    }
    budget.used = budget.used.saturating_add(size);
    if budget.used > budget.limit {
        return Err(ExtractError::Message(
            "Распакованный архив слишком большой".to_string(),
        ));
    }
    Ok(())
}

fn tar_path() -> Option<PathBuf> {
    let path = PathBuf::from(std::env::var_os("SystemRoot")?)
        .join("System32")
        .join("tar.exe");
    path.is_file().then_some(path)
}

fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

pub fn dir_size(root: &Path) -> u64 {
    let mut stack = vec![root.to_path_buf()];
    let mut total = 0u64;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if is_reparse(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    total
}

fn contains_reparse(root: &Path) -> bool {
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                continue;
            };
            if is_reparse(&metadata) {
                return true;
            }
            if metadata.is_dir() {
                stack.push(path);
            }
        }
    }
    false
}

fn is_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        false
    }
}

fn io_error(error: std::io::Error) -> ExtractError {
    ExtractError::Message(format!("Архив не читается: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn extracts_zip_and_counts_budget() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("mod.zip");
        build_zip(&archive, &[("vehicles/car/a.jbeam", b"hello")]);
        let dest = temp.path().join("out");
        let mut budget = ExtractBudget::new(1000);
        let cancel = AtomicBool::new(false);
        extract_archive(&archive, &dest, &mut budget, &cancel).unwrap();
        assert_eq!(
            fs::read(dest.join("vehicles/car/a.jbeam")).unwrap(),
            b"hello"
        );
        assert_eq!(budget.used, 5);
    }

    #[test]
    fn rejects_budget_and_unsafe_paths() {
        let temp = tempfile::tempdir().unwrap();
        let archive = temp.path().join("mod.zip");
        build_zip(&archive, &[("vehicles/car/a.jbeam", b"hello")]);
        let mut budget = ExtractBudget::new(2);
        let cancel = AtomicBool::new(false);
        let error =
            extract_archive(&archive, &temp.path().join("out"), &mut budget, &cancel).unwrap_err();
        assert!(error.to_string().contains("слишком большой"));

        let unsafe_zip = temp.path().join("evil.zip");
        build_zip(&unsafe_zip, &[("../evil.txt", b"x")]);
        let mut budget = ExtractBudget::new(1000);
        let error = extract_archive(
            &unsafe_zip,
            &temp.path().join("evil-out"),
            &mut budget,
            &cancel,
        )
        .unwrap_err();
        assert!(error.to_string().contains("опасным путём"));
    }

    #[test]
    fn archive_names() {
        assert!(is_archive_name("a.zip"));
        assert!(is_archive_name("a.RAR"));
        assert!(is_archive_name("a.7z"));
        assert!(!is_archive_name(".hidden.zip"));
        assert!(!is_archive_name("a.zip.crdownload"));
    }
}

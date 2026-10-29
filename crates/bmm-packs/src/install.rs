use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};
use sha2::{Digest, Sha256};

use bmm_domain::clock::Clock;

use crate::inspect::{ArchiveLimits, InspectionResult, inspect_zip};
use crate::safe_name::safe_file_name;
use crate::scan::ModLayout;

#[derive(Debug, Clone)]
pub struct InstallOptions {
    pub allow_warnings: bool,
    pub replace_existing: bool,
    pub limits: ArchiveLimits,
    pub max_archive_bytes: u64,
}

impl Default for InstallOptions {
    fn default() -> Self {
        Self {
            allow_warnings: false,
            replace_existing: false,
            limits: ArchiveLimits::default(),
            max_archive_bytes: 8 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallError {
    BadName,
    Blocked,
    HasWarnings,
    AlreadyExists,
    Io,
    InUse,
    TooLarge,
}

#[derive(Debug)]
pub struct InstallResult {
    pub success: bool,
    pub error: Option<InstallError>,
    pub message: Option<String>,
    pub path: Option<PathBuf>,
    pub sha256: Option<String>,
    pub inspection: Option<InspectionResult>,
}

impl InstallResult {
    fn fail(error: InstallError, message: impl Into<String>) -> Self {
        Self {
            success: false,
            error: Some(error),
            message: Some(message.into()),
            path: None,
            sha256: None,
            inspection: None,
        }
    }

    fn fail_with_inspection(
        error: InstallError,
        message: impl Into<String>,
        inspection: InspectionResult,
    ) -> Self {
        Self {
            success: false,
            error: Some(error),
            message: Some(message.into()),
            path: None,
            sha256: None,
            inspection: Some(inspection),
        }
    }
}

#[derive(Debug)]
pub struct UpdateResult {
    pub install: InstallResult,
    pub old_removed: bool,
    pub was_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpResult {
    pub success: bool,
    pub message: String,
    pub path: Option<PathBuf>,
}

impl OpResult {
    pub(crate) fn ok(message: impl Into<String>, path: Option<PathBuf>) -> Self {
        Self {
            success: true,
            message: message.into(),
            path,
        }
    }

    pub(crate) fn fail(message: impl Into<String>) -> Self {
        Self {
            success: false,
            message: message.into(),
            path: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BackupEntry {
    pub id: String,
    pub file_name: String,
    pub sequence: i64,
    pub saved_at: Option<DateTime<FixedOffset>>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct DeletedMod {
    pub file_name: String,
    pub copies: usize,
    pub last_saved_at: Option<DateTime<FixedOffset>>,
    pub last_size_bytes: u64,
}

pub struct ModInstaller {
    layout: ModLayout,
    now: Clock,
    keep_backups: Arc<dyn Fn() -> usize + Send + Sync>,
    gate: Mutex<()>,
}

impl ModInstaller {
    pub fn new(
        layout: ModLayout,
        now: Clock,
        keep_backups: Arc<dyn Fn() -> usize + Send + Sync>,
    ) -> Self {
        Self {
            layout,
            now,
            keep_backups,
            gate: Mutex::new(()),
        }
    }

    pub fn layout(&self) -> &ModLayout {
        &self.layout
    }

    pub fn mods_folder_available(&self) -> bool {
        self.layout.mods_dir.is_dir()
    }

    pub fn install_from_path(
        &self,
        source: &Path,
        file_name: &str,
        options: InstallOptions,
    ) -> InstallResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return InstallResult::fail(
                InstallError::BadName,
                "Недопустимое имя файла (нужен .zip без путей).",
            );
        };
        let temp = self
            .layout
            .mods_dir
            .join(format!(".{}.part", unique_suffix()));
        let result = self.install_inner(source, &name, &temp, &options);
        let _ = fs::remove_file(&temp);
        result
    }

    fn install_inner(
        &self,
        source: &Path,
        name: &str,
        temp: &Path,
        options: &InstallOptions,
    ) -> InstallResult {
        if let Err(message) = fs::create_dir_all(&self.layout.mods_dir) {
            return InstallResult::fail_from_io(message);
        }
        if let Some(existing) = self.check_target(name, options) {
            return existing;
        }

        let copy = copy_with_hash(source, temp, options.max_archive_bytes);
        let hash = match copy {
            Ok(hash) => hash,
            Err(error) => return InstallResult::fail(error.0, error.1),
        };

        let inspection = inspect_zip(temp, &options.limits);
        if inspection.is_blocked() {
            return InstallResult::fail_with_inspection(
                InstallError::Blocked,
                "Архив не прошёл проверку структуры.",
                inspection,
            );
        }
        if inspection.has_warnings() && !options.allow_warnings {
            return InstallResult::fail_with_inspection(
                InstallError::HasWarnings,
                "В архиве есть предупреждения: нужно ваше решение.",
                inspection,
            );
        }

        let _guard = self.gate.lock().unwrap();
        if let Some(existing) = self.check_target(name, options) {
            return existing;
        }
        let target = self.target_path(name);
        let mut moved: Option<PathBuf> = None;
        if target.exists() {
            match self.backup_file(&target, name) {
                Ok(path) => moved = Some(path),
                Err(error) => return InstallResult::fail_from_io(error),
            }
        }
        if let Some(parent) = target.parent()
            && let Err(error) = fs::create_dir_all(parent)
        {
            self.restore_moved(&moved, &target);
            return InstallResult::fail_from_io(error);
        }
        if let Err(error) = fs::rename(temp, &target) {
            self.restore_moved(&moved, &target);
            return InstallResult::fail_from_io(error);
        }
        self.rotate(name);

        InstallResult {
            success: true,
            error: None,
            message: None,
            path: Some(target),
            sha256: Some(hash),
            inspection: Some(inspection),
        }
    }

    pub fn update_from_path(
        &self,
        old_file_name: &str,
        source: &Path,
        new_file_name: &str,
        options: InstallOptions,
    ) -> UpdateResult {
        let Some(old_name) = safe_file_name(Some(old_file_name)) else {
            return UpdateResult {
                install: InstallResult::fail(
                    InstallError::BadName,
                    "Недопустимое имя файла (нужен .zip без путей).",
                ),
                old_removed: false,
                was_enabled: true,
            };
        };
        let Some(new_name) = safe_file_name(Some(new_file_name)) else {
            return UpdateResult {
                install: InstallResult::fail(
                    InstallError::BadName,
                    "Недопустимое имя файла (нужен .zip без путей).",
                ),
                old_removed: false,
                was_enabled: true,
            };
        };

        let (old_exists, was_enabled) = {
            let in_mods = self.layout.mods_dir.join(&old_name).is_file();
            let in_disabled = self.layout.disabled_dir.join(&old_name).is_file();
            (in_mods || in_disabled, !in_disabled || in_mods)
        };
        let same_name = old_name.eq_ignore_ascii_case(&new_name);
        if !same_name
            && (self.layout.mods_dir.join(&new_name).is_file()
                || self.layout.disabled_dir.join(&new_name).is_file())
        {
            return UpdateResult {
                install: InstallResult::fail(
                    InstallError::AlreadyExists,
                    "Файл с именем новой версии уже есть в папке.",
                ),
                old_removed: false,
                was_enabled,
            };
        }

        let options = InstallOptions {
            allow_warnings: options.allow_warnings,
            limits: options.limits,
            max_archive_bytes: options.max_archive_bytes,
            replace_existing: true,
        };
        let install = self.install_from_path(source, &new_name, options);
        if !install.success || same_name || !old_exists {
            // Старая версия «убрана» только если новая встала на её место.
            let old_removed = same_name && install.success;
            return UpdateResult {
                install,
                old_removed,
                was_enabled,
            };
        }

        let removed = self.remove(&old_name);
        if !removed.success {
            let _ = fs::remove_file(self.layout.mods_dir.join(&new_name));
            return UpdateResult {
                install: InstallResult::fail(InstallError::InUse, removed.message),
                old_removed: false,
                was_enabled,
            };
        }
        if !was_enabled {
            let off = self.disable(&new_name);
            if !off.success {
                return UpdateResult {
                    install: InstallResult {
                        message: Some(format!(
                            "Обновлено, но мод остался включённым: {}",
                            off.message
                        )),
                        ..install
                    },
                    old_removed: true,
                    was_enabled,
                };
            }
        }
        UpdateResult {
            install,
            old_removed: true,
            was_enabled,
        }
    }

    pub fn disable(&self, file_name: &str) -> OpResult {
        self.move_file(
            file_name,
            &self.layout.mods_dir,
            &self.layout.disabled_dir,
            "выключен",
        )
    }

    pub fn enable(&self, file_name: &str) -> OpResult {
        self.move_file(
            file_name,
            &self.layout.disabled_dir,
            &self.layout.mods_dir,
            "включён",
        )
    }

    fn move_file(&self, file_name: &str, from: &Path, to: &Path, verb: &str) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        let src = from.join(&name);
        let dst = to.join(&name);
        if !src.is_file() {
            return OpResult::fail("Файл не найден.");
        }
        if dst.exists() {
            return OpResult::fail("В папке уже есть файл с таким именем.");
        }
        if let Err(error) = fs::create_dir_all(to) {
            return OpResult::fail(io_message(&error));
        }
        match move_file(&src, &dst) {
            Ok(()) => OpResult::ok(format!("Мод {verb}."), Some(dst)),
            Err(error) => OpResult::fail(io_message(&error)),
        }
    }

    pub fn remove(&self, file_name: &str) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        let mut backup = None;
        let mut removed = 0;
        for dir in [&self.layout.mods_dir, &self.layout.disabled_dir] {
            let path = dir.join(&name);
            if !path.is_file() {
                continue;
            }
            match self.backup_file(&path, &name) {
                Ok(saved) => {
                    removed += 1;
                    backup.get_or_insert(saved);
                }
                Err(error) => return OpResult::fail(io_message(&error)),
            }
        }
        if removed == 0 {
            return OpResult::fail("Файл не найден.");
        }
        self.rotate(&name);
        OpResult::ok("Удалён, копия сохранена.", backup)
    }

    pub fn unremove(&self, file_name: &str, backup_id: &str, into_disabled: bool) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        let Some(source) = self.find_backup(&name, backup_id) else {
            return OpResult::fail("Копия удалённого мода пропала.");
        };
        if self.layout.mods_dir.join(&name).exists()
            || self.layout.disabled_dir.join(&name).exists()
        {
            return OpResult::fail("Файл с таким именем уже есть в папке.");
        }
        let dir = if into_disabled {
            &self.layout.disabled_dir
        } else {
            &self.layout.mods_dir
        };
        let target = dir.join(&name);
        if let Err(error) = fs::create_dir_all(dir) {
            return OpResult::fail(io_message(&error));
        }
        match move_file(&source, &target) {
            Ok(()) => OpResult::ok("Возвращено.", Some(target)),
            Err(error) => OpResult::fail(io_message(&error)),
        }
    }

    pub fn backups(&self, file_name: &str) -> Vec<PathBuf> {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return Vec::new();
        };
        let dir = long_path(&self.layout.backup_dir).join(&name);
        let Ok(entries) = fs::read_dir(dir) else {
            return Vec::new();
        };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.file_name()
                    .map(|file_name| is_backup_of(&file_name.to_string_lossy(), &name))
                    .unwrap_or(false)
            })
            .collect();
        files.sort();
        files
    }

    pub fn backed_up_file_names(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(long_path(&self.layout.backup_dir)) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter(|entry| entry.path().is_dir())
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                safe_file_name(Some(&name)).filter(|safe| !self.backups(safe).is_empty())
            })
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names
    }

    pub fn list_backups(&self, file_name: &str) -> Vec<BackupEntry> {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return Vec::new();
        };
        let mut list: Vec<BackupEntry> = self
            .backups(&name)
            .into_iter()
            .filter_map(|path| parse_backup(&path, &name))
            .collect();
        list.sort_by_key(|entry| std::cmp::Reverse(entry.sequence));
        list
    }

    pub fn restore_latest_backup(&self, file_name: &str) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        let Some(last) = self.backups(&name).last().cloned() else {
            return OpResult::fail("Копий нет.");
        };
        self.restore_from(&name, &last)
    }

    pub fn restore_backup(&self, file_name: &str, backup_id: &str) -> OpResult {
        let Some(name) = safe_file_name(Some(file_name)) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        let _guard = self.gate.lock().unwrap();
        match self.find_backup(&name, backup_id) {
            Some(path) => self.restore_from(&name, &path),
            None => OpResult::fail("Такой копии нет."),
        }
    }

    pub fn restore_version(
        &self,
        current_file_name: &str,
        backup_file_name: &str,
        backup_id: &str,
    ) -> OpResult {
        let (Some(current), Some(old)) = (
            safe_file_name(Some(current_file_name)),
            safe_file_name(Some(backup_file_name)),
        ) else {
            return OpResult::fail("Недопустимое имя файла.");
        };
        if current.eq_ignore_ascii_case(&old) {
            return self.restore_backup(&old, backup_id);
        }

        let _guard = self.gate.lock().unwrap();
        let Some(source) = self.find_backup(&old, backup_id) else {
            return OpResult::fail("Такой копии нет.");
        };
        let current_enabled = self.layout.mods_dir.join(&current).is_file();
        let current_disabled = self.layout.disabled_dir.join(&current).is_file();
        if current_enabled && current_disabled {
            return OpResult::fail(
                "Файл с таким именем есть и среди включённых, и среди выключенных: уберите лишнюю копию.",
            );
        }
        let dir = if current_disabled {
            &self.layout.disabled_dir
        } else {
            &self.layout.mods_dir
        };
        let target = dir.join(&old);
        if self.layout.mods_dir.join(&old).exists() || self.layout.disabled_dir.join(&old).exists()
        {
            return OpResult::fail("Файл с именем той версии уже есть в папке.");
        }

        let mut moved: Option<PathBuf> = None;
        if current_enabled || current_disabled {
            match self.backup_file(&dir.join(&current), &current) {
                Ok(path) => moved = Some(path),
                Err(error) => return OpResult::fail(io_message(&error)),
            }
        }
        if let Err(error) = fs::create_dir_all(dir) {
            self.restore_moved(&moved, &dir.join(&current));
            return OpResult::fail(io_message(&error));
        }
        if let Err(error) = move_file(&source, &target) {
            self.restore_moved(&moved, &dir.join(&current));
            return OpResult::fail(io_message(&error));
        }
        self.rotate(&current);
        self.rotate(&old);
        OpResult::ok("Возвращено.", Some(target))
    }

    pub fn deleted_mods(&self) -> Vec<DeletedMod> {
        let Ok(entries) = fs::read_dir(long_path(&self.layout.backup_dir)) else {
            return Vec::new();
        };
        let mut result = Vec::new();
        for entry in entries.filter_map(Result::ok) {
            if !entry.path().is_dir() {
                continue;
            }
            let raw = entry.file_name().to_string_lossy().into_owned();
            let Some(name) = safe_file_name(Some(&raw)) else {
                continue;
            };
            if self.layout.mods_dir.join(&name).exists()
                || self.layout.disabled_dir.join(&name).exists()
            {
                continue;
            }
            let list = self.list_backups(&name);
            let Some(latest) = list.first() else {
                continue;
            };
            result.push(DeletedMod {
                file_name: name,
                copies: list.len(),
                last_saved_at: latest.saved_at,
                last_size_bytes: latest.size_bytes,
            });
        }
        result.sort_by(|a, b| {
            b.last_saved_at
                .cmp(&a.last_saved_at)
                .then_with(|| a.file_name.to_lowercase().cmp(&b.file_name.to_lowercase()))
        });
        result
    }

    pub fn cleanup_stale_temp(&self, older_than: std::time::Duration) -> usize {
        let Ok(entries) = fs::read_dir(&self.layout.mods_dir) else {
            return 0;
        };
        // Возраст считается по тем же часам, что и метки копий: так поведение проверяется подменяемыми
        // часами и не зависит от системного времени (в legacy было ровно так же).
        let now = (self.now)();
        let threshold = chrono::Duration::from_std(older_than).unwrap_or(chrono::Duration::MAX);
        let mut removed = 0;
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') || !name.ends_with(".part") {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let modified = DateTime::<Utc>::from(modified);
            if now.signed_duration_since(modified) >= threshold
                && fs::remove_file(entry.path()).is_ok()
            {
                removed += 1;
            }
        }
        removed
    }

    fn check_target(&self, name: &str, options: &InstallOptions) -> Option<InstallResult> {
        let in_disabled = self.layout.disabled_dir.join(name).is_file();
        let in_mods = self.layout.mods_dir.join(name).is_file();
        if in_disabled && in_mods {
            return Some(InstallResult::fail(
                InstallError::AlreadyExists,
                "Файл с таким именем есть и среди включённых, и среди выключенных: уберите лишнюю копию.",
            ));
        }
        if (in_disabled || in_mods) && !options.replace_existing {
            return Some(InstallResult::fail(
                InstallError::AlreadyExists,
                "Такой файл уже установлен.",
            ));
        }
        None
    }

    fn target_path(&self, name: &str) -> PathBuf {
        if self.layout.disabled_dir.join(name).exists() {
            self.layout.disabled_dir.join(name)
        } else {
            self.layout.mods_dir.join(name)
        }
    }

    fn backup_file(&self, path: &Path, name: &str) -> io::Result<PathBuf> {
        let dir = long_path(&self.layout.backup_dir).join(name);
        fs::create_dir_all(&dir)?;
        let stamp = (self.now)().format("%Y%m%d-%H%M%S-%3f");
        let mut sequence = next_sequence(&dir);
        // Место копии резервируем через create_new: два экземпляра программы
        // не должны затереть копию друг друга одинаковым номером.
        let mut destination = None;
        for _ in 0..1024 {
            let candidate = dir.join(format!("{sequence:010}_{stamp}_{name}"));
            match File::options()
                .write(true)
                .create_new(true)
                .open(&candidate)
            {
                Ok(file) => {
                    drop(file);
                    destination = Some(candidate);
                    break;
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => sequence += 1,
                Err(error) => return Err(error),
            }
        }
        let Some(destination) = destination else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "не осталось свободных имён для копии",
            ));
        };
        if let Err(error) = move_file(path, &destination) {
            let _ = fs::remove_file(&destination);
            return Err(error);
        }
        Ok(destination)
    }

    fn restore_moved(&self, moved: &Option<PathBuf>, target: &Path) {
        if let Some(moved) = moved
            && !target.exists()
        {
            let _ = move_file(moved, target);
        }
    }

    fn restore_from(&self, name: &str, source: &Path) -> OpResult {
        let target = self.target_path(name);
        let mut moved = None;
        if target.exists() {
            match self.backup_file(&target, name) {
                Ok(path) => moved = Some(path),
                Err(error) => return OpResult::fail(io_message(&error)),
            }
        }
        if let Err(error) = fs::create_dir_all(target.parent().unwrap_or(Path::new("."))) {
            self.restore_moved(&moved, &target);
            return OpResult::fail(io_message(&error));
        }
        if let Err(error) = move_file(source, &target) {
            self.restore_moved(&moved, &target);
            return OpResult::fail(io_message(&error));
        }
        self.rotate(name);
        OpResult::ok("Возвращено.", Some(target))
    }

    fn find_backup(&self, name: &str, backup_id: &str) -> Option<PathBuf> {
        if backup_id.is_empty() {
            return None;
        }
        self.backups(name).into_iter().find(|path| {
            path.file_name()
                .map(|file| file.to_string_lossy() == backup_id)
                .unwrap_or(false)
        })
    }

    fn rotate(&self, name: &str) {
        let keep = (self.keep_backups)().max(1);
        let all = self.backups(name);
        if all.len() <= keep {
            return;
        }
        for old in all.iter().take(all.len() - keep) {
            let _ = fs::remove_file(old);
        }
    }
}

fn copy_with_hash(
    source: &Path,
    destination: &Path,
    max_bytes: u64,
) -> Result<String, (InstallError, String)> {
    let mut input = File::open(source).map_err(|error| io_failure(&error))?;
    let mut output = File::create(destination).map_err(|error| io_failure(&error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 81920];
    let mut total = 0u64;
    loop {
        let read = input
            .read(&mut buffer)
            .map_err(|error| io_failure(&error))?;
        if read == 0 {
            break;
        }
        total += read as u64;
        if total > max_bytes {
            return Err((
                InstallError::TooLarge,
                "Файл больше допустимого размера, загрузка прервана.".to_string(),
            ));
        }
        output
            .write_all(&buffer[..read])
            .map_err(|error| io_failure(&error))?;
        hasher.update(&buffer[..read]);
    }
    output.sync_all().map_err(|error| io_failure(&error))?;
    Ok(hex_lower(&hasher.finalize()))
}

impl InstallResult {
    fn fail_from_io(error: io::Error) -> Self {
        let (kind, message) = io_failure(&error);
        Self::fail(kind, message)
    }
}

const IN_USE_MESSAGE: &str =
    "Файл занят другой программой (возможно, запущена игра). Закройте игру и повторите.";
const DISK_FULL_MESSAGE: &str = "На диске не хватает места. Освободите место и повторите.";

fn io_failure(error: &io::Error) -> (InstallError, String) {
    if is_sharing_violation(error) {
        (InstallError::InUse, IN_USE_MESSAGE.to_string())
    } else if is_disk_full(error) {
        (InstallError::Io, DISK_FULL_MESSAGE.to_string())
    } else {
        (InstallError::Io, error.to_string())
    }
}

/// Перенос файла. `rename` не работает между дисками (моды на D:, бэкапы в
/// `%LOCALAPPDATA%`); там копируем и удаляем исходник с откатом при ошибке.
fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(error) if matches!(error.raw_os_error(), Some(17) | Some(18)) => {
            if let Err(error) = fs::copy(from, to) {
                let _ = fs::remove_file(to);
                return Err(error);
            }
            if let Err(error) = fs::remove_file(from) {
                let _ = fs::remove_file(to);
                return Err(error);
            }
            Ok(())
        }
        Err(error) => Err(error),
    }
}

fn io_message(error: &io::Error) -> String {
    if is_sharing_violation(error) {
        IN_USE_MESSAGE.to_string()
    } else if is_disk_full(error) {
        DISK_FULL_MESSAGE.to_string()
    } else {
        format!("Не получилось изменить файл: {error}")
    }
}

/// ERROR_HANDLE_DISK_FULL (39) и ERROR_DISK_FULL (112) у Windows, ENOSPC у остальных.
fn is_disk_full(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::StorageFull
        || (cfg!(windows) && matches!(error.raw_os_error(), Some(39) | Some(112)))
}

fn is_sharing_violation(error: &io::Error) -> bool {
    cfg!(windows) && matches!(error.raw_os_error(), Some(32) | Some(33))
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Имя временного файла установки. Часы Windows идут шагом 100 нс: без счётчика две
/// установки, начатые одновременно, писали бы в один и тот же `.part`.
fn unique_suffix() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!(
        "{}-{nanos}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Windows режет пути длиннее 260 символов, если приложение не объявлено
/// longPathAware. Копия лежит в папке с полным именем мода, поэтому внутри
/// папки копий переходим на verbatim-путь.
#[cfg(windows)]
fn long_path(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if text.starts_with(r"\\?\") {
        return path.to_path_buf();
    }
    if let Some(rest) = text.strip_prefix(r"\\") {
        return PathBuf::from(format!(r"\\?\UNC\{rest}"));
    }
    if path.is_absolute() {
        return PathBuf::from(format!(r"\\?\{}", path.display()));
    }
    path.to_path_buf()
}

#[cfg(not(windows))]
fn long_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

fn next_sequence(dir: &Path) -> i64 {
    let mut max = 0;
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().into_owned();
            let digits = name.chars().take_while(char::is_ascii_digit).count();
            if (1..=15).contains(&digits)
                && let Ok(value) = name[..digits].parse::<i64>()
            {
                max = max.max(value);
            }
        }
    }
    max + 1
}

fn is_backup_of(file_name: &str, name: &str) -> bool {
    let digits = file_name.chars().take_while(char::is_ascii_digit).count();
    digits > 0
        && digits <= 15
        && file_name.len() > digits + 1 + name.len()
        && file_name.as_bytes().get(digits) == Some(&b'_')
        && file_name
            .to_lowercase()
            .ends_with(&format!("_{}", name.to_lowercase()))
}

fn parse_backup(path: &Path, mod_name: &str) -> Option<BackupEntry> {
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    if !is_backup_of(&file_name, mod_name) {
        return None;
    }
    let digits = file_name.chars().take_while(char::is_ascii_digit).count();
    let sequence = file_name[..digits].parse::<i64>().ok()?;
    const STAMP_LEN: usize = 19;
    let mut saved_at = None;
    if file_name.len() >= digits + 1 + STAMP_LEN
        && let Ok(naive) = NaiveDateTime::parse_from_str(
            &file_name[digits + 1..digits + 1 + STAMP_LEN],
            "%Y%m%d-%H%M%S-%3f",
        )
    {
        saved_at = Some(Utc.from_utc_datetime(&naive).fixed_offset());
    }
    let size_bytes = fs::metadata(path).ok()?.len();
    Some(BackupEntry {
        id: file_name,
        file_name: mod_name.to_string(),
        sequence,
        saved_at,
        size_bytes,
    })
}

#[cfg(test)]
mod tests {

    #[test]
    fn disk_full_and_busy_file_have_plain_messages() {
        let full = io::Error::from(io::ErrorKind::StorageFull);
        assert_eq!(io_failure(&full).1, DISK_FULL_MESSAGE);
        assert!(io_message(&full).contains("не хватает места"));
        let busy = io::Error::from_raw_os_error(32);
        if cfg!(windows) {
            assert_eq!(io_failure(&busy).0, InstallError::InUse);
        }
        let other = io::Error::other("сбой");
        assert_eq!(io_failure(&other).1, "сбой");
    }
    use super::*;
    use crate::scan::ModLayout;
    use std::io::Write;

    /// Хеши — ключи папок, кэша и проверка архивов: формат не должен меняться
    /// при обновлении sha2.
    #[test]
    fn sha256_hex_is_stable() {
        assert_eq!(
            hex_lower(&Sha256::digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let mut hasher = Sha256::new();
        hasher.update(b"a");
        hasher.update(b"bc");
        assert_eq!(
            hex_lower(&hasher.finalize()),
            hex_lower(&Sha256::digest(b"abc"))
        );
    }

    fn layout(temp: &tempfile::TempDir) -> ModLayout {
        let root = temp.path();
        let mods = root.join("mods");
        let layout = ModLayout {
            mods_dir: mods.clone(),
            disabled_dir: root.join("disabled"),
            backup_dir: root.join("backups"),
        };
        fs::create_dir_all(&layout.mods_dir).unwrap();
        fs::create_dir_all(&layout.disabled_dir).unwrap();
        fs::create_dir_all(&layout.backup_dir).unwrap();
        layout
    }

    fn installer(temp: &tempfile::TempDir) -> ModInstaller {
        installer_with(temp, Arc::new(|| Utc::now().fixed_offset()), 3)
    }

    fn installer_with(temp: &tempfile::TempDir, now: Clock, keep: usize) -> ModInstaller {
        ModInstaller::new(layout(temp), now, Arc::new(move || keep))
    }

    fn zip(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn zip_stored(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap();
    }

    fn source_zip(temp: &tempfile::TempDir) -> PathBuf {
        source_zip_sized(temp, "source", 1)
    }

    fn source_zip_sized(temp: &tempfile::TempDir, tag: &str, size: usize) -> PathBuf {
        let path = temp.path().join(format!("{tag}.zip"));
        let data = vec![b'x'; size];
        zip_stored(
            &path,
            &[
                ("vehicles/car/info.json", b"{}"),
                ("vehicles/car/main.jbeam", data.as_slice()),
            ],
        );
        path
    }

    fn part_files(installer: &ModInstaller) -> Vec<String> {
        fs::read_dir(&installer.layout().mods_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".part"))
            .collect()
    }

    fn set_modified(path: &Path, at: DateTime<FixedOffset>) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(SystemTime::from(at)).unwrap();
    }

    #[test]
    fn installs_and_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        let result = installer.install_from_path(&source, "car.zip", InstallOptions::default());
        assert!(result.success, "{:?}", result.message);
        assert_eq!(
            result.path,
            Some(installer.layout().mods_dir.join("car.zip"))
        );
        assert_eq!(result.sha256.as_ref().unwrap().len(), 64);
        let expected = hex_lower(&Sha256::digest(fs::read(&source).unwrap()));
        assert_eq!(result.sha256.as_deref(), Some(expected.as_str()));
        assert!(installer.layout().mods_dir.join("car.zip").is_file());
        assert!(
            !fs::read_dir(&installer.layout().mods_dir)
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().ends_with(".part"))
        );
    }

    #[test]
    fn blocks_dangerous_paths_and_bad_names() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let bad = temp.path().join("bad.zip");
        zip(&bad, &[("../evil.lua", b"x")]);
        let result = installer.install_from_path(&bad, "bad.zip", InstallOptions::default());
        assert_eq!(result.error, Some(InstallError::Blocked));
        assert!(
            fs::read_dir(&installer.layout().mods_dir)
                .unwrap()
                .next()
                .is_none()
        );
        let result = installer.install_from_path(&bad, "CON.zip", InstallOptions::default());
        assert_eq!(result.error, Some(InstallError::BadName));
    }

    #[test]
    fn refuses_symlink_zip() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = temp.path().join("link.zip");
        let file = File::create(&source).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "vehicles/car/a.jbeam",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer
            .add_symlink(
                "link",
                "../outside.txt",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.finish().unwrap();

        let result = installer.install_from_path(&source, "link.zip", InstallOptions::default());

        assert_eq!(result.error, Some(InstallError::Blocked));
        assert!(result.inspection.as_ref().is_some_and(|inspection| {
            inspection
                .findings
                .iter()
                .any(|finding| finding.code == "symlink")
        }));
        assert!(!installer.layout().mods_dir.join("link.zip").exists());
    }

    #[test]
    fn mod_with_windows_scripts_installs_when_the_user_chose_it() {
        // Maserati GranTurismo (Flanje): tools/*.bat для Spotify. Решение — за пользователем.
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = temp.path().join("Maserati_Granturismo.zip");
        zip(
            &source,
            &[
                ("vehicles/maserati/a.jbeam", b"x"),
                ("tools/Start_Spotify_Remote.bat", b"x"),
            ],
        );
        let refused = installer.install_from_path(
            &source,
            "Maserati_Granturismo.zip",
            InstallOptions::default(),
        );
        assert_eq!(
            refused.error,
            Some(InstallError::HasWarnings),
            "без согласия не ставим"
        );
        let result = installer.install_from_path(
            &source,
            "Maserati_Granturismo.zip",
            InstallOptions {
                allow_warnings: true,
                ..InstallOptions::default()
            },
        );
        assert_eq!(result.error, None);
        assert!(
            installer
                .layout()
                .mods_dir
                .join("Maserati_Granturismo.zip")
                .is_file()
        );
    }

    #[test]
    fn refuses_duplicates_and_allows_replace() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        assert!(
            installer
                .install_from_path(&source, "car.zip", InstallOptions::default())
                .success
        );
        let again = installer.install_from_path(&source, "car.zip", InstallOptions::default());
        assert_eq!(again.error, Some(InstallError::AlreadyExists));
        let replace = installer.install_from_path(
            &source,
            "car.zip",
            InstallOptions {
                replace_existing: true,
                ..InstallOptions::default()
            },
        );
        assert!(replace.success, "{:?}", replace.message);
        assert_eq!(installer.backups("car.zip").len(), 1);
    }

    #[test]
    fn disable_enable_remove_and_unremove() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        installer.install_from_path(&source, "car.zip", InstallOptions::default());
        assert!(installer.disable("car.zip").success);
        assert!(installer.layout().disabled_dir.join("car.zip").is_file());
        assert!(installer.enable("car.zip").success);

        let removed = installer.remove("car.zip");
        assert!(removed.success);
        let backup_id = removed
            .path
            .as_ref()
            .unwrap()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert!(!installer.layout().mods_dir.join("car.zip").exists());
        assert!(installer.unremove("car.zip", &backup_id, false).success);
        assert!(installer.layout().mods_dir.join("car.zip").is_file());
    }

    #[test]
    fn backup_rotation_keeps_newest() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        for _ in 0..5 {
            installer.install_from_path(
                &source,
                "car.zip",
                InstallOptions {
                    replace_existing: true,
                    ..InstallOptions::default()
                },
            );
        }
        assert_eq!(installer.backups("car.zip").len(), 3);
        assert_eq!(installer.list_backups("car.zip").len(), 3);
    }

    #[test]
    fn update_removes_old_and_keeps_disabled_state() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        installer.install_from_path(&source, "car_v1.zip", InstallOptions::default());
        installer.disable("car_v1.zip");
        let update = installer.update_from_path(
            "car_v1.zip",
            &source,
            "car_v2.zip",
            InstallOptions::default(),
        );
        assert!(update.install.success, "{:?}", update.install.message);
        assert!(update.old_removed);
        assert!(!update.was_enabled);
        assert!(installer.layout().disabled_dir.join("car_v2.zip").is_file());
        assert!(!installer.layout().mods_dir.join("car_v1.zip").exists());
    }

    #[test]
    fn restores_specific_version_and_lists_deleted() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        installer.install_from_path(&source, "car_v1.zip", InstallOptions::default());
        installer.update_from_path(
            "car_v1.zip",
            &source,
            "car_v2.zip",
            InstallOptions::default(),
        );
        let versions = installer.list_backups("car_v1.zip");
        assert_eq!(versions.len(), 1);
        let restored = installer.restore_version("car_v2.zip", "car_v1.zip", &versions[0].id);
        assert!(restored.success, "{}", restored.message);
        assert!(installer.layout().mods_dir.join("car_v1.zip").is_file());
        assert!(!installer.layout().mods_dir.join("car_v2.zip").exists());

        let deleted = installer.deleted_mods();
        assert!(deleted.iter().any(|item| item.file_name == "car_v2.zip"));
    }

    #[test]
    fn update_to_taken_name_refuses_and_keeps_old() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let v1 = source_zip_sized(&temp, "v1", 40);
        let v2 = source_zip_sized(&temp, "v2", 80);
        assert!(
            installer
                .install_from_path(&v1, "car_v1.zip", InstallOptions::default())
                .success
        );
        assert!(
            installer
                .install_from_path(&v2, "car_v2.zip", InstallOptions::default())
                .success
        );
        let old_path = installer.layout().mods_dir.join("car_v1.zip");
        let taken_path = installer.layout().mods_dir.join("car_v2.zip");
        let old_bytes = fs::read(&old_path).unwrap();
        let taken_bytes = fs::read(&taken_path).unwrap();

        let update =
            installer.update_from_path("car_v1.zip", &v2, "car_v2.zip", InstallOptions::default());
        assert!(!update.install.success);
        assert_eq!(update.install.error, Some(InstallError::AlreadyExists));
        assert_eq!(
            update.install.message.as_deref(),
            Some("Файл с именем новой версии уже есть в папке.")
        );
        assert!(!update.old_removed);
        assert_eq!(fs::read(&old_path).unwrap(), old_bytes, "старый файл цел");
        assert_eq!(fs::read(&taken_path).unwrap(), taken_bytes);
        assert!(installer.backups("car_v1.zip").is_empty());
        assert!(part_files(&installer).is_empty());

        // Занятое имя может лежать и среди выключенных — отказ тот же.
        assert!(installer.disable("car_v2.zip").success);
        let update =
            installer.update_from_path("car_v1.zip", &v2, "car_v2.zip", InstallOptions::default());
        assert_eq!(update.install.error, Some(InstallError::AlreadyExists));
        assert!(!update.old_removed);
        assert!(old_path.is_file());
        assert!(installer.layout().disabled_dir.join("car_v2.zip").is_file());
        assert!(installer.backups("car_v1.zip").is_empty());
    }

    #[test]
    fn install_replaces_disabled_copy_without_duplicating() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let small = source_zip_sized(&temp, "small", 10);
        let large = source_zip_sized(&temp, "large", 300);
        assert!(
            installer
                .install_from_path(&small, "car.zip", InstallOptions::default())
                .success
        );
        assert!(installer.disable("car.zip").success);

        let denied = installer.install_from_path(&small, "car.zip", InstallOptions::default());
        assert_eq!(denied.error, Some(InstallError::AlreadyExists));
        assert!(!installer.layout().mods_dir.join("car.zip").exists());

        let replaced = installer.install_from_path(
            &large,
            "car.zip",
            InstallOptions {
                replace_existing: true,
                ..InstallOptions::default()
            },
        );
        assert!(replaced.success, "{:?}", replaced.message);
        assert_eq!(
            replaced.path,
            Some(installer.layout().disabled_dir.join("car.zip"))
        );
        assert!(!installer.layout().mods_dir.join("car.zip").exists());
        assert_eq!(
            fs::metadata(installer.layout().disabled_dir.join("car.zip"))
                .unwrap()
                .len(),
            fs::metadata(&large).unwrap().len()
        );
        assert_eq!(installer.backups("car.zip").len(), 1);
        assert!(installer.enable("car.zip").success);
    }

    #[test]
    fn install_refuses_when_name_is_in_mods_and_disabled() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        assert!(
            installer
                .install_from_path(&source, "car.zip", InstallOptions::default())
                .success
        );
        let enabled = installer.layout().mods_dir.join("car.zip");
        let disabled = installer.layout().disabled_dir.join("car.zip");
        fs::copy(&enabled, &disabled).unwrap();
        let before = fs::read(&enabled).unwrap();

        let result = installer.install_from_path(
            &source,
            "car.zip",
            InstallOptions {
                replace_existing: true,
                ..InstallOptions::default()
            },
        );
        assert_eq!(result.error, Some(InstallError::AlreadyExists));
        assert!(
            result
                .message
                .as_deref()
                .unwrap()
                .contains("и среди включённых, и среди выключенных")
        );
        assert_eq!(fs::read(&enabled).unwrap(), before);
        assert!(disabled.is_file());
        assert!(installer.backups("car.zip").is_empty());

        // Включение тоже отказывает и ничего не удаляет.
        assert!(!installer.enable("car.zip").success);
        assert!(enabled.is_file());
        assert!(disabled.is_file());
        assert!(part_files(&installer).is_empty());
    }

    #[test]
    fn restore_backup_keeps_disabled_state() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let small = source_zip_sized(&temp, "small", 10);
        let large = source_zip_sized(&temp, "large", 300);
        assert!(
            installer
                .install_from_path(&small, "car.zip", InstallOptions::default())
                .success
        );
        assert!(
            installer
                .install_from_path(
                    &large,
                    "car.zip",
                    InstallOptions {
                        replace_existing: true,
                        ..InstallOptions::default()
                    },
                )
                .success
        );
        assert!(installer.disable("car.zip").success);

        let backup = installer.list_backups("car.zip").first().cloned().unwrap();
        assert_eq!(backup.size_bytes, fs::metadata(&small).unwrap().len());
        let restored = installer.restore_backup("car.zip", &backup.id);
        assert!(restored.success, "{}", restored.message);

        assert!(!installer.layout().mods_dir.join("car.zip").exists());
        assert_eq!(
            fs::metadata(installer.layout().disabled_dir.join("car.zip"))
                .unwrap()
                .len(),
            fs::metadata(&small).unwrap().len(),
            "вернулась именно выключенная копия"
        );
        let remaining = installer.list_backups("car.zip");
        assert_eq!(remaining.len(), 1);
        assert_eq!(
            remaining[0].size_bytes,
            fs::metadata(&large).unwrap().len(),
            "текущая версия ушла в копии"
        );
    }

    #[test]
    fn backup_rotation_keeps_at_least_one_for_zero_and_one() {
        for keep in [0usize, 1] {
            let temp = tempfile::tempdir().unwrap();
            let installer = installer_with(&temp, Arc::new(|| Utc::now().fixed_offset()), keep);
            let source = source_zip_sized(&temp, "car", 20);
            assert!(
                installer
                    .install_from_path(&source, "car.zip", InstallOptions::default())
                    .success
            );
            for _ in 0..4 {
                assert!(
                    installer
                        .install_from_path(
                            &source,
                            "car.zip",
                            InstallOptions {
                                replace_existing: true,
                                ..InstallOptions::default()
                            },
                        )
                        .success
                );
            }
            assert_eq!(installer.backups("car.zip").len(), 1, "keep={keep}");
            let removed = installer.remove("car.zip");
            assert!(removed.success, "{}", removed.message);
            assert_eq!(
                installer.backups("car.zip").len(),
                1,
                "удаление не стирает единственную копию: keep={keep}"
            );
            assert!(installer.restore_latest_backup("car.zip").success);
            assert!(installer.layout().mods_dir.join("car.zip").is_file());

            // Восстановление при лимите 1 тоже не теряет ни файл, ни единственную копию.
            assert!(
                installer
                    .install_from_path(
                        &source,
                        "car.zip",
                        InstallOptions {
                            replace_existing: true,
                            ..InstallOptions::default()
                        },
                    )
                    .success
            );
            let restored = installer.restore_latest_backup("car.zip");
            assert!(restored.success, "{}", restored.message);
            assert!(installer.layout().mods_dir.join("car.zip").is_file());
            assert_eq!(installer.backups("car.zip").len(), 1, "keep={keep}");
            assert!(part_files(&installer).is_empty());
        }
    }

    #[test]
    fn rotation_with_frozen_clock_orders_backups_by_sequence() {
        let temp = tempfile::tempdir().unwrap();
        let frozen = Utc
            .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let installer = installer_with(&temp, Arc::new(move || frozen), 2);
        for size in [10usize, 11, 12, 13] {
            let source = source_zip_sized(&temp, &format!("car-{size}"), size);
            let options = if size == 10 {
                InstallOptions::default()
            } else {
                InstallOptions {
                    replace_existing: true,
                    ..InstallOptions::default()
                }
            };
            assert!(
                installer
                    .install_from_path(&source, "car.zip", options)
                    .success,
                "установка версии {size}"
            );
        }

        let backups = installer.list_backups("car.zip");
        assert_eq!(backups.len(), 2);
        assert!(
            backups.iter().all(|entry| entry.saved_at == Some(frozen)),
            "часы стояли, время в именах одинаковое"
        );
        // list_backups отдаёт новые первыми: старая копия меньше новой.
        assert!(backups[1].size_bytes < backups[0].size_bytes);
        let newest_backup_size = backups[0].size_bytes;
        let restored = installer.restore_latest_backup("car.zip");
        assert!(restored.success, "{}", restored.message);
        assert_eq!(
            fs::metadata(installer.layout().mods_dir.join("car.zip"))
                .unwrap()
                .len(),
            newest_backup_size,
            "вернулась самая новая копия, а не предыдущая"
        );
    }

    #[test]
    fn cleanup_stale_temp_removes_only_old_part_files() {
        let temp = tempfile::tempdir().unwrap();
        let clock_value = Utc
            .with_ymd_and_hms(2026, 10, 13, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        let installer = installer_with(&temp, Arc::new(move || clock_value), 3);
        let mods = installer.layout().mods_dir.clone();
        let old_part = mods.join(".old.part");
        let fresh_part = mods.join(".fresh.part");
        let old_mod = mods.join("old.zip");
        let old_other = mods.join("notes.txt");
        for path in [&old_part, &fresh_part, &old_mod, &old_other] {
            fs::write(path, b"x").unwrap();
        }
        set_modified(&old_part, clock_value - chrono::Duration::hours(5));
        set_modified(&fresh_part, clock_value);
        set_modified(&old_mod, clock_value - chrono::Duration::hours(5));
        set_modified(&old_other, clock_value - chrono::Duration::hours(5));

        let removed = installer.cleanup_stale_temp(std::time::Duration::from_secs(60 * 60));
        assert_eq!(removed, 1);
        assert!(!old_part.exists(), "старый .part убран");
        assert!(fresh_part.exists(), "свежий .part не тронут");
        assert!(old_mod.exists(), "обычный архив не тронут");
        assert!(old_other.exists(), "обычный файл не тронут");
    }

    #[test]
    fn failed_install_leaves_no_part_and_keeps_installed_mod() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        assert!(
            installer
                .install_from_path(&source, "car.zip", InstallOptions::default())
                .success
        );
        let target = installer.layout().mods_dir.join("car.zip");
        let before = fs::read(&target).unwrap();
        let replace = || InstallOptions {
            replace_existing: true,
            ..InstallOptions::default()
        };

        let broken = temp.path().join("broken.zip");
        fs::write(&broken, [0x50, 0x4B, 0x03, 0x04, 1, 2, 3]).unwrap();
        let blocked = installer.install_from_path(&broken, "car.zip", replace());
        assert_eq!(blocked.error, Some(InstallError::Blocked));
        assert_eq!(fs::read(&target).unwrap(), before);
        assert!(installer.backups("car.zip").is_empty());
        assert!(part_files(&installer).is_empty());

        let empty = temp.path().join("empty.zip");
        zip(&empty, &[] as &[(&str, &[u8])]);
        let warned = installer.install_from_path(&empty, "car.zip", replace());
        assert_eq!(warned.error, Some(InstallError::HasWarnings));
        assert_eq!(fs::read(&target).unwrap(), before);
        assert!(installer.backups("car.zip").is_empty());
        assert!(part_files(&installer).is_empty());

        let update =
            installer.update_from_path("car.zip", &broken, "car_v2.zip", InstallOptions::default());
        assert!(!update.install.success);
        assert_eq!(fs::read(&target).unwrap(), before);
        assert!(!installer.layout().mods_dir.join("car_v2.zip").exists());
        assert!(installer.backups("car.zip").is_empty());
        assert!(part_files(&installer).is_empty());

        // Пользователь может разрешить предупреждения: пустой архив тогда заменяет мод.
        let allowed = installer.install_from_path(
            &empty,
            "car.zip",
            InstallOptions {
                allow_warnings: true,
                replace_existing: true,
                ..InstallOptions::default()
            },
        );
        assert!(allowed.success, "{:?}", allowed.message);
        assert_eq!(
            fs::metadata(&target).unwrap().len(),
            fs::metadata(&empty).unwrap().len()
        );
        assert_eq!(installer.backups("car.zip").len(), 1);
        assert!(part_files(&installer).is_empty());
    }

    #[test]
    fn update_rolls_back_new_version_when_old_cannot_be_removed() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let v1 = source_zip_sized(&temp, "v1", 30);
        let v2 = source_zip_sized(&temp, "v2", 90);
        assert!(
            installer
                .install_from_path(&v1, "audi_v1.0.zip", InstallOptions::default())
                .success
        );
        // Папка копий занята файлом: убрать старую версию в копии нельзя (на Windows то же даёт
        // файл, занятый игрой).
        fs::remove_dir_all(&installer.layout().backup_dir).unwrap();
        fs::write(&installer.layout().backup_dir, b"not a folder").unwrap();

        let update = installer.update_from_path(
            "audi_v1.0.zip",
            &v2,
            "audi_v1.5.zip",
            InstallOptions::default(),
        );
        assert!(!update.install.success);
        assert_eq!(update.install.error, Some(InstallError::InUse));
        assert!(!update.old_removed);
        assert!(installer.layout().mods_dir.join("audi_v1.0.zip").is_file());
        assert!(!installer.layout().mods_dir.join("audi_v1.5.zip").exists());
    }

    #[test]
    fn update_rejects_bad_names_and_installs_when_old_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);

        let bad_old = installer.update_from_path(
            "bad:name.zip",
            &source,
            "ok.zip",
            InstallOptions::default(),
        );
        assert_eq!(bad_old.install.error, Some(InstallError::BadName));
        assert!(!bad_old.old_removed);
        let bad_new =
            installer.update_from_path("ok.zip", &source, "readme.txt", InstallOptions::default());
        assert_eq!(bad_new.install.error, Some(InstallError::BadName));
        assert!(!installer.layout().mods_dir.join("ok.zip").exists());

        let missing =
            installer.update_from_path("gone.zip", &source, "new.zip", InstallOptions::default());
        assert!(missing.install.success, "{:?}", missing.install.message);
        assert!(!missing.old_removed);
        assert!(installer.layout().mods_dir.join("new.zip").is_file());
    }

    #[test]
    fn disable_reports_error_and_keeps_mod_when_target_is_blocked() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let source = source_zip(&temp);
        assert!(
            installer
                .install_from_path(&source, "car.zip", InstallOptions::default())
                .success
        );
        let disabled = installer.layout().disabled_dir.clone();
        fs::remove_dir_all(&disabled).unwrap();
        fs::write(&disabled, b"not a folder").unwrap();

        let result = installer.disable("car.zip");
        assert!(!result.success);
        assert!(installer.layout().mods_dir.join("car.zip").is_file());
    }

    #[test]
    fn failed_update_under_same_name_does_not_claim_old_was_removed() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let v1 = source_zip(&temp);
        assert!(
            installer
                .install_from_path(&v1, "car.zip", InstallOptions::default())
                .success
        );
        let broken = temp.path().join("broken.zip");
        fs::write(&broken, b"definitely not a zip").unwrap();
        let update =
            installer.update_from_path("car.zip", &broken, "car.zip", InstallOptions::default());
        assert!(!update.install.success);
        assert!(!update.old_removed, "старая версия на месте");
        assert!(installer.layout().mods_dir.join("car.zip").is_file());
    }

    #[test]
    fn simultaneous_installs_do_not_share_a_temporary_file() {
        let temp = tempfile::tempdir().unwrap();
        let installer = Arc::new(installer(&temp));
        let sources: Vec<PathBuf> = (0..8)
            .map(|index| {
                let path = temp.path().join(format!("src{index}.zip"));
                let body = vec![index as u8; 200_000 + index * 1000];
                zip_stored(&path, &[(&format!("vehicles/car{index}/a.jbeam"), &body)]);
                path
            })
            .collect();
        std::thread::scope(|scope| {
            for (index, source) in sources.iter().enumerate() {
                let installer = Arc::clone(&installer);
                scope.spawn(move || {
                    let result = installer.install_from_path(
                        source,
                        &format!("car{index}.zip"),
                        InstallOptions::default(),
                    );
                    assert!(result.success, "{index}: {:?}", result.message);
                });
            }
        });
        for (index, source) in sources.iter().enumerate() {
            let installed = installer.layout().mods_dir.join(format!("car{index}.zip"));
            assert_eq!(
                fs::read(installed).unwrap(),
                fs::read(source).unwrap(),
                "{index}"
            );
        }
        assert!(part_files(&installer).is_empty());
    }

    #[test]
    fn temporary_names_are_unique() {
        let names: std::collections::HashSet<String> =
            (0..10_000).map(|_| unique_suffix()).collect();
        assert_eq!(names.len(), 10_000);
    }

    #[test]
    fn long_mod_name_can_be_removed() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        let long_name = format!("{}.zip", "a".repeat(190));
        let source = source_zip(&temp);
        let installed = installer.install_from_path(&source, &long_name, InstallOptions::default());
        assert!(installed.success, "{:?}", installed.message);

        let removed = installer.remove(&long_name);

        assert!(removed.success, "{}", removed.message);
        assert_eq!(installer.backups(&long_name).len(), 1);
        assert!(!installer.layout().mods_dir.join(&long_name).exists());
    }

    #[test]
    fn backup_does_not_overwrite_a_reserved_name() {
        let temp = tempfile::tempdir().unwrap();
        let fixed = DateTime::parse_from_rfc3339("2026-10-21T12:00:00+00:00").unwrap();
        let installer = installer_with(&temp, Arc::new(move || fixed), 3);
        let name = "car.zip";
        let stamp = fixed.format("%Y%m%d-%H%M%S-%3f");
        let dir = installer.layout().backup_dir.join(name);
        fs::create_dir_all(&dir).unwrap();
        let reserved = dir.join(format!("0000000001_{stamp}_{name}"));
        fs::write(&reserved, b"old").unwrap();
        let source = installer.layout().mods_dir.join(name);
        fs::write(&source, b"new").unwrap();

        let saved = installer.backup_file(&source, name).unwrap();

        assert!(
            saved
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("0000000002_"),
            "{saved:?}"
        );
        assert_eq!(fs::read(&reserved).unwrap(), b"old", "чужая копия цела");
        assert_eq!(fs::read(&saved).unwrap(), b"new");
    }

    #[test]
    fn remove_deletes_both_copies_and_shows_one_deleted_mod() {
        let temp = tempfile::tempdir().unwrap();
        let installer = installer(&temp);
        fs::write(installer.layout().mods_dir.join("car.zip"), b"enabled").unwrap();
        fs::write(installer.layout().disabled_dir.join("car.zip"), b"disabled").unwrap();

        let result = installer.remove("car.zip");

        assert!(result.success, "{}", result.message);
        assert!(!installer.layout().mods_dir.join("car.zip").exists());
        assert!(!installer.layout().disabled_dir.join("car.zip").exists());
        assert_eq!(installer.backups("car.zip").len(), 2);
        let deleted = installer.deleted_mods();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].file_name, "car.zip");
        assert_eq!(deleted[0].copies, 2);
    }
}

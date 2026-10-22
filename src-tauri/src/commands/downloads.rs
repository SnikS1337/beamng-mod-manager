//! Наблюдение за «Загрузками»: готовый архив предлагается к установке.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::state::AppState;

const STABLE_AFTER: Duration = Duration::from_millis(1500);

#[derive(Debug, Serialize, Clone)]
struct DownloadReady {
    path: String,
}

struct Scanner {
    seen: HashSet<PathBuf>,
    pending: HashMap<PathBuf, (u64, Instant)>,
    watched: HashSet<PathBuf>,
    stable_after: Duration,
}

impl Scanner {
    fn new(stable_after: Duration) -> Self {
        Self {
            seen: HashSet::new(),
            pending: HashMap::new(),
            watched: HashSet::new(),
            stable_after,
        }
    }

    fn scan(&mut self, folders: &[PathBuf], now: Instant) -> Vec<PathBuf> {
        let mut ready = Vec::new();
        for folder in folders {
            if self.watched.insert(folder.clone()) {
                self.remember_existing(folder);
            }
            let Ok(entries) = std::fs::read_dir(folder) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if !is_candidate(&path) || self.seen.contains(&path) {
                    continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let size = metadata.len();
                match self.pending.get(&path) {
                    Some((previous, since)) if *previous == size => {
                        if now.duration_since(*since) >= self.stable_after {
                            self.pending.remove(&path);
                            self.seen.insert(path.clone());
                            ready.push(path);
                        }
                    }
                    _ => {
                        self.pending.insert(path, (size, now));
                    }
                }
            }
        }
        self.watched.retain(|folder| folders.contains(folder));
        self.pending.retain(|path, _| path.is_file());
        self.seen.retain(|path| path.is_file());
        ready
    }

    fn remember_existing(&mut self, folder: &Path) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if is_candidate(&path) {
                self.seen.insert(path);
            }
        }
    }
}

pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        let mut scanner = Scanner::new(STABLE_AFTER);
        loop {
            std::thread::sleep(Duration::from_millis(600));
            let settings = state.settings.load().settings;
            let folders = watch_folders(&settings);
            for path in scanner.scan(&folders, Instant::now()) {
                let _ = app.emit(
                    "incoming://download",
                    DownloadReady {
                        path: path.display().to_string(),
                    },
                );
            }
        }
    });
}

fn watch_folders(settings: &bmm_domain::settings::AppSettings) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = Vec::new();
    if settings.allow_downloads {
        if let Some(downloads) = downloads_folder() {
            folders.push(downloads);
        }
    }
    for folder in &settings.watch_folders {
        folders.push(PathBuf::from(folder));
    }
    let mut seen = HashSet::new();
    folders
        .retain(|folder| folder.is_dir() && seen.insert(folder.to_string_lossy().to_lowercase()));
    folders
}

fn is_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
        return false;
    };
    if name.starts_with('.') {
        return false;
    }
    let lower = name.to_lowercase();
    [".zip", ".rar", ".7z"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

#[cfg(windows)]
fn downloads_folder() -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath};

    let mut raw: *mut u16 = std::ptr::null_mut();
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Downloads, 0, std::ptr::null_mut(), &mut raw) };
    if result < 0 || raw.is_null() {
        return fallback_downloads();
    }
    let length = unsafe {
        let mut length = 0usize;
        while *raw.add(length) != 0 {
            length += 1;
        }
        length
    };
    let value = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(raw, length) });
    unsafe { CoTaskMemFree(raw as *const core::ffi::c_void) };
    let path = PathBuf::from(value);
    path.is_dir().then_some(path).or_else(fallback_downloads)
}

#[cfg(not(windows))]
fn downloads_folder() -> Option<PathBuf> {
    fallback_downloads()
}

fn fallback_downloads() -> Option<PathBuf> {
    let profile = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let downloads = profile.join("Downloads");
    downloads.is_dir().then_some(downloads)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_only_stable_new_archives() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("car.zip");
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        assert!(scanner.scan(&[temp.path().to_path_buf()], start).is_empty());
        std::fs::write(&path, b"one").unwrap();
        assert!(
            scanner
                .scan(
                    &[temp.path().to_path_buf()],
                    start + Duration::from_millis(10)
                )
                .is_empty(),
            "новый файл: ждём стабильности"
        );
        std::fs::write(&path, b"one plus").unwrap();
        assert!(
            scanner
                .scan(
                    &[temp.path().to_path_buf()],
                    start + Duration::from_millis(20)
                )
                .is_empty(),
            "файл изменился — ждём ещё"
        );
        assert_eq!(
            scanner.scan(
                &[temp.path().to_path_buf()],
                start + Duration::from_millis(30)
            ),
            vec![path.clone()]
        );
        assert!(
            scanner
                .scan(&[temp.path().to_path_buf()], start + Duration::from_secs(1))
                .is_empty(),
            "второй раз не предлагаем"
        );
    }

    #[test]
    fn existing_archives_are_not_offered_on_start() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("old.zip"), b"old").unwrap();
        let mut scanner = Scanner::new(Duration::ZERO);
        assert!(
            scanner
                .scan(&[temp.path().to_path_buf()], Instant::now())
                .is_empty()
        );
        assert!(
            scanner
                .scan(
                    &[temp.path().to_path_buf()],
                    Instant::now() + Duration::from_secs(5)
                )
                .is_empty()
        );
    }

    #[test]
    fn ignores_other_files_and_missing_folders() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("photo.jpg"), b"x").unwrap();
        std::fs::write(temp.path().join(".hidden.zip"), b"x").unwrap();
        std::fs::write(temp.path().join("archive.zip.crdownload"), b"x").unwrap();
        let mut scanner = Scanner::new(Duration::ZERO);
        let missing = temp.path().join("missing");
        assert!(
            scanner
                .scan(&[temp.path().to_path_buf(), missing], Instant::now())
                .is_empty()
        );
    }

    #[cfg(windows)]
    #[test]
    fn downloads_folder_exists_or_falls_back() {
        if let Some(path) = downloads_folder() {
            assert!(path.is_dir());
        }
    }
}

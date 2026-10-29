use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, FixedOffset};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
static QUARANTINE_COUNTER: AtomicU64 = AtomicU64::new(0);
const WRITE_ATTEMPTS: u32 = 5;
const STALE_TEMP: Duration = Duration::from_secs(60 * 60);

pub fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

pub fn unique_temp_path(path: &Path) -> PathBuf {
    let serial = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut name: OsString = path.as_os_str().to_owned();
    name.push(format!(".{}.{serial}.tmp", std::process::id()));
    PathBuf::from(name)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with_temp(path, &unique_temp_path(path), bytes)
}

pub fn write_atomic_with_temp(path: &Path, tmp: &Path, bytes: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    cleanup_stale_temps(path);

    let mut attempt = 0u32;
    loop {
        let result = (|| {
            let mut file = File::create(tmp)?;
            file.write_all(bytes)?;
            file.sync_all()?;
            drop(file);
            fs::rename(tmp, path)
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(error) if attempt + 1 < WRITE_ATTEMPTS && transient(&error) => {
                let _ = fs::remove_file(tmp);
                std::thread::sleep(Duration::from_millis(10 << attempt));
                attempt += 1;
            }
            Err(error) => {
                let _ = fs::remove_file(tmp);
                return Err(error);
            }
        }
    }
}

/// Антивирус, индексатор или второй экземпляр могут кратко держать файл:
/// ERROR_ACCESS_DENIED (5), ERROR_SHARING_VIOLATION (32), ERROR_LOCK_VIOLATION (33).
fn transient(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists | io::ErrorKind::WouldBlock
    ) || matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

/// Убрать временные файлы, оставшиеся после падения процесса. Час — заведомо
/// больше любой живой записи, поэтому активные файлы не задеваются.
pub fn cleanup_stale_temps(path: &Path) {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return;
    };
    let Some(name) = path.file_name() else {
        return;
    };
    let prefix = format!("{}.", name.to_string_lossy());
    let Ok(entries) = fs::read_dir(parent) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.starts_with(&prefix) || !file_name.ends_with(".tmp") {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
            continue;
        };
        if now.duration_since(modified).unwrap_or_default() >= STALE_TEMP {
            let _ = fs::remove_file(entry.path());
        }
    }
}

pub fn quarantine_file(path: &Path, now: DateTime<FixedOffset>) -> Option<PathBuf> {
    for _ in 0..1024 {
        let serial = QUARANTINE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut name = path.as_os_str().to_owned();
        name.push(format!(
            ".bad-{}-{:03}-{}-{serial}",
            now.format("%Y%m%d-%H%M%S"),
            now.timestamp_subsec_millis(),
            std::process::id()
        ));
        let target = PathBuf::from(name);
        if target.exists() {
            continue;
        }
        match fs::rename(path, &target) {
            Ok(()) => return Some(target),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moment() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-21T12:00:00+00:00").unwrap()
    }

    fn temp_names(dir: &Path) -> Vec<String> {
        fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect()
    }

    #[test]
    fn replaces_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(
            temp_names(dir.path()).is_empty(),
            "{:?}",
            temp_names(dir.path())
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn stale_temps_are_removed_and_fresh_kept() {
        use filetime::{FileTime, set_file_mtime};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"old").unwrap();
        let stale = unique_temp_path(&path);
        let fresh = unique_temp_path(&path);
        fs::write(&stale, b"stale").unwrap();
        fs::write(&fresh, b"fresh").unwrap();
        let old = FileTime::from_unix_time(946_684_800, 0);
        set_file_mtime(&stale, old).unwrap();
        cleanup_stale_temps(&path);
        assert!(!stale.exists());
        assert!(fresh.exists());
    }

    #[test]
    fn transient_errors_are_retried_by_kind_and_windows_code() {
        assert!(transient(&io::Error::from(io::ErrorKind::PermissionDenied)));
        assert!(transient(&io::Error::from(io::ErrorKind::WouldBlock)));
        assert!(transient(&io::Error::from_raw_os_error(32)));
        assert!(transient(&io::Error::from_raw_os_error(33)));
        assert!(!transient(&io::Error::from(io::ErrorKind::NotFound)));
    }

    #[test]
    fn failed_write_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        fs::write(&blocker, b"file").unwrap();
        let path = blocker.join("settings.json");
        assert!(write_atomic(&path, b"x").is_err());
        assert!(!temp_path(&path).exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn temp_names_are_unique() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let first = unique_temp_path(&path);
        let second = unique_temp_path(&path);
        assert_ne!(first, second);
        assert!(!first.exists());
        assert!(!second.exists());
    }

    #[test]
    fn parallel_writes_leave_one_complete_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("data.bin");
        let payloads: Vec<Vec<u8>> = (0..8u8)
            .map(|index| vec![b'a' + index; 32 * 1024])
            .collect();
        std::thread::scope(|scope| {
            for payload in &payloads {
                let path = &path;
                scope.spawn(move || write_atomic(path, payload).unwrap());
            }
        });
        let result = fs::read(&path).unwrap();
        assert!(payloads.iter().any(|payload| payload == &result));
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn quarantine_keeps_both_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"first").unwrap();
        let first = quarantine_file(&path, moment()).unwrap();
        fs::write(&path, b"second").unwrap();
        let second = quarantine_file(&path, moment()).unwrap();
        assert_ne!(first, second);
        assert_eq!(fs::read(&first).unwrap(), b"first");
        assert_eq!(fs::read(&second).unwrap(), b"second");
        assert!(!path.exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }
}

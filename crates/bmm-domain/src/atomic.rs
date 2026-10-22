use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use chrono::{DateTime, FixedOffset};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);
static QUARANTINE_COUNTER: AtomicU64 = AtomicU64::new(0);

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

    let result = (|| {
        let mut file = File::create(tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(tmp, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(tmp);
    }

    result
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

    #[test]
    fn replaces_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(!temp_path(&path).exists());
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
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

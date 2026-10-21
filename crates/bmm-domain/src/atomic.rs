use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn temp_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".tmp");
    PathBuf::from(name)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with_temp(path, &temp_path(path), bytes)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_file_and_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"old").unwrap();
        write_atomic(&path, b"new").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new");
        assert!(!temp_path(&path).exists());
    }

    #[test]
    fn failed_write_leaves_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("blocker");
        fs::write(&blocker, b"file").unwrap();
        let path = blocker.join("settings.json");
        assert!(write_atomic(&path, b"x").is_err());
        assert!(!temp_path(&path).exists());
    }
}

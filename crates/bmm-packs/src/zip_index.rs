use std::fs::File;
use std::path::Path;

use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub raw_name: String,
    pub size: u64,
    pub depth: usize,
}

#[derive(Debug, Clone, Default)]
pub struct ZipIndex {
    pub entries: Vec<Entry>,
}

#[derive(Debug, thiserror::Error)]
pub enum ZipError {
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Zip(#[from] zip::result::ZipError),
}

pub fn read_index(path: &Path) -> Result<ZipIndex, ZipError> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let entry = archive.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        let raw_name = entry.name()?.into_owned();
        let Some(name) = normalize_name(&raw_name) else {
            continue;
        };
        let depth = name.split('/').count();
        entries.push(Entry {
            name,
            raw_name,
            size: entry.size(),
            depth,
        });
    }
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    Ok(ZipIndex { entries })
}

pub fn normalize_name(raw: &str) -> Option<String> {
    let normalized = raw.replace('\\', "/");
    let parts: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() || parts.contains(&"..") {
        return None;
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn normalizes_and_rejects_traversal() {
        assert_eq!(
            normalize_name("/vehicles/car/a.jpg").as_deref(),
            Some("vehicles/car/a.jpg")
        );
        assert_eq!(
            normalize_name("\\vehicles\\car\\a.jpg").as_deref(),
            Some("vehicles/car/a.jpg")
        );
        assert_eq!(normalize_name("../evil.jpg"), None);
        assert_eq!(normalize_name(""), None);
    }

    #[test]
    fn reads_zip_index() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("vehicles/car/default.jpg", options).unwrap();
        zip.write_all(b"x").unwrap();
        zip.start_file("vehicles/car/info.json", options).unwrap();
        zip.write_all(b"{}").unwrap();
        zip.finish().unwrap();

        let index = read_index(&path).unwrap();
        assert_eq!(index.entries.len(), 2);
        assert_eq!(index.entries[0].name, "vehicles/car/default.jpg");
        assert_eq!(index.entries[0].depth, 3);
    }
}

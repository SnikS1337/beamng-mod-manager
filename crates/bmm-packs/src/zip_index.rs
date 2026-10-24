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

/// Открывает zip-архив с диска.
///
/// По умолчанию библиотека zip ищет начало архива «на всякий случай» по всему файлу
/// (вдруг перед ним приклеен установщик) — на моде в полгигабайта это 4–8 секунд на
/// каждое открытие. Моды почти всегда начинаются с начала файла: пробуем так, а если
/// не вышло — ищем по-старому.
pub fn open_archive(path: &Path) -> Result<ZipArchive<File>, ZipError> {
    open_with(|| File::open(path))
}

fn open_with<R: std::io::Read + std::io::Seek>(
    open: impl Fn() -> std::io::Result<R>,
) -> Result<ZipArchive<R>, ZipError> {
    let fast = zip::read::Config {
        archive_offset: zip::read::ArchiveOffset::Known(0),
    };
    match ZipArchive::with_config(fast, open()?) {
        Ok(archive) => Ok(archive),
        Err(_) => Ok(ZipArchive::new(open()?)?),
    }
}

pub fn read_index(path: &Path) -> Result<ZipIndex, ZipError> {
    let mut archive = open_archive(path)?;
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

    /// Читатель, который считает, сколько байт у него попросили.
    struct Counting {
        inner: std::io::Cursor<Vec<u8>>,
        read: std::rc::Rc<std::cell::Cell<usize>>,
    }

    impl std::io::Read for Counting {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            let count = self.inner.read(buf)?;
            self.read.set(self.read.get() + count);
            Ok(count)
        }
    }

    impl std::io::Seek for Counting {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            self.inner.seek(pos)
        }
    }

    fn big_zip(prefix: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let stored = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            for index in 0..40 {
                zip.start_file(format!("vehicles/car/part{index}.dae"), stored)
                    .unwrap();
                zip.write_all(&vec![b'x'; 200_000]).unwrap();
            }
            zip.finish().unwrap();
        }
        let mut bytes = prefix.to_vec();
        bytes.extend(cursor.into_inner());
        bytes
    }

    #[test]
    fn opening_reads_only_the_directory_not_the_whole_file() {
        // Раньше открытие перечитывало архив целиком в поисках его начала: мод в
        // полгигабайта открывался 4–8 секунд, а панель мода — до 20.
        let bytes = big_zip(b"");
        let total = bytes.len();
        let read = std::rc::Rc::new(std::cell::Cell::new(0));
        let archive = open_with(|| {
            Ok(Counting {
                inner: std::io::Cursor::new(bytes.clone()),
                read: read.clone(),
            })
        })
        .unwrap();
        assert_eq!(archive.len(), 40);
        assert!(
            read.get() < total / 20,
            "прочитано {} из {total}",
            read.get()
        );
    }

    #[test]
    fn archive_with_something_glued_in_front_still_opens() {
        // Самораспаковывающийся архив или архив после установщика: начало не с нуля.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("glued.zip");
        std::fs::write(&path, big_zip(b"MZ installer stub ....")).unwrap();
        let index = read_index(&path).unwrap();
        assert_eq!(index.entries.len(), 40);
    }
}

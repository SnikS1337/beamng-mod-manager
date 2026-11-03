//! Отпечаток архива: тот ли это файл, что проверяли.
//!
//! Обычный архив мода ставится прямо из папки загрузок, без копии. Между «Архив
//! проверен» и «Обновить» файл могли удалить, переименовать или подменить другим
//! с тем же именем — тогда поставилось бы то, что никто не проверял. Копировать
//! архивы на гигабайты ради этого дорого; отпечаток — размер и начало с концом
//! файла (в конце ZIP лежит его оглавление) — ловит подмену дёшево.

use std::fs::File;
use std::hash::Hasher;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::time::SystemTime;

/// Сколько байт читать с начала и с конца файла.
const EDGE: u64 = 64 * 1024;

#[derive(Debug, Clone)]
pub struct Fingerprint {
    pub len: u64,
    pub modified: Option<SystemTime>,
    edges: u64,
}

/// Тот же файл — то же содержимое. Время изменения не сравниваем: Windows трогает
/// его и без смены содержимого (метка «скачано из интернета», антивирус), и
/// честный архив считался бы подменённым.
impl PartialEq for Fingerprint {
    fn eq(&self, other: &Self) -> bool {
        self.len == other.len && self.edges == other.edges
    }
}

impl Eq for Fingerprint {}

impl Fingerprint {
    pub fn of(path: &Path) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        if !metadata.is_file() {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "не файл"));
        }
        let len = metadata.len();
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut buffer = vec![0u8; EDGE as usize];
        let head = read_up_to(&mut file, &mut buffer)?;
        hasher.write(&buffer[..head]);
        if len > EDGE {
            // Конец не перекрывает уже прочитанное начало.
            file.seek(SeekFrom::Start((len - EDGE).max(EDGE)))?;
            let tail = read_up_to(&mut file, &mut buffer)?;
            hasher.write(&buffer[..tail]);
        }
        Ok(Self {
            len,
            modified: metadata.modified().ok(),
            edges: hasher.finish(),
        })
    }

    /// Тот же размер и время изменения: переименованный или перенесённый файл.
    /// Содержимое не читаем — старого пути уже нет.
    pub fn same_stamp(&self, len: u64, modified: Option<SystemTime>) -> bool {
        self.len == len && self.modified.is_some() && self.modified == modified
    }
}

fn read_up_to(file: &mut File, buffer: &mut [u8]) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buffer.len() {
        match file.read(&mut buffer[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    Ok(filled)
}

/// Можно ли ставить файл, проверенный с отпечатком `checked`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Same,
    /// Файла по этому пути больше нет: удалили, перенесли или переименовали.
    Missing,
    /// Файл другой: подменили, дописали, перезаписали.
    Changed,
    /// Этот файл не проверяли.
    Unchecked,
}

pub fn verdict(checked: Option<&Fingerprint>, path: &Path) -> Verdict {
    let Some(checked) = checked else {
        return Verdict::Unchecked;
    };
    match Fingerprint::of(path) {
        Ok(now) if &now == checked => Verdict::Same,
        Ok(_) => Verdict::Changed,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Verdict::Missing,
        // Файл есть, но не читается (занят, нет доступа) — проверенным его не считаем.
        Err(_) if path.exists() => Verdict::Changed,
        Err(_) => Verdict::Missing,
    }
}

impl Verdict {
    /// Что сказать человеку, если ставить нельзя.
    pub fn message(self) -> Option<&'static str> {
        match self {
            Verdict::Same => None,
            Verdict::Missing => Some(
                "Архив пропал из папки после проверки — его удалили, перенесли или переименовали. Добавьте его ещё раз.",
            ),
            Verdict::Changed => Some(
                "Архив изменился после проверки — его подменили или перезаписали. Откройте его ещё раз, чтобы проверить заново.",
            ),
            Verdict::Unchecked => Some("Этот архив не проверялся — откройте его ещё раз."),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn same_file_is_the_same() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, b"PK archive");
        let checked = Fingerprint::of(&path).unwrap();
        assert_eq!(verdict(Some(&checked), &path), Verdict::Same);
        assert_eq!(Verdict::Same.message(), None);
    }

    #[test]
    fn deleted_moved_or_renamed_file_is_missing() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, b"PK archive");
        let checked = Fingerprint::of(&path).unwrap();
        std::fs::rename(&path, temp.path().join("renamed.zip")).unwrap();
        assert_eq!(verdict(Some(&checked), &path), Verdict::Missing);
        assert!(Verdict::Missing.message().unwrap().contains("пропал"));
    }

    #[test]
    fn swapped_file_with_the_same_name_is_changed() {
        // Было: после «Архив проверен» файл подменили — ставился непроверенный.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, b"PK checked archive");
        let checked = Fingerprint::of(&path).unwrap();
        write(&path, b"PK another archive!");
        assert_eq!(verdict(Some(&checked), &path), Verdict::Changed);
    }

    #[test]
    fn same_size_and_time_but_other_content_is_changed() {
        // Копия другого файла в Проводнике сохраняет время изменения; размер мог
        // совпасть. Содержимое начала и конца всё равно другое.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, &[1u8; 1000]);
        let checked = Fingerprint::of(&path).unwrap();
        let mut other = vec![1u8; 1000];
        other[999] = 2;
        write(&path, &other);
        let file = File::options().write(true).open(&path).unwrap();
        file.set_modified(checked.modified.unwrap()).unwrap();
        drop(file);
        assert_eq!(verdict(Some(&checked), &path), Verdict::Changed);
    }

    #[test]
    fn change_in_the_tail_of_a_big_archive_is_caught() {
        // Оглавление ZIP — в конце файла: подмена там меняет состав архива.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("big.zip");
        let mut bytes = vec![7u8; (EDGE * 3) as usize];
        write(&path, &bytes);
        let checked = Fingerprint::of(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] = 8;
        write(&path, &bytes);
        let file = File::options().write(true).open(&path).unwrap();
        file.set_modified(checked.modified.unwrap()).unwrap();
        drop(file);
        assert_eq!(verdict(Some(&checked), &path), Verdict::Changed);
    }

    #[test]
    fn new_modification_time_alone_is_the_same_file() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, b"PK archive");
        let checked = Fingerprint::of(&path).unwrap();
        let file = File::options().write(true).open(&path).unwrap();
        file.set_modified(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000))
            .unwrap();
        drop(file);
        assert_eq!(verdict(Some(&checked), &path), Verdict::Same);
    }

    #[test]
    fn grown_file_is_changed() {
        // Архив ещё докачивался во время проверки.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, b"PK part");
        let checked = Fingerprint::of(&path).unwrap();
        write(&path, b"PK part and the rest");
        assert_eq!(verdict(Some(&checked), &path), Verdict::Changed);
    }

    #[test]
    fn unchecked_file_cannot_be_installed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, b"PK");
        assert_eq!(verdict(None, &path), Verdict::Unchecked);
        assert!(Verdict::Unchecked.message().is_some());
    }

    #[test]
    fn folder_in_place_of_the_archive_is_not_the_archive() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, b"PK");
        let checked = Fingerprint::of(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::create_dir(&path).unwrap();
        assert_ne!(verdict(Some(&checked), &path), Verdict::Same);
    }

    #[test]
    fn empty_and_tiny_files_have_fingerprints() {
        let temp = tempfile::tempdir().unwrap();
        let empty = temp.path().join("empty.zip");
        write(&empty, b"");
        let checked = Fingerprint::of(&empty).unwrap();
        assert_eq!(checked.len, 0);
        assert_eq!(verdict(Some(&checked), &empty), Verdict::Same);
    }

    #[test]
    fn renamed_file_keeps_its_stamp() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        write(&path, b"PK archive");
        let checked = Fingerprint::of(&path).unwrap();
        let renamed = temp.path().join("mod (renamed).zip");
        std::fs::rename(&path, &renamed).unwrap();
        let meta = std::fs::metadata(&renamed).unwrap();
        assert!(checked.same_stamp(meta.len(), meta.modified().ok()));
        assert!(!checked.same_stamp(meta.len() + 1, meta.modified().ok()));
        assert!(
            !checked.same_stamp(meta.len(), None),
            "без времени не узнаём"
        );
    }
}

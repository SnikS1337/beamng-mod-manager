//! Структурная проверка ZIP до установки: защита от zip-slip и архивных бомб, поиск
//! исполняемых файлов, подсказки о нетипичной структуре мода. Это эвристика, а не антивирус.

use std::io::{Read, Seek};
use std::path::Path;

use zip::ZipArchive;

/// Насколько серьёзно замечание инспектора.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Block,
}

/// Одно замечание по архиву.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub entry: Option<String>,
}

/// Итог проверки архива.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectionResult {
    pub findings: Vec<Finding>,
    pub entry_count: usize,
    pub total_uncompressed_bytes: u64,
}

impl InspectionResult {
    pub fn is_blocked(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Block)
    }

    pub fn has_warnings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == Severity::Warning)
    }
}

/// Пороги, после которых архив считаем слишком большим или подозрительно сжатым.
#[derive(Debug, Clone, PartialEq)]
pub struct ArchiveLimits {
    pub max_entries: usize,
    pub max_total_uncompressed_bytes: u64,
    /// Подозрительное сжатие считаем только для крупных файлов, чтобы не ловить обычный текст.
    pub ratio_check_min_bytes: u64,
    pub max_compression_ratio: f64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: 50_000,
            max_total_uncompressed_bytes: 20 * 1024 * 1024 * 1024,
            ratio_check_min_bytes: 100 * 1024 * 1024,
            max_compression_ratio: 200.0,
        }
    }
}

/// Исполняемые файлы и скрипты, которых в модах BeamNG не бывает.
const BLOCKED_EXTENSIONS: &[&str] = &[
    ".exe", ".dll", ".bat", ".cmd", ".com", ".scr", ".msi", ".ps1", ".vbs", ".sh", ".jar", ".lnk",
    ".pif", ".reg", ".vbe", ".wsf", ".hta", ".psm1", ".appx", ".msix", ".cpl",
];

/// Папки верхнего уровня, которые ожидает игра.
const KNOWN_ROOTS: &[&str] = &[
    "vehicles",
    "levels",
    "art",
    "lua",
    "scripts",
    "ui",
    "core",
    "settings",
    "camera",
    "trackeditor",
    "gameplay",
];

const MIB: u64 = 1024 * 1024;

const NOT_A_ZIP: &str = "Это не zip-архив или он повреждён.";
const CORRUPTED_ZIP: &str = "Архив повреждён и не читается.";

/// Проверяет ZIP-файл до установки.
pub fn inspect_zip(path: &Path, limits: &ArchiveLimits) -> InspectionResult {
    // Ошибка конструктора: файл не похож на ZIP или его каталог разрушен.
    let archive = match crate::zip_index::open_archive(path) {
        Ok(archive) => archive,
        Err(crate::zip_index::ZipError::Io(_)) => return not_zip(CORRUPTED_ZIP),
        Err(_) => return not_zip(NOT_A_ZIP),
    };

    // Ошибка чтения записей: архив открылся, но обрезан или подделан.
    match inspect_archive(archive, limits) {
        Ok(result) => result,
        Err(_) => not_zip(CORRUPTED_ZIP),
    }
}

/// Папка верхнего уровня, которую ждёт игра (vehicles, levels, art, lua…).
pub const EXECUTABLE_MESSAGE: &str = "Сомнительные файлы: программы или скрипты Windows. Игра их не запускает — ставьте, если доверяете источнику.";

pub fn is_known_root(folder: &str) -> bool {
    KNOWN_ROOTS
        .iter()
        .any(|root| root.eq_ignore_ascii_case(folder))
}

/// Путь внутри архива, который нельзя распаковывать (выход из папки, диск, поток NTFS,
/// управляющие символы).
pub fn is_unsafe_entry_path(name: &str) -> bool {
    is_unsafe_path(&name.replace('\\', "/"))
}

fn inspect_archive<R: Read + Seek>(
    mut archive: ZipArchive<R>,
    limits: &ArchiveLimits,
) -> Result<InspectionResult, zip::result::ZipError> {
    let entry_count = archive.len();

    if entry_count > limits.max_entries {
        return Ok(InspectionResult {
            findings: vec![Finding {
                severity: Severity::Block,
                code: "too-many-entries",
                message: format!(
                    "В архиве {entry_count} файлов, лимит {}.",
                    limits.max_entries
                ),
                entry: None,
            }],
            entry_count,
            total_uncompressed_bytes: 0,
        });
    }

    let mut findings = Vec::new();
    let mut total_uncompressed_bytes: u64 = 0;
    let mut top_level: Vec<String> = Vec::new();
    let mut root_has_files = false;

    for index in 0..entry_count {
        let entry = archive.by_index(index)?;
        let raw_name = entry.name()?.into_owned();
        let name = raw_name.replace('\\', "/");
        let size = entry.size();
        let compressed_size = entry.compressed_size();
        let is_directory = name.ends_with('/');

        if entry.is_symlink() {
            findings.push(Finding {
                severity: Severity::Block,
                code: "symlink",
                message: "В архиве есть символическая ссылка.".to_string(),
                entry: Some(raw_name.clone()),
            });
            continue;
        }

        total_uncompressed_bytes = total_uncompressed_bytes.saturating_add(size);

        if is_unsafe_path(&name) {
            findings.push(Finding {
                severity: Severity::Block,
                code: "zip-slip",
                message: "В архиве есть файл с опасным путём.".to_string(),
                entry: Some(raw_name.clone()),
            });
        }

        // Windows отбрасывает точки и пробелы в конце имени: «run.exe.» и «run.exe » — тот же
        // исполняемый файл. Игра такие файлы не запускает, но бывают и честные: Maserati
        // от Flanje несёт скрипты управления Spotify. Решает пользователь — предупреждаем,
        // а не блокируем; при установке файлы не трогаем.
        if !is_directory && has_blocked_extension(&name) {
            findings.push(Finding {
                severity: Severity::Warning,
                code: "executable",
                message: EXECUTABLE_MESSAGE.to_string(),
                entry: Some(raw_name.clone()),
            });
        }

        if !is_directory
            && size >= limits.ratio_check_min_bytes
            && compressed_size > 0
            && (size as f64 / compressed_size as f64) > limits.max_compression_ratio
        {
            findings.push(Finding {
                severity: Severity::Block,
                code: "compression-bomb",
                message: "Архив сжат подозрительно сильно.".to_string(),
                entry: Some(raw_name.clone()),
            });
        }

        let mut clean = name.as_str();
        while let Some(stripped) = clean.strip_prefix("./") {
            clean = stripped;
        }
        let relative = clean.trim_start_matches('/');
        let first = relative.split('/').next().unwrap_or("");
        if !first.is_empty()
            && !top_level
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(first))
        {
            top_level.push(first.to_string());
        }
        if !is_directory && !relative.contains('/') {
            root_has_files = true;
        }
    }

    if total_uncompressed_bytes > limits.max_total_uncompressed_bytes {
        findings.push(Finding {
            severity: Severity::Block,
            code: "too-large",
            message: format!(
                "Распакованный размер {} МБ превышает лимит.",
                total_uncompressed_bytes / MIB
            ),
            entry: None,
        });
    }

    add_structure_hints(&mut findings, entry_count, &top_level, root_has_files);

    Ok(InspectionResult {
        findings,
        entry_count,
        total_uncompressed_bytes,
    })
}

/// Путь, который нельзя распаковывать. Имя уже должно быть приведено к прямым слэшам.
fn is_unsafe_path(name: &str) -> bool {
    if name.starts_with('/') {
        return true;
    }
    if name.as_bytes().get(1) == Some(&b':') {
        return true; // C:\...
    }
    // «файл.txt:поток» — альтернативный поток данных NTFS; управляющие символы в путях не бывают
    // честными.
    if name.chars().any(|c| c == ':' || (c as u32) < 32) {
        return true;
    }
    // «..», а также «... » и «.. »: Windows отбрасывает точки и пробелы в конце имени сегмента.
    name.split('/').any(|segment| {
        segment == ".."
            || (segment.chars().count() >= 2 && segment.chars().all(|c| c == '.' || c == ' '))
            || is_windows_device(segment)
    })
}

/// `CON`, `NUL`, `COM1`… Windows открывает как устройство даже с расширением.
fn is_windows_device(segment: &str) -> bool {
    let stem = segment
        .split('.')
        .next()
        .unwrap_or(segment)
        .trim_end_matches(['.', ' '])
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

#[cfg(test)]
mod device_name_tests {
    use super::is_unsafe_path;

    #[test]
    fn windows_devices_are_rejected_at_any_depth() {
        for name in ["CON", "con.txt", "folder/NUL.zip", "a/COM1", "LPT9.log"] {
            assert!(is_unsafe_path(name), "{name}");
        }
        for name in ["console.txt", "COM0", "LPT10", "auxiliary/a.txt"] {
            assert!(!is_unsafe_path(name), "{name}");
        }
    }
}

/// Расширение файла с точкой, без учёта хвостовых точек и пробелов (как Windows).
fn has_blocked_extension(name: &str) -> bool {
    let trimmed = name.trim_end_matches(['.', ' ']);
    let file_name = trimmed.rsplit_once('/').map_or(trimmed, |(_, file)| file);
    let Some(dot) = file_name.rfind('.') else {
        return false;
    };
    let extension = &file_name[dot..];
    BLOCKED_EXTENSIONS
        .iter()
        .any(|blocked| blocked.eq_ignore_ascii_case(extension))
}

fn add_structure_hints(
    findings: &mut Vec<Finding>,
    entry_count: usize,
    top_level: &[String],
    root_has_files: bool,
) {
    if entry_count == 0 {
        findings.push(Finding {
            severity: Severity::Warning,
            code: "empty",
            message: "Архив пуст.".to_string(),
            entry: None,
        });
        return;
    }

    if top_level.iter().any(|folder| is_known_root(folder)) {
        return;
    }

    // Все файлы лежат внутри одной лишней папки: игра такой мод не увидит.
    if top_level.len() == 1 && !root_has_files {
        findings.push(Finding {
            severity: Severity::Warning,
            code: "nested-root",
            message: format!(
                "Все файлы лежат внутри папки «{}»: возможно, игра не увидит мод без перепаковки.",
                top_level[0]
            ),
            entry: None,
        });
        return;
    }

    findings.push(Finding {
        severity: Severity::Warning,
        code: "unusual-structure",
        message:
            "В корне архива нет привычных папок мода (vehicles, levels, art, lua, ui и т. п.)."
                .to_string(),
        entry: None,
    });
}

fn not_zip(message: &'static str) -> InspectionResult {
    InspectionResult {
        findings: vec![Finding {
            severity: Severity::Block,
            code: "not-zip",
            message: message.to_string(),
            entry: None,
        }],
        entry_count: 0,
        total_uncompressed_bytes: 0,
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::io::Write;
    use std::path::PathBuf;

    use tempfile::TempDir;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn zip_with(entries: &[(&str, &[u8])], options: SimpleFileOptions) -> (TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mod.zip");
        let file = File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap();
        (dir, path)
    }

    fn zip(entries: &[(&str, &[u8])]) -> (TempDir, PathBuf) {
        zip_with(entries, SimpleFileOptions::default())
    }

    fn inspect(entries: &[(&str, &[u8])]) -> InspectionResult {
        let (_dir, path) = zip(entries);
        inspect_zip(&path, &ArchiveLimits::default())
    }

    fn has_code(result: &InspectionResult, code: &str) -> bool {
        result.findings.iter().any(|finding| finding.code == code)
    }

    #[test]
    fn defaults_match_legacy_limits() {
        let limits = ArchiveLimits::default();
        assert_eq!(limits.max_entries, 50_000);
        assert_eq!(limits.max_total_uncompressed_bytes, 20 * 1024 * 1024 * 1024);
        assert_eq!(limits.ratio_check_min_bytes, 100 * 1024 * 1024);
        assert_eq!(limits.max_compression_ratio, 200.0);
    }

    #[test]
    fn regular_mod_has_no_findings() {
        let result = inspect(&[
            ("vehicles/foo/info.json", b"{}"),
            ("vehicles/foo/foo.jbeam", b"jbeam"),
            ("art/x.dds", b"dds"),
            ("ui/modules/app/app.js", b"js"),
        ]);

        assert!(!result.is_blocked());
        assert!(!result.has_warnings());
        assert!(result.findings.is_empty());
        assert_eq!(result.entry_count, 4);
        assert_eq!(result.total_uncompressed_bytes, 2 + 5 + 3 + 2);
    }

    #[test]
    fn blocks_zip_slip() {
        for name in [
            "../evil.lua",
            "..\\evil.lua",
            "/etc/evil.lua",
            "C:/evil.lua",
            "C:\\evil.lua",
        ] {
            let result = inspect(&[("vehicles/a.jbeam", b"x"), (name, b"x")]);
            assert!(result.is_blocked(), "{name} must be blocked");
            let finding = result
                .findings
                .iter()
                .find(|finding| finding.code == "zip-slip")
                .unwrap_or_else(|| panic!("{name} must produce a zip-slip finding"));
            assert_eq!(finding.severity, Severity::Block);
            assert_eq!(finding.message, "В архиве есть файл с опасным путём.");
            assert_eq!(finding.entry.as_deref(), Some(name));
        }
    }

    #[test]
    fn blocks_symlink_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("symlink.zip");
        let file = File::create(&path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file("vehicles/car/a.jbeam", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer
            .add_symlink("link", "../outside.txt", SimpleFileOptions::default())
            .unwrap();
        writer.finish().unwrap();

        let result = inspect_zip(&path, &ArchiveLimits::default());

        assert!(result.is_blocked());
        let finding = result
            .findings
            .iter()
            .find(|finding| finding.code == "symlink")
            .expect("symlink finding");
        assert_eq!(finding.severity, Severity::Block);
        assert_eq!(finding.message, "В архиве есть символическая ссылка.");
        assert_eq!(finding.entry.as_deref(), Some("link"));
    }

    #[test]
    fn executables_are_flagged_for_the_user_not_blocked() {
        for name in [
            "vehicles/x/run.exe",
            "vehicles/x/RUN.EXE",
            "vehicles/x/tool.Ps1",
            "vehicles/x/run.exe.",
            "vehicles/x/run.exe ",
            "tools/Start_Spotify_Remote.bat",
            ".exe",
        ] {
            let result = inspect(&[("vehicles/a.jbeam", b"x"), (name, b"x")]);
            assert!(!result.is_blocked(), "{name}: решает пользователь");
            assert!(result.has_warnings(), "{name}: но предупреждаем");
            let finding = result
                .findings
                .iter()
                .find(|finding| finding.code == "executable")
                .unwrap_or_else(|| panic!("{name} must be treated as executable"));
            assert_eq!(finding.severity, Severity::Warning);
            assert_eq!(finding.message, EXECUTABLE_MESSAGE);
            assert_eq!(finding.entry.as_deref(), Some(name));
        }
    }

    #[test]
    fn allows_js_and_lua_scripts() {
        let result = inspect(&[("ui/app.js", b"x"), ("lua/init.lua", b"x")]);

        assert!(!result.is_blocked());
        assert!(result.findings.is_empty());
    }

    #[test]
    fn directory_with_executable_suffix_is_not_blocked() {
        let result = inspect(&[("vehicles/run.exe/", b""), ("vehicles/a.jbeam", b"x")]);

        assert!(!result.is_blocked());
        assert!(!has_code(&result, "executable"));
        assert!(result.findings.is_empty());
    }

    #[test]
    fn warns_about_nested_root() {
        let result = inspect(&[("MyMod/vehicles/a.jbeam", b"x"), ("MyMod/info.json", b"{}")]);

        assert!(!result.is_blocked());
        assert!(result.has_warnings());
        let finding = result
            .findings
            .iter()
            .find(|finding| finding.code == "nested-root")
            .expect("nested-root warning");
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(
            finding.message,
            "Все файлы лежат внутри папки «MyMod»: возможно, игра не увидит мод без перепаковки."
        );
        assert!(finding.entry.is_none());
    }

    #[test]
    fn warns_about_unusual_structure() {
        let result = inspect(&[("readme.txt", b"x"), ("foo/bar.txt", b"x")]);

        assert!(!result.is_blocked());
        let finding = result
            .findings
            .iter()
            .find(|finding| finding.code == "unusual-structure")
            .expect("unusual-structure warning");
        assert_eq!(finding.severity, Severity::Warning);
        assert_eq!(
            finding.message,
            "В корне архива нет привычных папок мода (vehicles, levels, art, lua, ui и т. п.)."
        );
    }

    #[test]
    fn empty_archive_warns() {
        let result = inspect(&[]);

        assert!(!result.is_blocked());
        assert!(result.has_warnings());
        assert_eq!(result.entry_count, 0);
        assert_eq!(result.total_uncompressed_bytes, 0);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].code, "empty");
        assert_eq!(result.findings[0].severity, Severity::Warning);
        assert_eq!(result.findings[0].message, "Архив пуст.");
    }

    #[test]
    fn blocks_not_a_zip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.zip");
        std::fs::write(&path, b"this is definitely not a zip archive").unwrap();

        let result = inspect_zip(&path, &ArchiveLimits::default());

        assert!(result.is_blocked());
        assert_eq!(result.entry_count, 0);
        assert_eq!(result.total_uncompressed_bytes, 0);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].code, "not-zip");
        assert_eq!(result.findings[0].severity, Severity::Block);
        assert_eq!(result.findings[0].message, NOT_A_ZIP);
    }

    #[test]
    fn corrupted_entry_is_reported_without_panic() {
        let (_dir, path) = zip(&[("vehicles/a.jbeam", b"x")]);
        let mut bytes = std::fs::read(&path).unwrap();
        // Каталог архива цел, но локальный заголовок записи испорчен: конструктор проходит,
        // а чтение записи падает.
        let signature = bytes
            .windows(4)
            .position(|window| window == b"PK\x03\x04".as_slice())
            .expect("local file header");
        bytes[signature..signature + 4].copy_from_slice(&[0, 0, 0, 0]);
        std::fs::write(&path, &bytes).unwrap();

        let result = inspect_zip(&path, &ArchiveLimits::default());

        assert!(result.is_blocked());
        assert_eq!(result.entry_count, 0);
        assert_eq!(result.total_uncompressed_bytes, 0);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].code, "not-zip");
        assert_eq!(result.findings[0].severity, Severity::Block);
        assert_eq!(result.findings[0].message, CORRUPTED_ZIP);
    }

    #[test]
    fn blocks_too_many_entries() {
        let (_dir, path) = zip(&[
            ("vehicles/a", b"x"),
            ("vehicles/b", b"x"),
            ("vehicles/c", b"x"),
        ]);
        let limits = ArchiveLimits {
            max_entries: 2,
            ..ArchiveLimits::default()
        };

        let result = inspect_zip(&path, &limits);

        assert!(result.is_blocked());
        assert_eq!(result.entry_count, 3);
        assert_eq!(result.total_uncompressed_bytes, 0);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].code, "too-many-entries");
        assert_eq!(result.findings[0].severity, Severity::Block);
        assert_eq!(result.findings[0].message, "В архиве 3 файлов, лимит 2.");
    }

    #[test]
    fn blocks_too_large_total_size() {
        let data = vec![0u8; 1024 * 1024 + 1];
        let (_dir, path) = zip(&[("vehicles/big.bin", &data)]);
        let limits = ArchiveLimits {
            max_total_uncompressed_bytes: 1024,
            ..ArchiveLimits::default()
        };

        let result = inspect_zip(&path, &limits);

        assert!(result.is_blocked());
        assert_eq!(result.total_uncompressed_bytes, data.len() as u64);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].code, "too-large");
        assert_eq!(result.findings[0].severity, Severity::Block);
        assert_eq!(
            result.findings[0].message,
            "Распакованный размер 1 МБ превышает лимит."
        );
    }

    #[test]
    fn blocks_compression_bomb() {
        let data = vec![0u8; 64 * 1024];
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let (_dir, path) = zip_with(&[("vehicles/bomb.bin", &data)], options);
        let limits = ArchiveLimits {
            ratio_check_min_bytes: 1024,
            max_compression_ratio: 10.0,
            ..ArchiveLimits::default()
        };

        let result = inspect_zip(&path, &limits);

        assert!(result.is_blocked());
        assert_eq!(result.findings.len(), 1);
        let finding = &result.findings[0];
        assert_eq!(finding.code, "compression-bomb");
        assert_eq!(finding.severity, Severity::Block);
        assert_eq!(finding.entry.as_deref(), Some("vehicles/bomb.bin"));
        assert_eq!(finding.message, "Архив сжат подозрительно сильно.");
    }

    #[test]
    fn dot_slash_prefix_is_not_a_structure_warning() {
        let result = inspect(&[("./vehicles/car/a.jbeam", b"x")]);

        assert!(!result.is_blocked());
        assert!(result.findings.is_empty());
        assert_eq!(result.entry_count, 1);
    }

    #[test]
    fn unsafe_paths_allow_single_dot_and_block_dot_dot_segments() {
        assert!(!is_unsafe_entry_path("./"));
        assert!(!is_unsafe_entry_path("./vehicles/a.jbeam"));
        assert!(!is_unsafe_entry_path("vehicles/./a.jbeam"));
        assert!(!is_unsafe_entry_path("vehicles/.hidden/a.jbeam"));

        assert!(is_unsafe_entry_path("vehicles/.. /x"));
        assert!(is_unsafe_entry_path("vehicles/../x"));
        assert!(is_unsafe_entry_path("../x"));
        assert!(is_unsafe_entry_path("..\\x"));
        assert!(is_unsafe_entry_path("vehicles/.../x"));
        assert!(is_unsafe_entry_path("vehicles/. /x"));
        assert!(is_unsafe_entry_path("/etc/passwd"));
        assert!(is_unsafe_entry_path("C:/evil"));
        assert!(is_unsafe_entry_path("C:\\evil"));
        assert!(is_unsafe_entry_path("file.txt:stream"));
        assert!(is_unsafe_entry_path("foo/\u{1}bar"));
    }

    #[test]
    fn known_roots_are_case_insensitive() {
        assert!(is_known_root("vehicles"));
        assert!(is_known_root("Vehicles"));
        assert!(is_known_root("UI"));
        assert!(!is_known_root("mod"));
        assert!(!is_known_root("vehicles/"));
    }
}

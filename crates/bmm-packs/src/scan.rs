use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use bmm_domain::library::DiskMod;

pub const BRIDGE_FILE: &str = "bmm_bridge.zip";
const MAX_DEPTH: usize = 8;
const UNPACKED_DIR: &str = "unpacked";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModLayout {
    pub mods_dir: PathBuf,
    pub disabled_dir: PathBuf,
    pub backup_dir: PathBuf,
}

impl ModLayout {
    pub fn in_app_folder(mods_dir: impl AsRef<Path>, app_folder: impl AsRef<Path>) -> Self {
        let full = std::path::absolute(mods_dir.as_ref())
            .unwrap_or_else(|_| mods_dir.as_ref().to_path_buf());
        let key_text = full
            .to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase();
        let hash = Sha256::digest(key_text.as_bytes());
        let key = to_hex(&hash[..5]);
        let root = app_folder.as_ref().join(key);
        Self {
            mods_dir: mods_dir.as_ref().to_path_buf(),
            disabled_dir: root.join("disabled"),
            backup_dir: root.join("backups"),
        }
    }

    pub fn beside(mods_dir: impl AsRef<Path>) -> Self {
        let full = std::path::absolute(mods_dir.as_ref())
            .unwrap_or_else(|_| mods_dir.as_ref().to_path_buf());
        let root = full
            .parent()
            .map(|parent| parent.join("BeamModManager"))
            .unwrap_or_else(|| full.join("BeamModManager"));
        Self {
            mods_dir: mods_dir.as_ref().to_path_buf(),
            disabled_dir: root.join("disabled"),
            backup_dir: root.join("backups"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub mods: Vec<DiskMod>,
    pub folder_available: bool,
    /// Все каталоги удалось прочитать. Иначе пропавшие моды нельзя считать удалёнными.
    pub complete: bool,
    pub warnings: Vec<String>,
}

impl Default for ScanResult {
    fn default() -> Self {
        Self {
            mods: Vec::new(),
            folder_available: false,
            complete: true,
            warnings: Vec::new(),
        }
    }
}

pub fn scan_mods(layout: &ModLayout) -> ScanResult {
    let mut result = ScanResult::default();
    if !layout.mods_dir.is_dir() {
        result.warnings.push("Папка модов не найдена.".to_string());
        return result;
    }
    result.folder_available = true;

    let mut seen: HashSet<String> = HashSet::new();
    scan_flat(&layout.mods_dir, true, &mut seen, &mut result);
    if layout.disabled_dir.is_dir() {
        scan_flat(&layout.disabled_dir, false, &mut seen, &mut result);
    }
    scan_subfolders(
        &layout.mods_dir,
        &layout.mods_dir,
        1,
        &mut seen,
        &mut result,
    );

    result
        .mods
        .sort_by_key(|item| item.file_name.to_lowercase());
    result
}

/// Симлинк или junction: чужую папку не обходим. `is_symlink` не видит
/// junction, а `fs::metadata` по нему уводит сканер наружу.
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

/// Записи каталога; нечитаемая запись делает обход неполным. Раньше такие записи
/// молча пропускались, обход считался полным, и сверка стирала у «пропавшего» мода
/// страницу, проверки и историю.
fn readable<T>(
    entries: impl Iterator<Item = std::io::Result<T>>,
    result: &mut ScanResult,
) -> Vec<T> {
    let mut ok = Vec::new();
    for entry in entries {
        match entry {
            Ok(entry) => ok.push(entry),
            Err(_) => result.complete = false,
        }
    }
    ok
}

fn scan_flat(dir: &Path, enabled: bool, seen: &mut HashSet<String>, result: &mut ScanResult) {
    let Ok(entries) = fs::read_dir(dir) else {
        result.complete = false;
        return;
    };
    for entry in readable(entries, result) {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            result.complete = false;
            continue;
        };
        if is_reparse(&metadata) || !metadata.is_file() {
            continue;
        }
        let Some(file_name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if !is_zip(&file_name) {
            continue;
        }
        // Помощник менеджера внутри игры — служебный: в гараже его нет ни среди включённых,
        // ни среди выключенных (этот обход тоже заходит в корень mods). В подпапках его оставляем:
        // там он не наш.
        if file_name.eq_ignore_ascii_case(BRIDGE_FILE) {
            continue;
        }
        add_mod(&file_name, enabled, None, &metadata, seen, result);
    }
}

fn scan_subfolders(
    root: &Path,
    dir: &Path,
    depth: usize,
    seen: &mut HashSet<String>,
    result: &mut ScanResult,
) {
    if depth > MAX_DEPTH {
        result.complete = false;
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        result.complete = false;
        return;
    };
    for entry in readable(entries, result) {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            result.complete = false;
            continue;
        };
        if is_reparse(&metadata) {
            continue;
        }
        if metadata.is_dir() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.eq_ignore_ascii_case(UNPACKED_DIR) {
                continue;
            }
            scan_subfolders(root, &path, depth + 1, seen, result);
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let Some(file_name) = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
        else {
            continue;
        };
        if !is_zip(&file_name) {
            continue;
        }
        // Корневой помощник менеджера игра видит как мод, но в гараже его быть не должно
        // (этот обход тоже заходит в корень mods). В подпапках игры он остаётся: там он не наш.
        if file_name.eq_ignore_ascii_case(BRIDGE_FILE) && path.parent() == Some(root) {
            continue;
        }
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let Some(parent) = relative.parent() else {
            continue;
        };
        let sub_folder = parent
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        // Файлы в самом корне mods уже учёл scan_flat. Пропускаем их здесь, иначе
        // каждый мод считался бы «одинаковым именем в разных папках».
        if sub_folder.is_empty() {
            continue;
        }
        add_mod(&file_name, true, Some(sub_folder), &metadata, seen, result);
    }
}

fn add_mod(
    file_name: &str,
    enabled: bool,
    sub_folder: Option<String>,
    metadata: &fs::Metadata,
    seen: &mut HashSet<String>,
    result: &mut ScanResult,
) {
    // Имя, с которым программа не работает (начинается с точки, длиннее 200 знаков):
    // мод виден, но выключить или удалить его нельзя — говорим, что делать.
    if crate::safe_name::safe_file_name(Some(file_name)).as_deref() != Some(file_name) {
        result.warnings.push(format!(
            "Файл «{file_name}» программа не может выключить или удалить: переименуйте его (без точки в начале, короче 200 знаков)."
        ));
    }
    let key = file_name.to_lowercase();
    if !seen.insert(key) {
        result.warnings.push(format!(
            "Одинаковое имя файла в разных папках: {file_name} — в гараже показана одна копия."
        ));
        return;
    }
    let Ok(size_bytes) = i64::try_from(metadata.len()) else {
        result.complete = false;
        return;
    };
    let modified_utc = metadata.modified().ok().map(DateTime::<Utc>::from);
    result.mods.push(DiskMod {
        file_name: file_name.to_string(),
        enabled,
        size_bytes,
        sub_folder,
        modified_utc,
    });
}

fn is_zip(file_name: &str) -> bool {
    Path::new(file_name)
        .extension()
        .map(|extension| extension.to_string_lossy().eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
}

fn to_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(text, "{byte:02x}");
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_file(path: &Path, bytes: &[u8]) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, bytes).unwrap();
    }

    #[test]
    fn scans_root_disabled_subfolders_and_skips_service_files() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        let _layout = ModLayout {
            mods_dir: mods.clone(),
            disabled_dir: temp.path().join("disabled"),
            backup_dir: temp.path().join("backups"),
        };
        // Используем beside, чтобы проверить сам layout.
        let layout = ModLayout::beside(&mods);
        write_file(&mods.join("root.zip"), b"x");
        write_file(&mods.join("bmm_bridge.zip"), b"bridge");
        write_file(&mods.join("notzip.zipx"), b"x");
        write_file(&mods.join("unpacked").join("hidden.zip"), b"x");
        write_file(&mods.join("sub").join("nested.zip"), b"x");
        write_file(&mods.join("sub").join("bmm_bridge.zip"), b"x");
        write_file(&layout.disabled_dir.join("off.zip"), b"x");
        write_file(&mods.join("root.zip"), b"root wins");

        let result = scan_mods(&layout);
        assert!(result.folder_available);
        let names: Vec<_> = result
            .mods
            .iter()
            .map(|item| item.file_name.as_str())
            .collect();
        assert!(names.contains(&"root.zip"));
        assert!(names.contains(&"nested.zip"));
        assert!(names.contains(&"off.zip"));
        assert!(names.contains(&"bmm_bridge.zip")); // только в подпапке
        assert!(!names.contains(&"notzip.zipx"));
        assert!(!names.contains(&"hidden.zip"));
        assert_eq!(result.mods.len(), 4);

        let off = result
            .mods
            .iter()
            .find(|item| item.file_name == "off.zip")
            .unwrap();
        assert!(!off.enabled);
        let nested = result
            .mods
            .iter()
            .find(|item| item.file_name == "nested.zip")
            .unwrap();
        assert_eq!(nested.sub_folder.as_deref(), Some("sub"));
    }

    #[test]
    fn plain_mods_folder_has_no_duplicate_warnings() {
        // Обычная папка: все моды в корне, часть выключена программой. Ни одного
        // «одинакового имени» — раньше корень обходился дважды и предупреждение
        // получал каждый мод.
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        let layout = ModLayout::beside(&mods);
        write_file(&mods.join("a.zip"), b"a");
        write_file(&mods.join("b.zip"), b"b");
        write_file(&layout.disabled_dir.join("off.zip"), b"c");
        write_file(&mods.join("repo").join("c.zip"), b"d");
        let result = scan_mods(&layout);
        assert_eq!(result.mods.len(), 4);
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
    }

    #[test]
    fn duplicate_name_in_subfolder_is_reported() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        let layout = ModLayout::beside(&mods);
        write_file(&mods.join("same.zip"), b"one");
        write_file(&mods.join("sub").join("same.zip"), b"two");
        let result = scan_mods(&layout);
        assert_eq!(result.mods.len(), 1, "в гараже одна карточка");
        assert!(
            result
                .warnings
                .iter()
                .any(|warning| warning.contains("Одинаковое имя")),
            "{:?}",
            result.warnings
        );
    }

    #[test]
    fn unreadable_entry_makes_scan_incomplete() {
        let mut result = ScanResult::default();
        let entries = vec![Ok(1), Err(std::io::Error::other("нет доступа")), Ok(2)];
        assert_eq!(readable(entries.into_iter(), &mut result), vec![1, 2]);
        assert!(!result.complete, "иначе сверка сочла бы мод удалённым");
    }

    #[test]
    fn unmanageable_names_are_reported() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        let layout = ModLayout::beside(&mods);
        write_file(&mods.join(".hidden.zip"), b"x");
        let long = format!("{}.zip", "a".repeat(210));
        write_file(&mods.join(&long), b"x");
        write_file(&mods.join("normal.zip"), b"x");
        let result = scan_mods(&layout);
        let flagged: Vec<&String> = result
            .warnings
            .iter()
            .filter(|warning| warning.contains("переименуйте"))
            .collect();
        assert_eq!(flagged.len(), 2, "{:?}", result.warnings);
    }

    #[test]
    fn missing_folder_is_reported() {
        let layout = ModLayout::beside("C:\\missing");
        let result = scan_mods(&layout);
        assert!(!result.folder_available);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn skips_temp_service_and_lookalike_files() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        let layout = ModLayout {
            mods_dir: mods.clone(),
            disabled_dir: temp.path().join("disabled"),
            backup_dir: temp.path().join("backups"),
        };
        write_file(&mods.join("a.zip"), b"x");
        write_file(&mods.join(".abc.part"), b"x");
        write_file(&mods.join(".older.part"), b"x");
        write_file(&mods.join("b.zipx"), b"x");
        write_file(&mods.join("c.zip.bak"), b"x");
        write_file(&mods.join("d.part"), b"x");
        write_file(&mods.join("bmm_bridge.zip"), b"x");
        write_file(&mods.join("readme.txt"), b"x");
        write_file(&mods.join("sub").join("nested.zip"), b"x");
        write_file(&mods.join("sub").join(".nested.part"), b"x");
        write_file(&layout.disabled_dir.join("off.zip"), b"x");
        write_file(&layout.disabled_dir.join(".off.part"), b"x");
        write_file(&layout.disabled_dir.join("bmm_bridge.zip"), b"x");

        let result = scan_mods(&layout);
        let names: Vec<&str> = result
            .mods
            .iter()
            .map(|item| item.file_name.as_str())
            .collect();
        assert_eq!(names, vec!["a.zip", "nested.zip", "off.zip"]);
    }

    #[test]
    fn junction_is_not_followed() {
        let temp = tempfile::tempdir().unwrap();
        let outside = temp.path().join("outside");
        write_file(&outside.join("foreign.zip"), b"x");
        let mods = temp.path().join("mods");
        write_file(&mods.join("own.zip"), b"x");
        let link = mods.join("link");
        #[cfg(windows)]
        {
            let status = std::process::Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(&link)
                .arg(&outside)
                .status();
            if !matches!(status, Ok(status) if status.success()) {
                return; // среда не разрешает junction — проверка не применима
            }
        }
        // На других системах то же самое делает символическая ссылка на папку.
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let layout = ModLayout {
            mods_dir: mods,
            disabled_dir: temp.path().join("disabled"),
            backup_dir: temp.path().join("backups"),
        };
        let names: Vec<String> = scan_mods(&layout)
            .mods
            .into_iter()
            .map(|item| item.file_name)
            .collect();
        assert_eq!(
            names,
            vec!["own.zip".to_string()],
            "чужой junction не обходим"
        );
    }

    #[test]
    fn app_layout_key_is_stable() {
        let a = ModLayout::in_app_folder("C:\\Games\\BeamNG\\mods", "C:\\Data");
        let b = ModLayout::in_app_folder("c:\\games\\beamng\\mods\\", "C:\\Data");
        assert_eq!(a.disabled_dir, b.disabled_dir);
        assert!(a.disabled_dir.starts_with("C:\\Data"));
    }
}

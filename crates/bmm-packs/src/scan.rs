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

#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    pub mods: Vec<DiskMod>,
    pub folder_available: bool,
    pub warnings: Vec<String>,
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

fn scan_flat(dir: &Path, enabled: bool, seen: &mut HashSet<String>, result: &mut ScanResult) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() || !metadata.is_file() {
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
        if enabled && file_name.eq_ignore_ascii_case(BRIDGE_FILE) {
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
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if metadata.file_type().is_symlink() {
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
        let sub_folder = if sub_folder.is_empty() {
            None
        } else {
            Some(sub_folder)
        };
        add_mod(&file_name, true, sub_folder, &metadata, seen, result);
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
    let key = file_name.to_lowercase();
    if !seen.insert(key) {
        return;
    }
    let Ok(size_bytes) = i64::try_from(metadata.len()) else {
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
    fn missing_folder_is_reported() {
        let layout = ModLayout::beside("C:\\missing");
        let result = scan_mods(&layout);
        assert!(!result.folder_available);
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn app_layout_key_is_stable() {
        let a = ModLayout::in_app_folder("C:\\Games\\BeamNG\\mods", "C:\\Data");
        let b = ModLayout::in_app_folder("c:\\games\\beamng\\mods\\", "C:\\Data");
        assert_eq!(a.disabled_dir, b.disabled_dir);
        assert!(a.disabled_dir.starts_with("C:\\Data"));
    }
}

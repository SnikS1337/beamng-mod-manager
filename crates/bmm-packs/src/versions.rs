use std::collections::HashSet;
use std::fs;

use chrono::{DateTime, FixedOffset};

use bmm_domain::filename;
use bmm_domain::library::{InstalledMod, LibraryStore};
use bmm_domain::text::TitleNormalizer;

use crate::install::{BackupEntry, DeletedMod, ModInstaller, OpResult};

#[derive(Debug, Clone)]
pub struct PastVersion {
    pub backup: BackupEntry,
    pub version: Option<String>,
}

pub fn versions_for(
    installer: &ModInstaller,
    store: &dyn LibraryStore,
    installed: &InstalledMod,
) -> Vec<PastVersion> {
    let mods = store.mods();
    let other_current: HashSet<String> = mods
        .iter()
        .filter(|item| item.id != installed.id)
        .map(|item| item.file_name.to_lowercase())
        .collect();
    let key = name_key(&installed.file_name);
    let mut names: Vec<String> = store.file_history(&installed.id);
    names.push(installed.file_name.clone());
    if !key.is_empty() {
        names.extend(
            installer.backed_up_file_names().into_iter().filter(|name| {
                !other_current.contains(&name.to_lowercase()) && name_key(name) == key
            }),
        );
    }

    let mut seen = HashSet::new();
    let mut result = Vec::new();
    for name in names {
        if !seen.insert(name.to_lowercase()) {
            continue;
        }
        let version = filename::from_file_name(&name).version;
        for backup in installer.list_backups(&name) {
            result.push(PastVersion {
                backup,
                version: version.clone(),
            });
        }
    }
    result.sort_by(|a, b| {
        b.backup
            .saved_at
            .cmp(&a.backup.saved_at)
            .then_with(|| b.backup.sequence.cmp(&a.backup.sequence))
    });
    result
}

pub fn deleted(installer: &ModInstaller, store: &dyn LibraryStore) -> Vec<DeletedMod> {
    let raw = installer.deleted_mods();
    if raw.is_empty() {
        return raw;
    }
    let mut taken = HashSet::new();
    let mut keys = HashSet::new();
    for item in store.mods() {
        taken.insert(item.file_name.to_lowercase());
        for history in store.file_history(&item.id) {
            taken.insert(history.to_lowercase());
        }
        let key = name_key(&item.file_name);
        if !key.is_empty() {
            keys.insert(key);
        }
    }
    raw.into_iter()
        .filter(|item| {
            !taken.contains(&item.file_name.to_lowercase())
                && !keys.contains(&name_key(&item.file_name))
        })
        .collect()
}

pub fn restore_version(
    installer: &ModInstaller,
    store: &dyn LibraryStore,
    installed: &InstalledMod,
    version: &PastVersion,
    _now: DateTime<FixedOffset>,
) -> OpResult {
    let result = installer.restore_version(
        &installed.file_name,
        &version.backup.file_name,
        &version.backup.id,
    );
    if !result.success {
        return result;
    }
    let Some(path) = &result.path else {
        return result;
    };
    let Some(restored_name) = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        return result;
    };
    let guessed = filename::from_file_name(&restored_name);
    let size_bytes = fs::metadata(path).ok().map(|meta| meta.len() as i64);
    let enabled = path.parent() == Some(installer.layout().mods_dir.as_path());
    let mut updated = installed.clone();
    updated.file_name = restored_name;
    updated.version = guessed.version;
    updated.size_bytes = size_bytes;
    updated.enabled = enabled;
    store.upsert_mod(updated);
    store.add_file_history(&installed.id, &installed.file_name);
    result
}

pub fn restore_deleted(
    installer: &ModInstaller,
    store: &dyn LibraryStore,
    deleted: &DeletedMod,
    backup_id: Option<&str>,
    now: DateTime<FixedOffset>,
) -> OpResult {
    let id = match backup_id {
        Some(id) => id.to_string(),
        None => match installer.list_backups(&deleted.file_name).first() {
            Some(entry) => entry.id.clone(),
            None => return OpResult::fail("Копий нет."),
        },
    };
    let result = installer.restore_backup(&deleted.file_name, &id);
    if !result.success {
        return result;
    }
    let Some(path) = &result.path else {
        return result;
    };
    let Some(name) = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
    else {
        return result;
    };
    let guessed = filename::from_file_name(&name);
    let base_id = name.to_lowercase();
    let mut mod_id = base_id.clone();
    let mut suffix = 2;
    while store.get_mod(&mod_id).is_some() {
        mod_id = format!("{base_id}#{suffix}");
        suffix += 1;
    }
    let title = if guessed.name.trim().is_empty() {
        name.strip_suffix(".zip").unwrap_or(&name).to_string()
    } else {
        guessed.name
    };
    store.upsert_mod(InstalledMod {
        id: mod_id,
        name: title,
        file_name: name,
        version: guessed.version,
        size_bytes: fs::metadata(path).ok().map(|meta| meta.len() as i64),
        installed_at_utc: now,
        enabled: true,
    });
    result
}

fn name_key(file_name: &str) -> String {
    TitleNormalizer::normalize(&filename::from_file_name(file_name).name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install::InstallOptions;
    use crate::scan::ModLayout;
    use bmm_domain::library::InMemoryLibraryStore;
    use chrono::{TimeZone, Utc};
    use std::io::Write;
    use std::sync::Arc;

    fn now() -> DateTime<FixedOffset> {
        Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
            .unwrap()
            .fixed_offset()
    }

    fn setup() -> (tempfile::TempDir, ModInstaller) {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        let layout = ModLayout {
            mods_dir: mods,
            disabled_dir: temp.path().join("disabled"),
            backup_dir: temp.path().join("backups"),
        };
        std::fs::create_dir_all(&layout.mods_dir).unwrap();
        std::fs::create_dir_all(&layout.disabled_dir).unwrap();
        std::fs::create_dir_all(&layout.backup_dir).unwrap();
        let installer = ModInstaller::new(layout, Arc::new(now), Arc::new(|| 5));
        (temp, installer)
    }

    fn zip(path: &std::path::Path) {
        let file = std::fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "vehicles/car/a.jbeam",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn lists_versions_and_restores_them_in_library() {
        let (temp, installer) = setup();
        let source = temp.path().join("source.zip");
        zip(&source);
        installer.install_from_path(&source, "car_v1.zip", InstallOptions::default());
        installer.update_from_path(
            "car_v1.zip",
            &source,
            "car_v2.zip",
            InstallOptions::default(),
        );
        let store = InMemoryLibraryStore::new();
        let installed = InstalledMod {
            id: "car_v2.zip".to_string(),
            name: "Car".to_string(),
            file_name: "car_v2.zip".to_string(),
            version: Some("2".to_string()),
            size_bytes: Some(1),
            installed_at_utc: now(),
            enabled: true,
        };
        store.upsert_mod(installed.clone());
        let versions = versions_for(&installer, &store, &installed);
        assert!(!versions.is_empty());
        let restored = restore_version(&installer, &store, &installed, &versions[0], now());
        assert!(restored.success, "{}", restored.message);
        let updated = store.get_mod("car_v2.zip").unwrap();
        assert_eq!(updated.file_name, "car_v1.zip");
        assert_eq!(store.file_history("car_v2.zip"), vec!["car_v2.zip"]);
    }

    #[test]
    fn deleted_excludes_names_known_to_library() {
        let (temp, installer) = setup();
        let source = temp.path().join("source.zip");
        zip(&source);
        installer.install_from_path(&source, "deleted.zip", InstallOptions::default());
        installer.remove("deleted.zip");
        let store = InMemoryLibraryStore::new();
        let raw = deleted(&installer, &store);
        assert!(raw.iter().any(|item| item.file_name == "deleted.zip"));
        store.upsert_mod(InstalledMod {
            id: "deleted.zip".to_string(),
            name: "Deleted".to_string(),
            file_name: "deleted.zip".to_string(),
            version: None,
            size_bytes: Some(1),
            installed_at_utc: now(),
            enabled: true,
        });
        assert!(deleted(&installer, &store).is_empty());
    }

    #[test]
    fn restore_deleted_creates_library_entry() {
        let (temp, installer) = setup();
        let source = temp.path().join("source.zip");
        zip(&source);
        installer.install_from_path(&source, "old.zip", InstallOptions::default());
        installer.remove("old.zip");
        let store = InMemoryLibraryStore::new();
        let item = deleted(&installer, &store).into_iter().next().unwrap();
        let restored = restore_deleted(&installer, &store, &item, None, now());
        assert!(restored.success, "{}", restored.message);
        assert!(store.get_mod("old.zip").is_some());
        assert!(installer.layout().mods_dir.join("old.zip").is_file());
    }
}

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use chrono::{DateTime, FixedOffset};

use crate::filename;
use crate::text::TitleNormalizer;
use crate::versioning::SizeComparer;

use super::{DiskMod, InstalledMod, LibraryStore};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    pub added: Vec<String>,
    pub removed: Vec<String>,
    pub changed: Vec<String>,
    pub renamed: Vec<(String, String)>,
    pub warning: Option<String>,
}

static GATE: Mutex<()> = Mutex::new(());

pub fn reconcile(
    store: &dyn LibraryStore,
    disk: &[DiskMod],
    folder_available: bool,
    now: DateTime<FixedOffset>,
) -> SyncReport {
    let _gate = GATE.lock().unwrap_or_else(|error| error.into_inner());
    let known = store.mods();
    if !folder_available {
        return SyncReport {
            warning: Some("Папка модов недоступна.".to_string()),
            ..SyncReport::default()
        };
    }
    if disk.is_empty() && !known.is_empty() {
        return SyncReport {
            warning: Some(
                "Папка модов пуста, а в библиотеке есть моды: изменения не применены.".to_string(),
            ),
            ..SyncReport::default()
        };
    }

    let mut sorted = disk.to_vec();
    sorted.sort_by_key(|item| std::cmp::Reverse(item.enabled));
    let mut on_disk: HashMap<String, DiskMod> = HashMap::new();
    for item in sorted {
        on_disk.entry(key_of(&item.file_name)).or_insert(item);
    }

    let mut by_file: HashMap<String, InstalledMod> = HashMap::new();
    for item in &known {
        by_file
            .entry(key_of(&item.file_name))
            .or_insert_with(|| item.clone());
    }

    let mut report = SyncReport::default();

    for (key, disk_item) in &on_disk {
        let Some(existing) = by_file.get(key) else {
            continue;
        };
        let mut updated = existing.clone();
        if updated.enabled != disk_item.enabled {
            updated.enabled = disk_item.enabled;
        }
        if updated.size_bytes != Some(disk_item.size_bytes) {
            updated.size_bytes = Some(disk_item.size_bytes);
            updated.version = version_after_external_change(store, existing, disk_item);
        }
        if updated != *existing {
            store.upsert_mod(updated);
            report.changed.push(disk_item.file_name.clone());
        }
    }

    let vanished: Vec<InstalledMod> = known
        .iter()
        .filter(|item| {
            let key = key_of(&item.file_name);
            if !on_disk.contains_key(&key) {
                return true;
            }
            match by_file.get(&key) {
                Some(first) => first.id != item.id,
                None => true,
            }
        })
        .cloned()
        .collect();
    let mut fresh: Vec<DiskMod> = on_disk
        .iter()
        .filter(|(key, _)| !by_file.contains_key(*key))
        .map(|(_, item)| item.clone())
        .collect();
    fresh.sort_by_key(|item| item.file_name.to_lowercase());

    let mut vanished_by_name: HashMap<String, Vec<InstalledMod>> = HashMap::new();
    for item in &vanished {
        vanished_by_name
            .entry(name_key(&item.file_name))
            .or_default()
            .push(item.clone());
    }
    let mut fresh_by_name: HashMap<String, Vec<DiskMod>> = HashMap::new();
    for item in &fresh {
        fresh_by_name
            .entry(name_key(&item.file_name))
            .or_default()
            .push(item.clone());
    }

    let mut handled_vanished: HashSet<String> = HashSet::new();
    let mut handled_fresh: HashSet<String> = HashSet::new();

    for (name, old_items) in &vanished_by_name {
        if name.is_empty() || old_items.len() != 1 {
            continue;
        }
        let Some(new_items) = fresh_by_name.get(name) else {
            continue;
        };
        if new_items.len() != 1 {
            continue;
        }
        let old = &old_items[0];
        let new = &new_items[0];
        migrate_mod(store, old, new);
        report
            .renamed
            .push((old.file_name.clone(), new.file_name.clone()));
        handled_vanished.insert(old.id.clone());
        handled_fresh.insert(key_of(&new.file_name));
    }

    for item in &vanished {
        if handled_vanished.contains(&item.id) {
            continue;
        }
        store.remove_mod(&item.id);
        report.removed.push(item.file_name.clone());
    }

    for item in &fresh {
        if handled_fresh.contains(&key_of(&item.file_name)) {
            continue;
        }
        store.upsert_mod(new_mod(store, item, now));
        report.added.push(item.file_name.clone());
    }

    report
}

fn version_after_external_change(
    store: &dyn LibraryStore,
    installed: &InstalledMod,
    disk: &DiskMod,
) -> Option<String> {
    if let Some(version) = filename::from_file_name(&disk.file_name).version {
        return Some(version);
    }
    for binding in store.bindings() {
        if binding.mod_id != installed.id || !binding.confirmed {
            continue;
        }
        let Some(check) = store.get_check(&installed.id, &binding.source) else {
            continue;
        };
        if let (Some(version), Some(size)) = (check.last_version, check.last_size)
            && !SizeComparer::differs(Some(disk.size_bytes), Some(size))
        {
            return Some(version);
        }
    }
    None
}

fn migrate_mod(store: &dyn LibraryStore, old: &InstalledMod, new: &DiskMod) {
    // Файл просто переименовали (размер тот же) — версия прежняя, если в новом имени нет своей.
    let version = if old.size_bytes == Some(new.size_bytes) {
        filename::from_file_name(&new.file_name)
            .version
            .or_else(|| old.version.clone())
    } else {
        version_after_external_change(store, old, new)
    };
    let updated = InstalledMod {
        file_name: new.file_name.clone(),
        version,
        size_bytes: Some(new.size_bytes),
        enabled: new.enabled,
        ..old.clone()
    };
    store.upsert_mod(updated);
    store.add_file_history(&old.id, &old.file_name);
}

/// Программа сама поставила новую версию мода под другим именем файла (обновление,
/// узнанное по содержимому). Запись библиотеки переезжает на новый файл: страница
/// мода, проверки и история остаются, иначе сверка с папкой приняла бы обновление
/// за удаление старого мода и появление нового. `false` — записи со старым именем нет.
pub fn rename_file(store: &dyn LibraryStore, old_file: &str, new_file: &str, size: i64) -> bool {
    let _gate = GATE.lock().unwrap_or_else(|error| error.into_inner());
    let Some(old) = store
        .mods()
        .into_iter()
        .find(|item| item.file_name.eq_ignore_ascii_case(old_file))
    else {
        return false;
    };
    let disk = DiskMod {
        file_name: new_file.to_string(),
        enabled: old.enabled,
        size_bytes: size,
        sub_folder: None,
        modified_utc: None,
    };
    migrate_mod(store, &old, &disk);
    true
}

fn new_mod(store: &dyn LibraryStore, disk: &DiskMod, now: DateTime<FixedOffset>) -> InstalledMod {
    let guessed = filename::from_file_name(&disk.file_name);
    let base_id = key_of(&disk.file_name);
    let mut id = base_id.clone();
    let mut suffix = 2;
    while store.get_mod(&id).is_some() {
        id = format!("{base_id}#{suffix}");
        suffix += 1;
    }
    let name = if guessed.name.trim().is_empty() {
        disk.file_name
            .strip_suffix(".zip")
            .or_else(|| disk.file_name.strip_suffix(".ZIP"))
            .unwrap_or(&disk.file_name)
            .to_string()
    } else {
        guessed.name
    };
    InstalledMod {
        id,
        name,
        file_name: disk.file_name.clone(),
        version: guessed.version,
        size_bytes: Some(disk.size_bytes),
        installed_at_utc: now,
        enabled: disk.enabled,
    }
}

fn key_of(file_name: &str) -> String {
    file_name.to_lowercase()
}

fn name_key(file_name: &str) -> String {
    TitleNormalizer::normalize(&filename::from_file_name(file_name).name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{CheckState, InMemoryLibraryStore, ModBinding};
    use chrono::{TimeZone, Utc};
    use url::Url;

    fn now() -> DateTime<FixedOffset> {
        Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
            .unwrap()
            .fixed_offset()
    }

    fn bind(store: &InMemoryLibraryStore, item: &InstalledMod, version: &str, size: i64) {
        store
            .upsert_binding(ModBinding {
                mod_id: item.id.clone(),
                source: "modland".to_string(),
                source_id: "x".to_string(),
                url: Url::parse("https://modland.net/x").unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            mod_id: item.id.clone(),
            source: "modland".to_string(),
            last_checked_utc: now(),
            last_changed_utc: None,
            last_version: Some(version.to_string()),
            last_size: Some(size),
            miss_count: 0,
            last_game_version: None,
            last_title: None,
        });
    }

    fn disk(file_name: &str, enabled: bool, size: i64) -> DiskMod {
        DiskMod {
            file_name: file_name.to_string(),
            enabled,
            size_bytes: size,
            sub_folder: None,
            modified_utc: None,
        }
    }

    fn installed(id: &str, file_name: &str, name: &str, size: i64) -> InstalledMod {
        InstalledMod {
            id: id.to_string(),
            name: name.to_string(),
            file_name: file_name.to_string(),
            version: Some("1.0".to_string()),
            size_bytes: Some(size),
            installed_at_utc: now(),
            enabled: true,
        }
    }

    #[test]
    fn adds_new_mods_and_reports_nothing_on_repeat() {
        let store = InMemoryLibraryStore::new();
        let disk = vec![disk("bmw_m3.zip", true, 10)];
        let report = reconcile(&store, &disk, true, now());
        assert_eq!(report.added, vec!["bmw_m3.zip"]);
        assert_eq!(store.mods().len(), 1);
        assert_eq!(store.mods()[0].name, "bmw m3");

        let report = reconcile(&store, &disk, true, now());
        assert!(report.added.is_empty());
        assert!(report.changed.is_empty());
    }

    #[test]
    fn removes_vanished_mods() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a.zip", "a.zip", "A", 10));
        let report = reconcile(&store, &[], true, now());
        assert!(report.warning.is_some());
        assert!(store.mods().len() == 1); // пустая папка — сбой, не удаление

        let report = reconcile(&store, &[disk("b.zip", true, 20)], true, now());
        assert_eq!(report.removed, vec!["a.zip"]);
        assert_eq!(store.mods().len(), 1);
        assert_eq!(store.mods()[0].id, "b.zip");
    }

    #[test]
    fn detects_rename_by_normalized_name() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("bmw_m3_v1.zip", "bmw_m3_v1.zip", "BMW M3", 10));
        let report = reconcile(&store, &[disk("bmw_m3_v2.zip", true, 20)], true, now());
        assert_eq!(
            report.renamed,
            vec![("bmw_m3_v1.zip".to_string(), "bmw_m3_v2.zip".to_string())]
        );
        assert_eq!(store.mods().len(), 1);
        assert_eq!(store.mods()[0].file_name, "bmw_m3_v2.zip");
        assert_eq!(store.mods()[0].name, "BMW M3");
        assert_eq!(store.file_history("bmw_m3_v1.zip"), vec!["bmw_m3_v1.zip"]);
    }

    #[test]
    fn ambiguous_rename_is_not_guessed() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a1.zip", "a1.zip", "Same", 10));
        store.upsert_mod(installed("a2.zip", "a2.zip", "Same", 10));
        let report = reconcile(
            &store,
            &[disk("b.zip", true, 20), disk("c.zip", true, 20)],
            true,
            now(),
        );
        assert!(report.renamed.is_empty());
        assert_eq!(report.removed.len(), 2);
        assert_eq!(report.added.len(), 2);
    }

    #[test]
    fn disabled_copy_wins_by_enabled_flag() {
        let store = InMemoryLibraryStore::new();
        let disk = vec![disk("same.zip", false, 10), disk("same.zip", true, 10)];
        reconcile(&store, &disk, true, now());
        assert_eq!(store.mods().len(), 1);
        assert!(store.mods()[0].enabled);
    }

    #[test]
    fn case_only_name_change_is_not_a_change() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("bmw.zip", "bmw.zip", "BMW", 10));
        let report = reconcile(&store, &[disk("BMW.zip", true, 10)], true, now());
        assert!(report.changed.is_empty());
        assert!(report.removed.is_empty());
        assert_eq!(store.mods().len(), 1);
    }

    #[test]
    fn enabled_follows_disk_both_ways() {
        let store = InMemoryLibraryStore::new();
        reconcile(&store, &[disk("a.zip", true, 10)], true, now());
        assert!(store.mods()[0].enabled);
        reconcile(&store, &[disk("a.zip", false, 10)], true, now());
        assert!(!store.mods()[0].enabled);
        reconcile(&store, &[disk("a.zip", true, 10)], true, now());
        assert!(store.mods()[0].enabled);
    }

    #[test]
    fn removed_file_drops_bindings_and_checks() {
        let store = InMemoryLibraryStore::new();
        reconcile(
            &store,
            &[disk("a.zip", true, 10), disk("b.zip", true, 20)],
            true,
            now(),
        );
        let a = store
            .mods()
            .into_iter()
            .find(|item| item.file_name == "a.zip")
            .unwrap();
        bind(&store, &a, "1.0", 10);
        assert_eq!(store.bindings().len(), 1);
        assert_eq!(store.checks().len(), 1);

        let report = reconcile(&store, &[disk("b.zip", true, 20)], true, now());
        assert_eq!(report.removed, vec!["a.zip"]);
        assert_eq!(store.mods().len(), 1);
        assert!(store.bindings().is_empty());
        assert!(store.checks().is_empty());
    }

    #[test]
    fn replaced_file_with_site_size_takes_site_version() {
        let store = InMemoryLibraryStore::new();
        reconcile(&store, &[disk("audi.zip", true, 1000)], true, now());
        let original = store.mods().into_iter().next().unwrap();
        store.upsert_mod(InstalledMod {
            version: Some("1.0".to_string()),
            ..original.clone()
        });
        bind(&store, &original, "1.5", 1300);

        let report = reconcile(&store, &[disk("audi.zip", true, 1300)], true, now());
        assert_eq!(report.changed, vec!["audi.zip"]);
        let updated = store.mods().into_iter().next().unwrap();
        assert_eq!(updated.version.as_deref(), Some("1.5"));
        assert_eq!(updated.installed_at_utc, original.installed_at_utc);
    }

    #[test]
    fn replaced_file_with_other_size_loses_stale_version() {
        let store = InMemoryLibraryStore::new();
        reconcile(&store, &[disk("audi.zip", true, 1000)], true, now());
        let original = store.mods().into_iter().next().unwrap();
        store.upsert_mod(InstalledMod {
            version: Some("9.9".to_string()),
            ..original.clone()
        });
        bind(&store, &original, "1.5", 5000);

        let report = reconcile(&store, &[disk("audi.zip", true, 2000)], true, now());
        assert_eq!(report.changed, vec!["audi.zip"]);
        let updated = store.mods().into_iter().next().unwrap();
        assert_eq!(updated.version, None);
        assert_eq!(updated.installed_at_utc, original.installed_at_utc);
    }

    #[test]
    fn id_collision_keeps_both_mods() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("b.zip", "other.zip", "Other", 1));
        let report = reconcile(
            &store,
            &[disk("other.zip", true, 1), disk("b.zip", true, 1)],
            true,
            now(),
        );
        assert!(report.added.contains(&"b.zip".to_string()));
        assert_eq!(store.mods().len(), 2);
        let ids: HashSet<String> = store.mods().into_iter().map(|item| item.id).collect();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn parallel_reconcile_does_not_duplicate_entries() {
        let store = InMemoryLibraryStore::new();
        let disk: Vec<DiskMod> = (0..50)
            .map(|index| disk(&format!("m{index}_v1.{index}.zip"), true, 100 + index))
            .collect();
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    for _ in 0..5 {
                        reconcile(&store, &disk, true, now());
                    }
                });
            }
        });
        assert_eq!(store.mods().len(), 50);
        let keys: HashSet<String> = store
            .mods()
            .into_iter()
            .map(|item| item.file_name.to_lowercase())
            .collect();
        assert_eq!(keys.len(), 50);
    }

    #[test]
    fn renamed_update_takes_version_from_tracked_page_by_size() {
        // Скачали новую версию с сайта: в имени файла версии нет, но размер совпал
        // с тем, что программа видела на странице мода, — значит, это та версия.
        let store = InMemoryLibraryStore::new();
        let old = installed("rr", "rolls_royce.zip", "rolls royce", 100);
        store.upsert_mod(old.clone());
        bind(&store, &old, "1.3", 250);
        let report = reconcile(&store, &[disk("Rolls Royce.zip", true, 250)], true, now());
        assert_eq!(report.renamed.len(), 1, "{report:?}");
        let after = store.get_mod("rr").unwrap();
        assert_eq!(after.version.as_deref(), Some("1.3"));
    }

    #[test]
    fn moved_record_keeps_id_bindings_and_history() {
        let store = InMemoryLibraryStore::new();
        let old = installed("acadia", "acadia.zip", "acadia", 100);
        store.upsert_mod(old.clone());
        bind(&store, &old, "2.0", 300);
        assert!(rename_file(
            &store,
            "ACADIA.zip",
            "GMC_Acadia_GMT968.zip",
            300
        ));
        let report = reconcile(
            &store,
            &[disk("GMC_Acadia_GMT968.zip", true, 300)],
            true,
            now(),
        );
        assert!(
            report.added.is_empty() && report.removed.is_empty(),
            "{report:?}"
        );
        let after = store.get_mod("acadia").unwrap();
        assert_eq!(after.file_name, "GMC_Acadia_GMT968.zip");
        assert_eq!(after.version.as_deref(), Some("2.0"));
        assert_eq!(store.bindings().len(), 1);
        assert_eq!(store.file_history("acadia"), vec!["acadia.zip"]);
        assert!(!rename_file(&store, "missing.zip", "x.zip", 1));
    }

    #[test]
    fn plain_rename_keeps_known_version() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("car", "car_mod.zip", "car mod", 100));
        let report = reconcile(&store, &[disk("Car Mod.zip", true, 100)], true, now());
        assert_eq!(report.renamed.len(), 1, "{report:?}");
        assert_eq!(
            store.get_mod("car").unwrap().version.as_deref(),
            Some("1.0")
        );
    }
}

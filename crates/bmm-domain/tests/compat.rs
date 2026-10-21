use std::fs;

use bmm_domain::clock::{Clock, system_clock};
use bmm_domain::library::{
    CheckState, InstalledMod, JsonLibraryStore, LibraryStore, ModBinding, SourceStatus,
};
use bmm_domain::settings::AppSettings;
use bmm_domain::settings_store::SettingsStore;
use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use url::Url;

fn moment() -> DateTime<FixedOffset> {
    Utc.with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
        .unwrap()
        .fixed_offset()
}

fn installed(id: &str, name: &str) -> InstalledMod {
    InstalledMod {
        id: id.to_string(),
        name: name.to_string(),
        file_name: format!("{id}.zip"),
        version: Some("1.0".to_string()),
        size_bytes: Some(100),
        installed_at_utc: moment(),
        enabled: true,
    }
}

fn binding(mod_id: &str) -> ModBinding {
    ModBinding {
        mod_id: mod_id.to_string(),
        source: "modland".to_string(),
        source_id: "x".to_string(),
        url: Url::parse("https://modland.net/x").unwrap(),
        confirmed: true,
    }
}

fn check(mod_id: &str) -> CheckState {
    CheckState {
        mod_id: mod_id.to_string(),
        source: "modland".to_string(),
        last_checked_utc: moment(),
        last_changed_utc: None,
        last_version: Some("1.0".to_string()),
        last_size: Some(100),
        miss_count: 0,
        last_game_version: None,
        last_title: None,
    }
}

#[test]
fn old_library_without_page_searches_loads_without_warnings() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.json");
    fs::write(
        &path,
        r#"{
            "Schema": 1,
            "Mods": [
                {
                    "Id": "bmw",
                    "Name": "BMW M3 E30",
                    "FileName": "bmw.zip",
                    "Version": "1.0",
                    "SizeBytes": 123,
                    "InstalledAtUtc": "2026-10-15T12:00:00+00:00",
                    "Enabled": true
                }
            ],
            "Bindings": [
                {
                    "ModId": "bmw",
                    "Source": "modland",
                    "SourceId": "abc",
                    "Url": "https://modland.net/abc",
                    "Confirmed": true
                }
            ],
            "Checks": [
                {
                    "ModId": "bmw",
                    "Source": "modland",
                    "LastCheckedUtc": "2026-10-15T12:00:00+00:00",
                    "LastVersion": "1.0",
                    "LastSize": 123,
                    "MissCount": 0
                }
            ],
            "Sources": { "modland": "NeedsHumanCheck" },
            "Dismissed": { "bmw": "1.1" },
            "History": { "bmw": ["bmw_old.zip"] },
            "Declined": [],
            "Unused": { "ignored": true }
        }"#,
    )
    .unwrap();

    let store = JsonLibraryStore::new(&path, system_clock());
    assert!(
        store.load_warnings().is_empty(),
        "{:?}",
        store.load_warnings()
    );
    assert_eq!(store.mods().len(), 1);
    assert_eq!(store.mods()[0].name, "BMW M3 E30");
    assert_eq!(store.bindings().len(), 1);
    assert!(store.get_check("bmw", "modland").is_some());
    assert_eq!(
        store.source_status("modland"),
        SourceStatus::NeedsHumanCheck
    );
    assert_eq!(store.get_dismissed_update("bmw").as_deref(), Some("1.1"));
    assert_eq!(store.file_history("bmw"), vec!["bmw_old.zip"]);
    assert!(store.get_page_search("bmw").is_none());
}

#[test]
fn unicode_folder_and_names_survive_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Библиотека модов");
    fs::create_dir(&folder).unwrap();
    let path = folder.join("library.json");

    {
        let store = JsonLibraryStore::new(&path, system_clock());
        store.upsert_mod(InstalledMod {
            id: "машина".to_string(),
            name: "Нива 4x4".to_string(),
            file_name: "Нива 4x4.zip".to_string(),
            version: Some("2.0".to_string()),
            size_bytes: Some(42),
            installed_at_utc: moment(),
            enabled: true,
        });
        store.upsert_binding(binding("машина")).unwrap();
        store.save_check(check("машина"));
        store.add_file_history("машина", "Нива 4x4 — старая.zip");
    }

    let loaded = JsonLibraryStore::new(&path, system_clock());
    assert!(
        loaded.load_warnings().is_empty(),
        "{:?}",
        loaded.load_warnings()
    );
    assert_eq!(loaded.mods()[0].name, "Нива 4x4");
    assert_eq!(loaded.bindings()[0].mod_id, "машина");
    assert!(loaded.get_check("машина", "modland").is_some());
    assert_eq!(loaded.file_history("машина"), vec!["Нива 4x4 — старая.zip"]);
}

#[test]
fn settings_survive_unicode_path_and_old_schema() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Настройки");
    fs::create_dir(&folder).unwrap();
    let path = folder.join("settings.json");
    fs::write(
        &path,
        r#"{ "SchemaVersion": 1, "DisabledSources": ["stmods"] }"#,
    )
    .unwrap();

    let store = SettingsStore::with_system_clock(&path);
    let loaded = store.load();
    assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
    assert_eq!(loaded.settings.disabled_sources, vec!["modland", "stmods"]);

    let changed = AppSettings {
        mods_folder: Some("C:\\Моды игры".to_string()),
        ..loaded.settings
    };
    store.save(&changed).unwrap();
    let again = store.load();
    assert_eq!(again.settings.mods_folder.as_deref(), Some("C:\\Моды игры"));
}

#[test]
fn memory_and_json_stores_agree_on_entities() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.json");
    let memory = bmm_domain::library::InMemoryLibraryStore::new();
    let json = JsonLibraryStore::new(&path, system_clock());

    for store in [&memory as &dyn LibraryStore, &json as &dyn LibraryStore] {
        store.upsert_mod(installed("a", "A"));
        store.upsert_binding(binding("a")).unwrap();
        store.save_check(check("a"));
        store.set_source_status("modland", SourceStatus::Unavailable);
        store.set_dismissed_update("a", Some("1.0"));
        store.add_file_history("a", "old.zip");
        store.set_checking_declined("a", true);
    }

    assert_eq!(memory.mods(), json.mods());
    assert_eq!(memory.bindings(), json.bindings());
    assert_eq!(memory.checks(), json.checks());
    assert_eq!(memory.source_statuses(), json.source_statuses());
    assert_eq!(memory.file_history("a"), json.file_history("a"));
    assert_eq!(
        memory.get_dismissed_update("a"),
        json.get_dismissed_update("a")
    );
    assert_eq!(
        memory.is_checking_declined("a"),
        json.is_checking_declined("a")
    );
}

#[allow(dead_code)]
fn fixed_clock() -> Clock {
    let moment = moment();
    std::sync::Arc::new(move || moment)
}

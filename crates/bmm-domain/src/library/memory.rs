use chrono::{DateTime, FixedOffset};
use std::collections::BTreeMap;
use std::sync::Mutex;

use super::{
    CheckState, Inner, InstalledMod, LibraryError, LibraryStore, ModBinding, PageSearch,
    SourceStatus,
};

pub struct InMemoryLibraryStore {
    inner: Mutex<Inner>,
}

impl Default for InMemoryLibraryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryLibraryStore {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }
}

impl LibraryStore for InMemoryLibraryStore {
    fn mods(&self) -> Vec<InstalledMod> {
        self.inner.lock().unwrap().sorted_mods()
    }

    fn get_mod(&self, id: &str) -> Option<InstalledMod> {
        self.inner.lock().unwrap().mods.get(id).cloned()
    }

    fn upsert_mod(&self, installed: InstalledMod) {
        self.inner.lock().unwrap().upsert_mod(installed);
    }

    fn remove_mod(&self, id: &str) {
        self.inner.lock().unwrap().remove_mod(id);
    }

    fn bindings(&self) -> Vec<ModBinding> {
        self.inner
            .lock()
            .unwrap()
            .bindings
            .values()
            .cloned()
            .collect()
    }

    fn upsert_binding(&self, binding: ModBinding) -> Result<(), LibraryError> {
        self.inner
            .lock()
            .unwrap()
            .upsert_binding(binding)
            .map(|_| ())
    }

    fn remove_binding(&self, mod_id: &str, source: &str) {
        self.inner.lock().unwrap().remove_binding(mod_id, source);
    }

    fn get_check(&self, mod_id: &str, source: &str) -> Option<CheckState> {
        self.inner
            .lock()
            .unwrap()
            .checks
            .get(&(mod_id.to_string(), source.to_string()))
            .cloned()
    }

    fn save_check(&self, state: CheckState) {
        self.inner.lock().unwrap().save_check(state);
    }

    fn checks(&self) -> Vec<CheckState> {
        self.inner
            .lock()
            .unwrap()
            .checks
            .values()
            .cloned()
            .collect()
    }

    fn source_status(&self, source: &str) -> SourceStatus {
        self.inner
            .lock()
            .unwrap()
            .statuses
            .get(source)
            .copied()
            .unwrap_or_default()
    }

    fn set_source_status(&self, source: &str, status: SourceStatus) {
        self.inner.lock().unwrap().set_source_status(source, status);
    }

    fn source_statuses(&self) -> BTreeMap<String, SourceStatus> {
        self.inner.lock().unwrap().statuses.clone()
    }

    fn last_run(&self) -> Option<DateTime<FixedOffset>> {
        self.inner.lock().unwrap().last_run
    }

    fn set_last_run(&self, at: DateTime<FixedOffset>) {
        self.inner.lock().unwrap().last_run = Some(at);
    }

    fn get_dismissed_update(&self, mod_id: &str) -> Option<String> {
        self.inner.lock().unwrap().dismissed.get(mod_id).cloned()
    }

    fn set_dismissed_update(&self, mod_id: &str, key: Option<&str>) {
        self.inner.lock().unwrap().set_dismissed_update(mod_id, key);
    }

    fn file_history(&self, mod_id: &str) -> Vec<String> {
        self.inner
            .lock()
            .unwrap()
            .history
            .get(mod_id)
            .cloned()
            .unwrap_or_default()
    }

    fn add_file_history(&self, mod_id: &str, file_name: &str) {
        self.inner
            .lock()
            .unwrap()
            .add_file_history(mod_id, file_name);
    }

    fn is_checking_declined(&self, mod_id: &str) -> bool {
        self.inner.lock().unwrap().declined.contains(mod_id)
    }

    fn set_checking_declined(&self, mod_id: &str, declined: bool) {
        self.inner
            .lock()
            .unwrap()
            .set_checking_declined(mod_id, declined);
    }

    fn get_page_search(&self, mod_id: &str) -> Option<PageSearch> {
        self.inner
            .lock()
            .unwrap()
            .page_searches
            .get(mod_id)
            .cloned()
    }

    fn set_page_search(&self, mod_id: &str, state: Option<PageSearch>) {
        self.inner.lock().unwrap().set_page_search(mod_id, state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::InstalledMod;
    use chrono::{TimeZone, Utc};
    use url::Url;

    fn installed(id: &str, name: &str) -> InstalledMod {
        InstalledMod {
            id: id.to_string(),
            name: name.to_string(),
            file_name: format!("{id}.zip"),
            version: Some("1.0".to_string()),
            size_bytes: Some(10),
            installed_at_utc: Utc
                .with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
                .unwrap()
                .fixed_offset(),
            enabled: true,
        }
    }

    fn binding(mod_id: &str, source: &str) -> ModBinding {
        ModBinding {
            mod_id: mod_id.to_string(),
            source: source.to_string(),
            source_id: "x".to_string(),
            url: Url::parse("https://example.org/x").unwrap(),
            confirmed: true,
        }
    }

    #[test]
    fn binding_to_missing_mod_is_rejected() {
        let store = InMemoryLibraryStore::new();
        assert!(matches!(
            store.upsert_binding(binding("missing", "modland")),
            Err(LibraryError::NoMod(_))
        ));
    }

    #[test]
    fn remove_mod_cleans_everything() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "A"));
        store.upsert_binding(binding("a", "modland")).unwrap();
        store.set_dismissed_update("a", Some("1.0"));
        store.add_file_history("a", "old.zip");
        store.set_checking_declined("a", true);
        store.remove_mod("a");
        assert!(store.mods().is_empty());
        assert!(store.bindings().is_empty());
        assert!(store.get_dismissed_update("a").is_none());
        assert!(store.file_history("a").is_empty());
        assert!(!store.is_checking_declined("a"));
    }

    #[test]
    fn history_keeps_twenty_and_no_current_name() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "A"));
        for index in 0..25 {
            store.add_file_history("a", &format!("mod_v{index}.zip"));
        }
        store.add_file_history("a", "a.zip");
        let history = store.file_history("a");
        assert_eq!(history.len(), 20);
        assert_eq!(history.last().map(String::as_str), Some("mod_v24.zip"));
        assert!(!history.iter().any(|name| name == "a.zip"));
    }

    #[test]
    fn history_ignores_case_and_unicode_form() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "A"));
        store.add_file_history("a", "A.ZIP");
        assert!(
            store.file_history("a").is_empty(),
            "имя в другом регистре — не прошлый файл"
        );

        let decomposed = "cafe\u{301}.zip";
        let composed = "café.zip";
        store.add_file_history("a", decomposed);
        store.add_file_history("a", composed);
        assert_eq!(store.file_history("a"), vec![composed.to_string()]);
    }

    #[test]
    fn save_check_requires_binding() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "A"));
        let state = CheckState {
            mod_id: "a".to_string(),
            source: "modland".to_string(),
            last_checked_utc: Utc
                .with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
                .unwrap()
                .fixed_offset(),
            last_changed_utc: None,
            last_version: Some("1.0".to_string()),
            last_size: Some(10),
            miss_count: 0,
            last_game_version: None,
            last_title: None,
        };
        store.save_check(state.clone());
        assert!(store.get_check("a", "modland").is_none());
        store.upsert_binding(binding("a", "modland")).unwrap();
        store.save_check(state);
        assert!(store.get_check("a", "modland").is_some());
    }
}

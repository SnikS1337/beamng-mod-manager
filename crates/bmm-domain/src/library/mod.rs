pub mod disk;
pub mod json;
pub mod memory;
pub mod sync;

pub use disk::DiskMod;
pub use json::JsonLibraryStore;
pub use memory::InMemoryLibraryStore;
pub use sync::{SyncReport, reconcile, rename_file};

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use url::Url;

pub const MAX_FILE_HISTORY: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct InstalledMod {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub version: Option<String>,
    pub size_bytes: Option<i64>,
    pub installed_at_utc: DateTime<FixedOffset>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ModBinding {
    pub mod_id: String,
    pub source: String,
    pub source_id: String,
    pub url: Url,
    pub confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct CheckState {
    pub mod_id: String,
    pub source: String,
    pub last_checked_utc: DateTime<FixedOffset>,
    pub last_changed_utc: Option<DateTime<FixedOffset>>,
    pub last_version: Option<String>,
    pub last_size: Option<i64>,
    #[serde(default)]
    pub miss_count: i32,
    pub last_game_version: Option<String>,
    pub last_title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PageSearch {
    pub at: DateTime<FixedOffset>,
    #[serde(default)]
    pub rejected: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum SourceStatus {
    #[default]
    Ok,
    NeedsHumanCheck,
    Unavailable,
}

#[derive(Debug, thiserror::Error)]
pub enum LibraryError {
    #[error("Нет мода {0}")]
    NoMod(String),
}

pub trait LibraryStore: Send + Sync {
    fn mods(&self) -> Vec<InstalledMod>;
    fn get_mod(&self, id: &str) -> Option<InstalledMod>;
    fn upsert_mod(&self, installed: InstalledMod);
    fn remove_mod(&self, id: &str);

    fn bindings(&self) -> Vec<ModBinding>;
    fn upsert_binding(&self, binding: ModBinding) -> Result<(), LibraryError>;
    fn remove_binding(&self, mod_id: &str, source: &str);

    fn get_check(&self, mod_id: &str, source: &str) -> Option<CheckState>;
    fn save_check(&self, state: CheckState);
    fn checks(&self) -> Vec<CheckState>;

    fn source_status(&self, source: &str) -> SourceStatus;
    fn set_source_status(&self, source: &str, status: SourceStatus);
    fn source_statuses(&self) -> BTreeMap<String, SourceStatus>;

    fn get_dismissed_update(&self, mod_id: &str) -> Option<String>;
    fn set_dismissed_update(&self, mod_id: &str, key: Option<&str>);
    fn file_history(&self, mod_id: &str) -> Vec<String>;
    fn add_file_history(&self, mod_id: &str, file_name: &str);
    fn is_checking_declined(&self, mod_id: &str) -> bool;
    fn set_checking_declined(&self, mod_id: &str, declined: bool);
    fn get_page_search(&self, mod_id: &str) -> Option<PageSearch>;
    fn set_page_search(&self, mod_id: &str, state: Option<PageSearch>);
}

#[derive(Debug, Default)]
pub(crate) struct Inner {
    pub(crate) mods: HashMap<String, InstalledMod>,
    pub(crate) bindings: HashMap<(String, String), ModBinding>,
    pub(crate) checks: HashMap<(String, String), CheckState>,
    pub(crate) statuses: BTreeMap<String, SourceStatus>,
    pub(crate) dismissed: HashMap<String, String>,
    pub(crate) history: HashMap<String, Vec<String>>,
    pub(crate) declined: HashSet<String>,
    pub(crate) page_searches: HashMap<String, PageSearch>,
}

impl Inner {
    pub(crate) fn sorted_mods(&self) -> Vec<InstalledMod> {
        let mut mods: Vec<_> = self.mods.values().cloned().collect();
        mods.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        mods
    }

    pub(crate) fn upsert_mod(&mut self, installed: InstalledMod) -> bool {
        if self.mods.get(&installed.id) == Some(&installed) {
            return false;
        }
        self.mods.insert(installed.id.clone(), installed);
        true
    }

    pub(crate) fn remove_mod(&mut self, id: &str) {
        self.mods.remove(id);
        self.bindings.retain(|(mod_id, _), _| mod_id != id);
        self.checks.retain(|(mod_id, _), _| mod_id != id);
        self.dismissed.remove(id);
        self.history.remove(id);
        self.declined.remove(id);
        self.page_searches.remove(id);
    }

    pub(crate) fn upsert_binding(&mut self, binding: ModBinding) -> Result<bool, LibraryError> {
        if !self.mods.contains_key(&binding.mod_id) {
            return Err(LibraryError::NoMod(binding.mod_id));
        }
        let key = (binding.mod_id.clone(), binding.source.clone());
        if let Some(existing) = self.bindings.get(&key) {
            if existing == &binding
                && !(binding.confirmed && self.declined.contains(&binding.mod_id))
            {
                return Ok(false);
            }
        }
        let confirmed = binding.confirmed;
        let mod_id = binding.mod_id.clone();
        self.bindings.insert(key, binding);
        if confirmed {
            self.declined.remove(&mod_id);
        }
        Ok(true)
    }

    pub(crate) fn remove_binding(&mut self, mod_id: &str, source: &str) {
        self.bindings
            .remove(&(mod_id.to_string(), source.to_string()));
        self.checks
            .remove(&(mod_id.to_string(), source.to_string()));
    }

    pub(crate) fn save_check(&mut self, state: CheckState) -> bool {
        let key = (state.mod_id.clone(), state.source.clone());
        if !self.bindings.contains_key(&key) {
            return false;
        }
        if self.checks.get(&key) == Some(&state) {
            return false;
        }
        self.checks.insert(key, state);
        true
    }

    pub(crate) fn set_source_status(&mut self, source: &str, status: SourceStatus) -> bool {
        if self.statuses.get(source).copied() == Some(status) {
            return false;
        }
        self.statuses.insert(source.to_string(), status);
        true
    }

    pub(crate) fn set_dismissed_update(&mut self, mod_id: &str, key: Option<&str>) -> bool {
        if !self.mods.contains_key(mod_id) {
            return false;
        }
        let key = key.filter(|key| !key.is_empty());
        let current = self.dismissed.get(mod_id).map(String::as_str);
        if current == key {
            return false;
        }
        match key {
            Some(key) => {
                self.dismissed.insert(mod_id.to_string(), key.to_string());
            }
            None => {
                self.dismissed.remove(mod_id);
            }
        }
        true
    }

    pub(crate) fn add_file_history(&mut self, mod_id: &str, file_name: &str) {
        if file_name.trim().is_empty() {
            return;
        }
        let Some(current) = self.mods.get(mod_id) else {
            return;
        };
        if current.file_name.eq_ignore_ascii_case(file_name) {
            return;
        }
        let history = self.history.entry(mod_id.to_string()).or_default();
        history.retain(|existing| !existing.eq_ignore_ascii_case(file_name));
        history.push(file_name.to_string());
        if history.len() > MAX_FILE_HISTORY {
            let excess = history.len() - MAX_FILE_HISTORY;
            history.drain(0..excess);
        }
    }

    pub(crate) fn set_checking_declined(&mut self, mod_id: &str, declined: bool) -> bool {
        if !self.mods.contains_key(mod_id) {
            return false;
        }
        if declined {
            self.declined.insert(mod_id.to_string())
        } else {
            self.declined.remove(mod_id)
        }
    }

    pub(crate) fn set_page_search(&mut self, mod_id: &str, state: Option<PageSearch>) -> bool {
        if !self.mods.contains_key(mod_id) {
            return false;
        }
        let current = self.page_searches.get(mod_id);
        if current == state.as_ref() {
            return false;
        }
        match state {
            Some(state) => {
                self.page_searches.insert(mod_id.to_string(), state);
            }
            None => {
                self.page_searches.remove(mod_id);
            }
        }
        true
    }
}

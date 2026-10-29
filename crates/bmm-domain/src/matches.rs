use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, FixedOffset};
use serde::Serialize;

use crate::atomic;
use crate::clock::Clock;
use crate::models::Listing;

const MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualMatch {
    Same,
    Different,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchDecisionEntry {
    pub key_a: String,
    pub key_b: String,
    pub decision: ManualMatch,
    pub decided_at_utc: DateTime<FixedOffset>,
}

pub trait MatchDecisions {
    fn get(&self, a: &Listing, b: &Listing) -> Option<ManualMatch>;
}

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error("Пара состоит из одной и той же записи.")]
    SameRecord,
    #[error("Записи с одного сайта не объединяются.")]
    SameSource,
    #[error("{0}")]
    Io(String),
}

pub struct MatchDecisionStore {
    path: Option<PathBuf>,
    now: Clock,
    items: Mutex<BTreeMap<(String, String), MatchDecisionEntry>>,
    blocked: bool,
    load_warnings: Vec<String>,
}

impl MatchDecisionStore {
    pub fn new(path: Option<PathBuf>, now: Clock) -> Self {
        let mut warnings = Vec::new();
        let (items, blocked) = match &path {
            Some(path) => load(path, &now, &mut warnings),
            None => (BTreeMap::new(), false),
        };
        Self {
            path,
            now,
            items: Mutex::new(items),
            blocked,
            load_warnings: warnings,
        }
    }

    pub fn load_warnings(&self) -> &[String] {
        &self.load_warnings
    }

    pub fn key_of(listing: &Listing) -> String {
        format!(
            "{}:{}",
            listing.source.trim().to_lowercase(),
            listing.source_id.trim()
        )
    }

    fn norm_key(key: &str) -> Option<String> {
        let (source, id) = key.split_once(':')?;
        let source = source.trim().to_lowercase();
        let id = id.trim();
        if source.is_empty() || id.is_empty() {
            None
        } else {
            Some(format!("{source}:{id}"))
        }
    }

    fn pair(a: String, b: String) -> (String, String) {
        if a <= b { (a, b) } else { (b, a) }
    }

    pub fn all(&self) -> Vec<MatchDecisionEntry> {
        let items = self.items.lock().unwrap();
        items.values().cloned().collect()
    }

    pub fn set(&self, a: &Listing, b: &Listing, decision: ManualMatch) -> Result<(), MatchError> {
        let key_a = Self::key_of(a);
        let key_b = Self::key_of(b);
        if key_a == key_b {
            return Err(MatchError::SameRecord);
        }
        if a.source.trim().eq_ignore_ascii_case(b.source.trim()) {
            return Err(MatchError::SameSource);
        }

        let key = Self::pair(key_a, key_b);
        let entry = MatchDecisionEntry {
            key_a: key.0.clone(),
            key_b: key.1.clone(),
            decision,
            decided_at_utc: (self.now)(),
        };

        let mut items = self.items.lock().unwrap();
        let previous = items.insert(key.clone(), entry);
        if let Err(error) = self.save_locked(&items) {
            match previous {
                Some(previous) => {
                    items.insert(key, previous);
                }
                None => {
                    items.remove(&key);
                }
            }
            return Err(error);
        }
        Ok(())
    }

    pub fn clear(&self, a: &Listing, b: &Listing) -> Result<(), MatchError> {
        let key_a = Self::key_of(a);
        let key_b = Self::key_of(b);
        if key_a == key_b {
            return Ok(());
        }
        let key = Self::pair(key_a, key_b);

        let mut items = self.items.lock().unwrap();
        let Some(previous) = items.remove(&key) else {
            return Ok(());
        };
        if let Err(error) = self.save_locked(&items) {
            items.insert(key, previous);
            return Err(error);
        }
        Ok(())
    }

    fn save_locked(
        &self,
        items: &BTreeMap<(String, String), MatchDecisionEntry>,
    ) -> Result<(), MatchError> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if self.blocked {
            return Err(MatchError::Io(
                "Файл решений по дублям не прочитан; изменения не сохраняются до перезапуска программы."
                    .to_string(),
            ));
        }

        #[derive(Serialize)]
        struct FileItem {
            a: String,
            b: String,
            decision: &'static str,
            at: DateTime<FixedOffset>,
        }

        #[derive(Serialize)]
        struct FileModel {
            schema: i32,
            items: Vec<FileItem>,
        }

        let model = FileModel {
            schema: 1,
            items: items
                .values()
                .map(|entry| FileItem {
                    a: entry.key_a.clone(),
                    b: entry.key_b.clone(),
                    decision: match entry.decision {
                        ManualMatch::Same => "same",
                        ManualMatch::Different => "different",
                    },
                    at: entry.decided_at_utc,
                })
                .collect(),
        };

        let json = serde_json::to_string_pretty(&model)
            .map_err(|error| MatchError::Io(error.to_string()))?;
        atomic::write_atomic(path, json.as_bytes())
            .map_err(|error| MatchError::Io(error.to_string()))
    }
}

impl MatchDecisions for MatchDecisionStore {
    fn get(&self, a: &Listing, b: &Listing) -> Option<ManualMatch> {
        let key_a = Self::key_of(a);
        let key_b = Self::key_of(b);
        if key_a == key_b {
            return None;
        }
        let key = Self::pair(key_a, key_b);
        let items = self.items.lock().unwrap();
        items.get(&key).map(|entry| entry.decision)
    }
}

fn load(
    path: &Path,
    now: &Clock,
    warnings: &mut Vec<String>,
) -> (BTreeMap<(String, String), MatchDecisionEntry>, bool) {
    let mut items = BTreeMap::new();
    if !path.exists() {
        return (items, false);
    }

    if let Ok(metadata) = fs::metadata(path)
        && metadata.len() > MAX_BYTES
    {
        let blocked = quarantine(path, now, "файл решений слишком большой", warnings);
        return (items, blocked);
    }

    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            warnings.push(format!(
                "не удалось прочитать решения по дублям: {error}; изменения не сохранятся до перезапуска программы"
            ));
            return (items, true);
        }
    };

    let value: serde_json::Value = match json5::from_str(&text) {
        Ok(value) => value,
        Err(_) => {
            let blocked = quarantine(path, now, "файл решений повреждён", warnings);
            return (items, blocked);
        }
    };

    let Some(root) = value.as_object() else {
        let blocked = quarantine(path, now, "файл решений имеет неверный формат", warnings);
        return (items, blocked);
    };
    let Some(array) = root.get("items").and_then(serde_json::Value::as_array) else {
        let blocked = quarantine(path, now, "файл решений имеет неверный формат", warnings);
        return (items, blocked);
    };

    let mut skipped = 0;
    for item in array {
        let Some(object) = item.as_object() else {
            skipped += 1;
            continue;
        };
        let (Some(a), Some(b), Some(decision)) = (
            object.get("a").and_then(serde_json::Value::as_str),
            object.get("b").and_then(serde_json::Value::as_str),
            object.get("decision").and_then(serde_json::Value::as_str),
        ) else {
            skipped += 1;
            continue;
        };
        let (Some(key_a), Some(key_b)) = (
            MatchDecisionStore::norm_key(a),
            MatchDecisionStore::norm_key(b),
        ) else {
            skipped += 1;
            continue;
        };
        if key_a == key_b {
            skipped += 1;
            continue;
        }
        let decision = match decision {
            "same" => ManualMatch::Same,
            "different" => ManualMatch::Different,
            _ => {
                skipped += 1;
                continue;
            }
        };
        let at = object
            .get("at")
            .and_then(serde_json::Value::as_str)
            .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
            .unwrap_or_else(|| (now)());

        let key = MatchDecisionStore::pair(key_a.clone(), key_b.clone());
        items.insert(
            key,
            MatchDecisionEntry {
                key_a,
                key_b,
                decision,
                decided_at_utc: at,
            },
        );
    }

    if skipped > 0 {
        warnings.push(format!(
            "решений по дублям пропущено из-за неверной записи: {skipped}"
        ));
    }

    (items, false)
}

fn quarantine(path: &Path, now: &Clock, why: &str, warnings: &mut Vec<String>) -> bool {
    let quarantined = atomic::quarantine_file(path, (now)());
    let blocked = quarantined.is_none();
    let warning = match quarantined {
        Some(target) => {
            let name = target
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            format!("{why}: сохранён как {name}, решения начаты заново")
        }
        None => {
            format!("{why}, файл убрать не удалось; решения начаты заново, сохранение остановлено")
        }
    };
    warnings.push(warning);
    blocked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::system_clock;
    use chrono::{TimeZone, Utc};
    use url::Url;

    fn listing(source: &str, id: &str) -> Listing {
        Listing::new(
            source,
            Url::parse("https://example.org/x").unwrap(),
            id,
            "Toyota Sienna",
        )
    }

    fn fixed_clock() -> Clock {
        let moment = Utc
            .with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        std::sync::Arc::new(move || moment)
    }

    #[test]
    fn decision_is_symmetric() {
        let store = MatchDecisionStore::new(None, system_clock());
        let a = listing("modland", "a");
        let b = listing("stmods", "c");
        store.set(&a, &b, ManualMatch::Same).unwrap();
        assert_eq!(store.get(&a, &b), Some(ManualMatch::Same));
        assert_eq!(store.get(&b, &a), Some(ManualMatch::Same));
    }

    #[test]
    fn same_source_and_same_record_are_rejected() {
        let store = MatchDecisionStore::new(None, system_clock());
        let a = listing("modland", "a");
        let b = listing("ModLand", "b");
        assert!(matches!(
            store.set(&a, &b, ManualMatch::Same),
            Err(MatchError::SameSource)
        ));
        assert!(matches!(
            store.set(&a, &a, ManualMatch::Same),
            Err(MatchError::SameRecord)
        ));
    }

    #[cfg(windows)]
    #[test]
    fn unremovable_bad_file_blocks_new_decisions() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matches.json");
        fs::write(&path, "{ это не json").unwrap();
        let _lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let store = MatchDecisionStore::new(Some(path.clone()), system_clock());
        assert_eq!(store.load_warnings().len(), 1);
        let result = store.set(&listing("a", "1"), &listing("b", "2"), ManualMatch::Same);
        assert!(result.is_err(), "{result:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "{ это не json");
    }

    #[test]
    fn clear_removes_decision() {
        let store = MatchDecisionStore::new(None, system_clock());
        let a = listing("modland", "a");
        let b = listing("stmods", "c");
        store.set(&a, &b, ManualMatch::Different).unwrap();
        store.clear(&a, &b).unwrap();
        assert_eq!(store.get(&a, &b), None);
    }

    #[test]
    fn keys_lowercase_source_and_keep_id_case() {
        assert_eq!(
            MatchDecisionStore::key_of(&listing("ModLand", "AbC")),
            "modland:AbC"
        );
    }

    #[test]
    fn file_round_trip_and_no_temp() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matches.json");
        let store = MatchDecisionStore::new(Some(path.clone()), fixed_clock());
        let a = listing("modland", "a");
        let b = listing("stmods", "c");
        store.set(&a, &b, ManualMatch::Same).unwrap();
        assert!(!dir.path().join("matches.json.tmp").exists());

        let loaded = MatchDecisionStore::new(Some(path), fixed_clock());
        assert_eq!(loaded.get(&a, &b), Some(ManualMatch::Same));
        assert_eq!(loaded.all()[0].decided_at_utc.timestamp(), 1_792_584_000);
    }

    #[test]
    fn bad_entries_are_skipped_with_one_warning() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matches.json");
        fs::write(
            &path,
            r#"{
                "schema": 1,
                "items": [
                    { "a": "modland:a", "b": "stmods:c", "decision": "different", "at": "2026-10-01T00:00:00Z" },
                    { "a": 5, "b": "x", "decision": "same" },
                    { "a": "x:1", "b": "x:1", "decision": "same" },
                    { "a": "x:1", "b": "y:2", "decision": "maybe" },
                    "мусор"
                ]
            }"#,
        )
        .unwrap();
        let store = MatchDecisionStore::new(Some(path), fixed_clock());
        assert_eq!(
            store.load_warnings().len(),
            1,
            "{:?}",
            store.load_warnings()
        );
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].decision, ManualMatch::Different);
    }

    #[test]
    fn missing_at_uses_now() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matches.json");
        fs::write(
            &path,
            r#"{ "items": [ { "a": "ModLand:a", "b": " STMods :c", "decision": "same" } ] }"#,
        )
        .unwrap();
        let store = MatchDecisionStore::new(Some(path), fixed_clock());
        assert!(
            store.load_warnings().is_empty(),
            "{:?}",
            store.load_warnings()
        );
        assert_eq!(store.all().len(), 1);
        assert_eq!(store.all()[0].decided_at_utc.timestamp(), 1_792_584_000);
    }

    #[test]
    fn corrupted_file_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matches.json");
        fs::write(&path, "{ это не json").unwrap();
        let store = MatchDecisionStore::new(Some(path.clone()), fixed_clock());
        assert_eq!(store.load_warnings().len(), 1);
        assert!(!path.exists());
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .any(|entry| entry.file_name().to_string_lossy().contains(".bad-"))
        );
    }

    #[test]
    fn second_quarantine_keeps_first_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matches.json");
        fs::write(&path, "{ первый").unwrap();
        let first = MatchDecisionStore::new(Some(path.clone()), fixed_clock());
        assert_eq!(first.load_warnings().len(), 1);
        fs::write(&path, "{ второй").unwrap();
        let second = MatchDecisionStore::new(Some(path.clone()), fixed_clock());
        assert_eq!(second.load_warnings().len(), 1);

        let backups: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bad-"))
            .map(|entry| fs::read_to_string(entry.path()).unwrap())
            .collect();
        assert_eq!(backups.len(), 2, "{backups:?}");
        assert!(backups.contains(&"{ первый".to_string()), "{backups:?}");
        assert!(backups.contains(&"{ второй".to_string()), "{backups:?}");
    }

    #[test]
    fn failed_save_rolls_back_memory() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("matches.json");
        fs::create_dir(&path).unwrap();
        let store = MatchDecisionStore::new(Some(path), fixed_clock());
        let a = listing("modland", "a");
        let b = listing("stmods", "c");
        assert!(store.set(&a, &b, ManualMatch::Same).is_err());
        assert_eq!(store.get(&a, &b), None);
    }
}

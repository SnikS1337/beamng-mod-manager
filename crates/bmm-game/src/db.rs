use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, FixedOffset};
use serde_json::{Map, Value};

use bmm_domain::atomic;

const MAX_BYTES: u64 = 128 * 1024 * 1024;
const BACKUP_SUFFIX: &str = ".bmm-bak";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    pub unique_id: String,
    pub version: Option<String>,
    pub resource_id: Option<i64>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub last_update: Option<DateTime<FixedOffset>>,
}

impl RepoInfo {
    /// Сведения о моде из `modData` в db.json игры или из `mod_info/<ID>/info.json`
    /// в самом архиве — там одно и то же. Мод из репозитория BeamNG несёт
    /// `version_string`, `resource_id`, `title`, `username`; мод, оформленный автором
    /// сам, — `version`, `name`, `author`.
    pub fn from_mod_data(unique_id: &str, data: &Value) -> Self {
        let text = |keys: &[&str]| {
            keys.iter().find_map(|key| {
                data.get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
        };
        RepoInfo {
            unique_id: unique_id.to_string(),
            version: text(&["version_string", "version"]),
            resource_id: data.get("resource_id").and_then(number_or_string),
            title: text(&["title", "name"]),
            author: text(&["username", "author"]),
            last_update: data.get("last_update").and_then(|value| {
                value
                    .as_str()
                    .and_then(|text| DateTime::parse_from_rfc3339(text).ok())
                    .map(|date| date.fixed_offset())
                    .or_else(|| {
                        value.as_i64().and_then(|seconds| {
                            DateTime::from_timestamp(seconds, 0).map(|date| date.fixed_offset())
                        })
                    })
            }),
        }
    }

    pub fn page_url(&self) -> Option<String> {
        self.resource_id
            .map(|id| format!("https://beamng.com/resources/{id}/"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameModEntry {
    pub key: String,
    pub file_name: String,
    pub full_path: Option<String>,
    pub active: bool,
    pub mod_type: String,
    pub date_added: Option<DateTime<FixedOffset>>,
    pub file_size: Option<i64>,
    pub repo: Option<RepoInfo>,
}

impl GameModEntry {
    pub fn in_mods_root(&self) -> bool {
        self.full_path.as_deref().is_none_or(|path| {
            normalize_full_path(path) == format!("mods/{}", normalize_full_path(&self.file_name))
        })
    }

    pub fn is_at(&self, sub_folder: Option<&str>) -> bool {
        let Some(full) = &self.full_path else {
            return true;
        };
        let expected = match sub_folder {
            Some(folder) => format!("mods/{folder}/{}", self.file_name),
            None => format!("mods/{}", self.file_name),
        };
        normalize_full_path(full) == normalize_full_path(&expected)
    }
}

fn normalize_full_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches('/')
        .to_ascii_lowercase()
}

#[derive(Debug)]
pub struct GameDbReadResult {
    pub exists: bool,
    pub format_version: Option<f64>,
    pub mods: Vec<GameModEntry>,
    pub warnings: Vec<String>,
    pub unreadable: bool,
    by_key: HashMap<String, usize>,
    by_file: HashMap<String, usize>,
}

impl GameDbReadResult {
    pub fn missing() -> Self {
        Self {
            exists: false,
            format_version: None,
            mods: Vec::new(),
            warnings: Vec::new(),
            unreadable: false,
            by_key: HashMap::new(),
            by_file: HashMap::new(),
        }
    }

    pub fn find(&self, file_name: &str) -> Option<&GameModEntry> {
        self.by_key
            .get(&key_of(file_name))
            .or_else(|| self.by_file.get(&file_name.to_lowercase()))
            .map(|index| &self.mods[*index])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetActiveOutcome {
    Done,
    Unchanged,
    GameRunning,
    NoDb,
    NotInDb,
    Unreadable,
    ChangedMeanwhile,
    IoError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetActiveResult {
    pub outcome: SetActiveOutcome,
    pub not_in_db: Vec<String>,
    pub message: Option<String>,
}

impl SetActiveResult {
    pub fn success(&self) -> bool {
        matches!(
            self.outcome,
            SetActiveOutcome::Done | SetActiveOutcome::Unchanged
        )
    }
}

pub struct GameModDb {
    path: PathBuf,
}

impl GameModDb {
    pub fn new(mods_dir: impl AsRef<Path>) -> Self {
        Self {
            path: mods_dir.as_ref().join("db.json"),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn backup_path(&self) -> PathBuf {
        PathBuf::from(format!("{}{BACKUP_SUFFIX}", self.path.display()))
    }

    pub fn read_stable(&self) -> GameDbReadResult {
        let mut result = self.read();
        for attempt in 0..3 {
            if !result.unreadable {
                break;
            }
            thread::sleep(Duration::from_millis(60 * (attempt + 1)));
            result = self.read();
        }
        result
    }

    pub fn read(&self) -> GameDbReadResult {
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return GameDbReadResult::missing();
            }
            Err(_) => return unreadable("файл db.json не открылся"),
        };
        if metadata.len() > MAX_BYTES {
            return unreadable("файл db.json слишком большой");
        }
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(_) => return unreadable("файл db.json не читается"),
        };
        let value: Value = match json5::from_str(&text) {
            Ok(value) => value,
            Err(_) => return unreadable("файл db.json повреждён"),
        };
        if !value.is_object() {
            return unreadable("файл db.json имеет неверный формат");
        }
        parse_value(value)
    }

    pub fn set_active(
        &self,
        changes: &[(&str, bool)],
        game_running: impl Fn() -> bool,
    ) -> SetActiveResult {
        if game_running() {
            return result(SetActiveOutcome::GameRunning, Vec::new(), None);
        }

        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return result(SetActiveOutcome::NoDb, Vec::new(), None);
            }
            Err(_) => return result(SetActiveOutcome::IoError, Vec::new(), None),
        };
        let mtime = metadata.modified().ok();
        let length = metadata.len();
        if length > MAX_BYTES {
            return result(
                SetActiveOutcome::Unreadable,
                Vec::new(),
                Some("файл db.json слишком большой".to_string()),
            );
        }
        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(_) => return result(SetActiveOutcome::IoError, Vec::new(), None),
        };
        let mut root: Value = match json5::from_str(&text) {
            Ok(value) => value,
            Err(_) => return result(SetActiveOutcome::Unreadable, Vec::new(), None),
        };
        let Some(mods) = root.get_mut("mods").and_then(Value::as_object_mut) else {
            return result(SetActiveOutcome::Unreadable, Vec::new(), None);
        };

        let mut not_in_db = Vec::new();
        let mut changed = false;
        for (file_name, desired) in changes {
            let wanted_key = key_of(file_name);
            let found_key = find_object_key(mods, file_name, &wanted_key);
            let Some(key) = found_key else {
                not_in_db.push((*file_name).to_string());
                continue;
            };
            let Some(entry) = mods.get_mut(&key) else {
                continue;
            };
            let current = entry.get("active").and_then(Value::as_bool).unwrap_or(true);
            if current != *desired {
                entry["active"] = Value::Bool(*desired);
                changed = true;
            }
        }

        if !changed {
            let outcome = if !changes.is_empty() && not_in_db.len() == changes.len() {
                SetActiveOutcome::NotInDb
            } else {
                SetActiveOutcome::Unchanged
            };
            return result(outcome, not_in_db, None);
        }

        if game_running() {
            return result(SetActiveOutcome::GameRunning, not_in_db, None);
        }

        let current_meta = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(_) => return result(SetActiveOutcome::IoError, not_in_db, None),
        };
        if current_meta.len() != length
            || (mtime.is_some() && current_meta.modified().ok() != mtime)
        {
            return result(SetActiveOutcome::ChangedMeanwhile, not_in_db, None);
        }

        if fs::copy(&self.path, self.backup_path()).is_err() {
            return result(SetActiveOutcome::IoError, not_in_db, None);
        }

        let json = match serde_json::to_string_pretty(&root) {
            Ok(json) => json,
            Err(error) => {
                return result(
                    SetActiveOutcome::IoError,
                    not_in_db,
                    Some(error.to_string()),
                );
            }
        };
        // Уникальный временный файл: два экземпляра приложения не должны
        // писать в один `db.json.bmm-tmp` и подменять недописанный файл.
        let temp = atomic::unique_temp_path(&self.path);
        if let Err(error) = atomic::write_atomic_with_temp(&self.path, &temp, json.as_bytes()) {
            return result(
                SetActiveOutcome::IoError,
                not_in_db,
                Some(error.to_string()),
            );
        }

        result(SetActiveOutcome::Done, not_in_db, None)
    }
}

pub fn key_of(file_name: &str) -> String {
    let name = Path::new(file_name.trim())
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.trim().to_string());
    let name = if name.to_ascii_lowercase().ends_with(".zip") {
        &name[..name.len() - 4]
    } else {
        &name
    };
    name.chars()
        .map(|ch| {
            if ch.is_ascii_uppercase() {
                ch.to_ascii_lowercase()
            } else {
                ch
            }
        })
        .collect()
}

fn find_object_key(mods: &Map<String, Value>, file_name: &str, wanted_key: &str) -> Option<String> {
    let wanted = wanted_key.to_lowercase();
    if let Some((key, _)) = mods.iter().find(|(key, _)| key.to_lowercase() == wanted) {
        return Some(key.clone());
    }
    let wanted_name = file_name.to_lowercase();
    mods.iter()
        .find(|(_, value)| {
            value
                .get("filename")
                .and_then(Value::as_str)
                .is_some_and(|name| name.to_lowercase() == wanted_name)
        })
        .map(|(key, _)| key.clone())
}

fn parse_value(root: Value) -> GameDbReadResult {
    let mut warnings = Vec::new();
    let format_version = root
        .get("header")
        .and_then(|header| header.get("version"))
        .and_then(Value::as_f64);
    if let Some(version) = format_version
        && version >= 2.0
    {
        warnings.push("db.json новее, чем понимает программа".to_string());
    }

    let mut mods = Vec::new();
    let mut unreadable = false;
    match root.get("mods") {
        None | Some(Value::Null) => {}
        Some(Value::Object(entries)) => {
            for (key, value) in entries {
                match parse_entry(key, value) {
                    Ok(entry) => mods.push(entry),
                    Err(reason) => {
                        warnings.push(reason);
                    }
                }
            }
        }
        Some(_) => {
            warnings.push("поле mods не является объектом".to_string());
            unreadable = true;
        }
    }

    let mut by_key = HashMap::new();
    let mut by_file = HashMap::new();
    for (index, entry) in mods.iter().enumerate() {
        by_key.entry(entry.key.to_lowercase()).or_insert(index);
        by_file
            .entry(entry.file_name.to_lowercase())
            .or_insert(index);
    }

    GameDbReadResult {
        exists: true,
        format_version,
        mods,
        warnings,
        unreadable,
        by_key,
        by_file,
    }
}

fn parse_entry(key: &str, value: &Value) -> Result<GameModEntry, String> {
    let Some(object) = value.as_object() else {
        return Err(format!("запись {key}: не объект"));
    };
    let Some(file_name) = object.get("filename").and_then(Value::as_str) else {
        return Err(format!("запись {key}: нет filename"));
    };
    if file_name.trim().is_empty() {
        return Err(format!("запись {key}: пустой filename"));
    }

    let active = object
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let full_path = object
        .get("fullpath")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mod_type = object
        .get("modType")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let file_size = object
        .get("stat")
        .and_then(|stat| stat.get("filesize"))
        .and_then(number_or_string);
    let date_added = object
        .get("dateAdded")
        .and_then(Value::as_i64)
        .filter(|seconds| *seconds > 0 && *seconds < 32_503_680_000)
        .and_then(|seconds| DateTime::from_timestamp(seconds, 0))
        .map(|date| date.fixed_offset());

    let unique_id = object.get("modID").and_then(Value::as_str).or_else(|| {
        object
            .get("modData")
            .and_then(|data| data.get("tagid"))
            .and_then(Value::as_str)
    });
    let repo = unique_id.map(|unique_id| {
        RepoInfo::from_mod_data(unique_id, object.get("modData").unwrap_or(&Value::Null))
    });

    Ok(GameModEntry {
        key: key.to_lowercase(),
        file_name: file_name.to_string(),
        full_path,
        active,
        mod_type,
        date_added,
        file_size,
        repo,
    })
}

fn number_or_string(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|value| value as i64))
        .or_else(|| value.as_str().and_then(|text| text.parse::<i64>().ok()))
}

fn unreadable(message: &str) -> GameDbReadResult {
    GameDbReadResult {
        exists: true,
        format_version: None,
        mods: Vec::new(),
        warnings: vec![message.to_string()],
        unreadable: true,
        by_key: HashMap::new(),
        by_file: HashMap::new(),
    }
}

fn result(
    outcome: SetActiveOutcome,
    not_in_db: Vec<String>,
    message: Option<String>,
) -> SetActiveResult {
    SetActiveResult {
        outcome,
        not_in_db,
        message,
    }
}

#[cfg(test)]
mod repo_info_tests {
    use super::*;

    #[test]
    fn repository_and_author_made_mod_info() {
        // dummy.zip из репозитория BeamNG и hotw0.3.0.zip, оформленный автором.
        let repo = RepoInfo::from_mod_data(
            "M9EL2DOCR",
            &serde_json::json!({
                "tagid": "M9EL2DOCR", "title": "Crash Test Dummy", "version_string": "1.1",
                "resource_id": 906, "username": "B25Mitch"
            }),
        );
        assert_eq!(repo.version.as_deref(), Some("1.1"));
        assert_eq!(repo.title.as_deref(), Some("Crash Test Dummy"));
        assert_eq!(repo.author.as_deref(), Some("B25Mitch"));
        assert_eq!(
            repo.page_url().as_deref(),
            Some("https://beamng.com/resources/906/")
        );

        let own = RepoInfo::from_mod_data(
            "HANDSONTHEWHEEL",
            &serde_json::json!({"name": "Hands on the Wheel", "version": "0.3.0", "author": "1Matzh"}),
        );
        assert_eq!(own.version.as_deref(), Some("0.3.0"));

        // В реальном db.json дата бывает Unix-числом, а не RFC3339.
        let numeric =
            RepoInfo::from_mod_data("X", &serde_json::json!({"last_update": 1_791_803_568i64}));
        assert_eq!(
            numeric.last_update.map(|date| date.timestamp()),
            Some(1_791_803_568)
        );
        assert_eq!(own.title.as_deref(), Some("Hands on the Wheel"));
        assert_eq!(own.author.as_deref(), Some("1Matzh"));
        assert_eq!(own.page_url(), None, "не из репозитория — страницы нет");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DB_5MODS: &str = include_str!("../../../fixtures/game/db-5mods-0.39.4.json");
    const DB_BMW_OFF: &str = include_str!("../../../fixtures/game/db-5mods-bmw-off-0.39.4.json");
    const DB_EMPTY: &str = include_str!("../../../fixtures/game/db-empty-0.39.4.json");

    fn temp_db(text: &str) -> (tempfile::TempDir, GameModDb) {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("db.json"), text).unwrap();
        let db = GameModDb::new(dir.path());
        (dir, db)
    }

    #[test]
    fn cyrillic_case_difference_still_finds_mod() {
        let (dir, db) = temp_db(r#"{"mods":{"МОД":{"filename":"мод.zip","active":true}}}"#);
        let outcome = db.set_active(&[("МОД.zip", false)], || false);
        assert_eq!(outcome.outcome, SetActiveOutcome::Done, "{outcome:?}");
        let text = fs::read_to_string(dir.path().join("db.json")).unwrap();
        assert!(text.contains("\"active\": false"), "{text}");
    }

    #[test]
    fn key_matches_game_lua_rules() {
        assert_eq!(key_of("Машина_BMW.zip"), "Машина_bmw");
        assert_eq!(key_of("mods/repo/BMW_M2_G87_inf.zip"), "bmw_m2_g87_inf");
        assert_eq!(key_of("BMW.ZIP"), "bmw");
    }

    #[test]
    fn reads_fixture_with_five_mods() {
        let (_dir, db) = temp_db(DB_5MODS);
        let read = db.read();
        assert!(read.exists);
        assert_eq!(read.mods.len(), 5);
        assert!(read.warnings.is_empty(), "{:?}", read.warnings);
        let bmw = read.find("BMW_M2_G87_inf.zip").unwrap();
        assert!(bmw.active);
        assert_eq!(bmw.mod_type, "vehicle");
        assert_eq!(bmw.file_size, Some(331_770_968));
        assert!(bmw.in_mods_root());
        assert!(read.find("unknown.zip").is_none());
    }

    #[test]
    fn reads_inactive_and_repo_data() {
        let (_dir, db) = temp_db(DB_BMW_OFF);
        let read = db.read();
        let bmw = read.find("BMW_M2_G87_inf.zip").unwrap();
        assert!(!bmw.active);

        let (_dir, db) = temp_db(DB_5MODS);
        let read = db.read();
        let repo = read.mods.iter().find(|entry| entry.repo.is_some()).unwrap();
        assert_eq!(repo.repo.as_ref().unwrap().unique_id, "M7P2C4JGW");
        assert_eq!(repo.repo.as_ref().unwrap().resource_id, Some(34443));
        assert_eq!(
            repo.repo.as_ref().unwrap().page_url().as_deref(),
            Some("https://beamng.com/resources/34443/")
        );
    }

    #[test]
    fn empty_db_is_valid() {
        let (_dir, db) = temp_db(DB_EMPTY);
        let read = db.read();
        assert_eq!(read.mods.len(), 0);
        assert!(read.warnings.is_empty());
        assert_eq!(read.format_version, Some(1.1));
    }

    #[test]
    fn garbage_is_unreadable() {
        let (_dir, db) = temp_db("{ не json");
        let read = db.read();
        assert!(read.unreadable);
        assert_eq!(read.warnings.len(), 1);

        let (_dir, db) = temp_db("[1,2]");
        assert!(db.read().unreadable);
    }

    #[test]
    fn bad_entries_are_skipped_with_warnings() {
        let text = r#"{
            "header": {"version": 1},
            "mods": {
                "bad": 5,
                "nofile": {"active": true},
                "ok": {"filename": "ok.zip"}
            }
        }"#;
        let (_dir, db) = temp_db(text);
        let read = db.read();
        assert_eq!(read.mods.len(), 1);
        assert_eq!(read.warnings.len(), 2);
        assert!(read.find("ok.zip").unwrap().active);
    }

    #[test]
    fn writing_changes_only_active_and_creates_backup() {
        let (dir, db) = temp_db(DB_5MODS);
        let outcome = db.set_active(&[("BMW_M2_G87_inf.zip", false)], || false);
        assert_eq!(outcome.outcome, SetActiveOutcome::Done);
        assert!(db.backup_path().exists());
        assert!(!dir.path().join("db.json.bmm-tmp").exists());

        let text = fs::read_to_string(db.path()).unwrap();
        assert!(!text.starts_with('\u{feff}'));
        assert!(text.contains("\"discord.gg\"") || text.contains("hashes"));

        let read = db.read();
        assert!(!read.find("BMW_M2_G87_inf.zip").unwrap().active);
        assert!(
            read.find("etk800.zip")
                .map(|entry| entry.active)
                .unwrap_or(true)
        );
    }

    #[test]
    fn unchanged_does_not_touch_file() {
        let (dir, db) = temp_db(DB_5MODS);
        let before = fs::read(dir.path().join("db.json")).unwrap();
        let outcome = db.set_active(&[("BMW_M2_G87_inf.zip", true)], || false);
        assert_eq!(outcome.outcome, SetActiveOutcome::Unchanged);
        assert_eq!(fs::read(dir.path().join("db.json")).unwrap(), before);
        assert!(!db.backup_path().exists());
    }

    #[test]
    fn game_running_blocks_writing() {
        let (_dir, db) = temp_db(DB_5MODS);
        assert_eq!(
            db.set_active(&[("BMW_M2_G87_inf.zip", false)], || true)
                .outcome,
            SetActiveOutcome::GameRunning
        );
    }

    #[test]
    fn missing_db_and_unknown_file_outcomes() {
        let dir = tempfile::tempdir().unwrap();
        let db = GameModDb::new(dir.path());
        assert_eq!(
            db.set_active(&[("a.zip", false)], || false).outcome,
            SetActiveOutcome::NoDb
        );

        let (_dir, db) = temp_db(DB_5MODS);
        let outcome = db.set_active(&[("missing.zip", false)], || false);
        assert_eq!(outcome.outcome, SetActiveOutcome::NotInDb);
        assert_eq!(outcome.not_in_db, vec!["missing.zip"]);

        let outcome = db.set_active(
            &[("BMW_M2_G87_inf.zip", false), ("missing.zip", false)],
            || false,
        );
        assert_eq!(outcome.outcome, SetActiveOutcome::Done);
        assert_eq!(outcome.not_in_db, vec!["missing.zip"]);
    }

    #[test]
    fn date_added_is_parsed() {
        let (dir, db) = temp_db(DB_5MODS);
        let _ = dir;
        let read = db.read();
        // Фикстура содержит dateAdded; проверяем, что дата не теряется.
        assert!(read.mods.iter().any(|entry| entry.date_added.is_some()));
    }
}

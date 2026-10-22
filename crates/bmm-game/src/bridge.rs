//! Помощник внутри игры (`bmm_bridge.zip`): команды и признак жизни через файлы
//! в папке пользователя игры, рядом с `mods`.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use chrono::{DateTime, FixedOffset};
use serde_json::{Map, Value, json};

use bmm_domain::atomic;
use bmm_domain::clock::{Clock, system_clock};

pub const MOD_FILE_NAME: &str = "bmm_bridge.zip";
pub const MOD_KEY: &str = "bmm_bridge";
pub const ALIVE_WINDOW_SECS: i64 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BridgeAnswer {
    pub ack_id: Option<String>,
    pub results: HashMap<String, String>,
    pub error: Option<String>,
}

impl BridgeAnswer {
    pub fn result(&self, key: &str) -> Option<&str> {
        self.results
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .map(|(_, value)| value.as_str())
    }
}

pub struct GameBridge {
    dir: PathBuf,
    now: Clock,
    seq: AtomicU64,
}

impl GameBridge {
    pub fn new(mods_dir: impl AsRef<Path>, now: Clock) -> Self {
        let full = std::path::absolute(mods_dir.as_ref())
            .unwrap_or_else(|_| mods_dir.as_ref().to_path_buf());
        let user_root = full.parent().unwrap_or(&full);
        Self {
            dir: user_root.join("bmm_bridge"),
            now,
            seq: AtomicU64::new(0),
        }
    }

    pub fn with_system_clock(mods_dir: impl AsRef<Path>) -> Self {
        Self::new(mods_dir, system_clock())
    }

    pub fn command_path(&self) -> PathBuf {
        self.dir.join("commands.json")
    }

    pub fn state_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    pub fn is_alive(&self) -> bool {
        let Some(state) = self.read_state() else {
            return false;
        };
        let Some(value) = state.get("alive") else {
            return false;
        };
        let alive = if let Some(number) = value.as_i64() {
            number
        } else if let Some(number) = value.as_f64() {
            if !number.is_finite() || number.abs() >= 1e11 {
                return false;
            }
            number as i64
        } else {
            return false;
        };
        (self.now)().timestamp().saturating_sub(alive).abs() < ALIVE_WINDOW_SECS
    }

    pub fn send(&self, wanted: &[(&str, bool)]) -> Option<String> {
        let moment = (self.now)();
        let serial = self.seq.fetch_add(1, Ordering::Relaxed) + 1;
        let id = format!("{}-{serial}", moment.timestamp_millis());
        let mut set = Map::new();
        for (file, enabled) in wanted {
            let key = crate::db::key_of(file);
            if key.is_empty() || key == MOD_KEY {
                continue;
            }
            set.insert(key, Value::Bool(*enabled));
        }
        let command = json!({
            "id": id,
            "time": moment.timestamp(),
            "set": set,
        });
        let bytes = serde_json::to_vec(&command).ok()?;
        if fs::create_dir_all(&self.dir).is_err() {
            return None;
        }
        let path = self.command_path();
        for attempt in 0..3 {
            match atomic::write_atomic(&path, &bytes) {
                Ok(()) => return Some(id),
                Err(_) if attempt < 2 => std::thread::sleep(Duration::from_millis(20)),
                Err(_) => return None,
            }
        }
        None
    }

    pub fn read_answer(&self) -> Option<BridgeAnswer> {
        let state = self.read_state()?;
        let ack_id = state
            .get("ackId")
            .and_then(Value::as_str)
            .map(str::to_string);
        let error = state
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string);
        let mut results = HashMap::new();
        if let Some(object) = state.get("results").and_then(Value::as_object) {
            for (key, value) in object {
                if let Some(text) = value.as_str() {
                    results.insert(key.clone(), text.to_string());
                }
            }
        }
        Some(BridgeAnswer {
            ack_id,
            results,
            error,
        })
    }

    fn read_state(&self) -> Option<Value> {
        let path = self.state_path();
        for attempt in 0..3 {
            let Ok(text) = fs::read_to_string(&path) else {
                if attempt < 2 {
                    std::thread::sleep(Duration::from_millis(15));
                    continue;
                }
                return None;
            };
            match serde_json::from_str::<Value>(&text) {
                Ok(value) => return Some(value),
                Err(_) if attempt < 2 => {
                    std::thread::sleep(Duration::from_millis(15));
                }
                Err(_) => return None,
            }
        }
        None
    }

    pub fn build_mod() -> Vec<u8> {
        let mut cursor = std::io::Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut cursor);
            let stamp = zip::DateTime::from_date_and_time(2026, 10, 15, 0, 0, 0)
                .expect("статическая дата архива");
            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .last_modified_time(stamp);
            for (name, bytes) in [
                (
                    "lua/ge/extensions/bmmBridge.lua",
                    include_bytes!("../resources/bmmBridge.lua").as_slice(),
                ),
                (
                    "scripts/bmm_bridge/modScript.lua",
                    include_bytes!("../resources/modScript.lua").as_slice(),
                ),
            ] {
                zip.start_file(name, options).expect("запись архива");
                zip.write_all(bytes).expect("запись ресурса");
            }
            zip.finish().expect("закрытие архива");
        }
        cursor.into_inner()
    }

    pub fn ensure_installed(mods_dir: impl AsRef<Path>) -> bool {
        let mods_dir = mods_dir.as_ref();
        if !mods_dir.is_dir() {
            return false;
        }
        let path = mods_dir.join(MOD_FILE_NAME);
        let bytes = Self::build_mod();
        if fs::read(&path).ok().as_deref() == Some(bytes.as_slice()) {
            return true;
        }
        atomic::write_atomic(&path, &bytes).is_ok()
    }

    pub fn now(&self) -> DateTime<FixedOffset> {
        (self.now)()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn moment(seconds: i64) -> DateTime<FixedOffset> {
        chrono::Utc
            .timestamp_opt(seconds, 0)
            .single()
            .unwrap()
            .fixed_offset()
    }

    fn setup() -> (tempfile::TempDir, PathBuf, GameBridge) {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        let clock: Clock = std::sync::Arc::new(|| moment(1_760_000_000));
        let bridge = GameBridge::new(&mods, clock);
        (temp, mods, bridge)
    }

    #[test]
    fn alive_window_integer_fraction_future_and_broken() {
        let (_temp, _mods, bridge) = setup();
        assert!(!bridge.is_alive(), "файла нет");
        fs::create_dir_all(bridge.state_path().parent().unwrap()).unwrap();
        for (text, expected) in [
            (r#"{"alive":1759999998}"#, true),
            (r#"{"alive":1759999998.0}"#, true),
            (r#"{"alive":1759999990}"#, false),
            (r#"{"alive":1760000100}"#, false),
            (r#"{"alive":"x"}"#, false),
            (r#"{"alive":17599"#, false),
            ("", false),
        ] {
            fs::write(bridge.state_path(), text).unwrap();
            assert_eq!(bridge.is_alive(), expected, "{text}");
        }
    }

    #[test]
    fn sends_keys_and_reads_answer() {
        let (_temp, _mods, bridge) = setup();
        let id = bridge
            .send(&[
                ("BMW_M2_G87_inf.zip", false),
                ("mudstone_valley.zip", true),
                ("bmm_bridge.zip", false),
                ("", true),
            ])
            .unwrap();
        assert!(id.starts_with("1760000000000-"));
        let command: Value =
            serde_json::from_str(&fs::read_to_string(bridge.command_path()).unwrap()).unwrap();
        assert_eq!(command["id"], id);
        assert_eq!(command["time"], 1_760_000_000);
        assert_eq!(command["set"]["bmw_m2_g87_inf"], false);
        assert_eq!(command["set"]["mudstone_valley"], true);
        assert!(command["set"].get("bmm_bridge").is_none());

        fs::write(
            bridge.state_path(),
            format!(
                r#"{{"alive":1760000000,"ackId":"{id}","results":{{"bmw_m2_g87_inf":"ok","ghost":"missing"}}}}"#
            ),
        )
        .unwrap();
        let answer = bridge.read_answer().unwrap();
        assert_eq!(answer.ack_id.as_deref(), Some(id.as_str()));
        assert_eq!(answer.result("BMW_M2_G87_INF"), Some("ok"));
        assert_eq!(answer.result("ghost"), Some("missing"));
        assert!(bridge.is_alive());
    }

    #[test]
    fn answer_with_error_and_broken_state() {
        let (_temp, _mods, bridge) = setup();
        fs::create_dir_all(bridge.state_path().parent().unwrap()).unwrap();
        fs::write(
            bridge.state_path(),
            r#"{"alive":1,"ackId":"7-1","error":"boom","results":{"a":"failed"}}"#,
        )
        .unwrap();
        let answer = bridge.read_answer().unwrap();
        assert_eq!(answer.error.as_deref(), Some("boom"));
        assert_eq!(answer.result("a"), Some("failed"));
        fs::write(bridge.state_path(), "{ битый").unwrap();
        assert!(bridge.read_answer().is_none());
    }

    #[test]
    fn builds_same_archive_and_installs_it() {
        let (_temp, mods, _bridge) = setup();
        let first = GameBridge::build_mod();
        let second = GameBridge::build_mod();
        assert_eq!(first, second);
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(first.clone())).unwrap();
        assert!(archive.by_name("lua/ge/extensions/bmmBridge.lua").is_ok());
        assert!(archive.by_name("scripts/bmm_bridge/modScript.lua").is_ok());

        assert!(GameBridge::ensure_installed(&mods));
        assert_eq!(fs::read(mods.join(MOD_FILE_NAME)).unwrap(), first);
        assert!(GameBridge::ensure_installed(&mods));
        assert!(!GameBridge::ensure_installed(mods.join("missing")));
    }
}

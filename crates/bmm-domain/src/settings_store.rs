use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde_json::{Map, Value};

use crate::atomic;
use crate::clock::{Clock, system_clock};
use crate::settings::{AppSettings, BackgroundStyle, BackgroundTheme, UiMode};

const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct SettingsLoadResult {
    pub settings: AppSettings,
    pub warnings: Vec<String>,
    /// Файл не прочитался: сохранять поверх него нельзя, иначе настройки
    /// по умолчанию затрут настоящие.
    pub save_blocked: bool,
}

impl SettingsLoadResult {
    pub fn had_problems(&self) -> bool {
        !self.warnings.is_empty()
    }
}

pub struct SettingsStore {
    path: PathBuf,
    now: Clock,
    gate: Mutex<()>,
}

impl SettingsStore {
    pub fn new(path: impl Into<PathBuf>, now: Clock) -> Self {
        let path = path.into();
        atomic::cleanup_stale_temps(&path);
        Self {
            path,
            now,
            gate: Mutex::new(()),
        }
    }

    pub fn with_system_clock(path: impl Into<PathBuf>) -> Self {
        Self::new(path, system_clock())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> SettingsLoadResult {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        if !self.path.exists() {
            return SettingsLoadResult {
                settings: AppSettings::default(),
                warnings: Vec::new(),
                save_blocked: false,
            };
        }

        if let Ok(metadata) = fs::metadata(&self.path)
            && metadata.len() > MAX_BYTES
        {
            return self.quarantine("файл настроек слишком большой");
        }

        let text = match fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) => {
                return SettingsLoadResult {
                    settings: AppSettings::default(),
                    warnings: vec![format!("не удалось прочитать настройки: {error}")],
                    save_blocked: true,
                };
            }
        };

        let value: Value = match crate::json5_safe::from_str(&text) {
            Ok(value) => value,
            Err(_) => return self.quarantine("файл настроек повреждён"),
        };

        let Some(root) = value.as_object() else {
            return self.quarantine("файл настроек имеет неверный формат");
        };

        let mut warnings = Vec::new();
        let defaults = AppSettings::default();

        let schema_version = int_field(root, "SchemaVersion", 1, &mut warnings);
        let mut disabled_sources = strs_field(root, "DisabledSources", &mut warnings);
        if schema_version < 2 && !disabled_sources.iter().any(|source| source == "modland") {
            disabled_sources.push("modland".to_string());
        }

        let settings = AppSettings {
            schema_version: 2,
            allow_downloads: bool_field(
                root,
                "AllowDownloads",
                defaults.allow_downloads,
                &mut warnings,
            ),
            mods_folder: str_field(root, "ModsFolder", &mut warnings),
            user_folder: str_field(root, "UserFolder", &mut warnings),
            downloads_folder: str_field(root, "DownloadsFolder", &mut warnings),
            delete_downloads_after_install: bool_field(
                root,
                "DeleteDownloadsAfterInstall",
                defaults.delete_downloads_after_install,
                &mut warnings,
            ),
            check_ttl_hours: int_field(
                root,
                "CheckTtlHours",
                defaults.check_ttl_hours,
                &mut warnings,
            ),
            auto_check_on_start: bool_field(
                root,
                "AutoCheckOnStart",
                defaults.auto_check_on_start,
                &mut warnings,
            ),
            check_app_updates: bool_field(
                root,
                "CheckAppUpdates",
                defaults.check_app_updates,
                &mut warnings,
            ),
            auto_check_every_hours: int_field(
                root,
                "AutoCheckEveryHours",
                defaults.auto_check_every_hours,
                &mut warnings,
            ),
            check_disabled_mods: bool_field(
                root,
                "CheckDisabledMods",
                defaults.check_disabled_mods,
                &mut warnings,
            ),
            disabled_sources,
            keep_backups: int_field(root, "KeepBackups", defaults.keep_backups, &mut warnings),
            max_card_checks_per_source: int_field(
                root,
                "MaxCardChecksPerSource",
                defaults.max_card_checks_per_source,
                &mut warnings,
            ),
            liquid_glass: bool_field(root, "LiquidGlass", defaults.liquid_glass, &mut warnings),
            mode: UiMode::from_i32(int_field(root, "Mode", 0, &mut warnings)),
            theme: BackgroundTheme::from_i32(int_field(root, "Theme", 0, &mut warnings)),
            background: BackgroundStyle::from_i32(int_field(
                root,
                "Background",
                defaults.background.as_i32(),
                &mut warnings,
            )),
            ui_animations: bool_field(root, "UiAnimations", defaults.ui_animations, &mut warnings),
            animated_background: bool_field(
                root,
                "AnimatedBackground",
                defaults.animated_background,
                &mut warnings,
            ),
            fireflies: bool_field(root, "Fireflies", defaults.fireflies, &mut warnings),
            background_tilt: bool_field(
                root,
                "BackgroundTilt",
                defaults.background_tilt,
                &mut warnings,
            ),
            active_mods_warn_above: int_field(
                root,
                "ActiveModsWarnAbove",
                defaults.active_mods_warn_above,
                &mut warnings,
            ),
            first_run_done: bool_field(
                root,
                "FirstRunDone",
                defaults.first_run_done,
                &mut warnings,
            ),
            slow_offer_declined: bool_field(
                root,
                "SlowOfferDeclined",
                defaults.slow_offer_declined,
                &mut warnings,
            ),
            watch_folders: strs_field(root, "WatchFolders", &mut warnings),
            window_width: int_field(root, "WindowWidth", 0, &mut warnings),
            window_height: int_field(root, "WindowHeight", 0, &mut warnings),
            window_maximized: bool_field(root, "WindowMaximized", false, &mut warnings),
        }
        .normalize();

        SettingsLoadResult {
            settings,
            warnings,
            save_blocked: false,
        }
    }

    pub fn save(&self, settings: &AppSettings) -> std::io::Result<()> {
        let _gate = self.gate.lock().unwrap_or_else(|error| error.into_inner());
        let json =
            serde_json::to_string_pretty(&settings.normalize()).map_err(std::io::Error::other)?;
        atomic::write_atomic(&self.path, json.as_bytes())
    }

    fn quarantine(&self, why: &str) -> SettingsLoadResult {
        let quarantined = atomic::quarantine_file(&self.path, (self.now)());
        let blocked = quarantined.is_none();
        let warning = match quarantined {
            Some(target) => {
                let name = target
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                format!("{why}: сохранён как {name}, использованы настройки по умолчанию")
            }
            None => format!(
                "{why}, файл убрать не удалось; использованы настройки по умолчанию, сохранение остановлено"
            ),
        };

        SettingsLoadResult {
            settings: AppSettings::default(),
            warnings: vec![warning],
            save_blocked: blocked,
        }
    }
}

fn bool_field(
    root: &Map<String, Value>,
    name: &str,
    default: bool,
    warnings: &mut Vec<String>,
) -> bool {
    match root.get(name) {
        None => default,
        Some(Value::Bool(value)) => *value,
        Some(_) => {
            warnings.push(format!(
                "{name}: ожидалось true или false, использовано значение по умолчанию"
            ));
            default
        }
    }
}

fn int_field(
    root: &Map<String, Value>,
    name: &str,
    default: i32,
    warnings: &mut Vec<String>,
) -> i32 {
    match root.get(name) {
        None => default,
        Some(Value::Number(number)) => match number.as_i64() {
            Some(value) if i32::try_from(value).is_ok() => value as i32,
            _ => {
                warnings.push(format!(
                    "{name}: ожидалось целое число, использовано значение по умолчанию"
                ));
                default
            }
        },
        Some(_) => {
            warnings.push(format!(
                "{name}: ожидалось целое число, использовано значение по умолчанию"
            ));
            default
        }
    }
}

fn str_field(root: &Map<String, Value>, name: &str, warnings: &mut Vec<String>) -> Option<String> {
    match root.get(name) {
        None | Some(Value::Null) => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => {
            warnings.push(format!("{name}: ожидалась строка, значение сброшено"));
            None
        }
    }
}

fn strs_field(root: &Map<String, Value>, name: &str, warnings: &mut Vec<String>) -> Vec<String> {
    let Some(value) = root.get(name) else {
        return Vec::new();
    };
    if value.is_null() {
        return Vec::new();
    }
    let Some(items) = value.as_array() else {
        warnings.push(format!("{name}: ожидался список, значение сброшено"));
        return Vec::new();
    };

    let mut result = Vec::new();
    for item in items {
        match item {
            Value::String(value) => result.push(value.clone()),
            _ => warnings.push(format!("{name}: элемент не строка пропущен")),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use std::sync::Arc;

    fn fixed_clock() -> Clock {
        let moment = Utc
            .with_ymd_and_hms(2026, 10, 21, 12, 0, 0)
            .unwrap()
            .fixed_offset();
        Arc::new(move || moment)
    }

    fn store_with(dir: &std::path::Path, text: &str) -> SettingsStore {
        let path = dir.join("settings.json");
        fs::write(&path, text).unwrap();
        SettingsStore::new(path, fixed_clock())
    }

    #[cfg(windows)]
    #[test]
    fn unremovable_corrupt_file_blocks_saving() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, "{ это не json").unwrap();
        let _lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        let store = SettingsStore::new(path, fixed_clock());
        let result = store.load();
        assert!(result.save_blocked, "{:?}", result.warnings);
        assert!(
            result.warnings[0].contains("остановлено"),
            "{:?}",
            result.warnings
        );
    }

    #[test]
    fn unreadable_settings_file_blocks_saving() {
        // Папка вместо файла: прочитать нельзя, но и перезаписывать нельзя —
        // иначе настройки по умолчанию затрут файл, который станет доступен позже.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::create_dir_all(&path).unwrap();
        let store = SettingsStore::new(path, fixed_clock());
        let result = store.load();
        assert!(result.save_blocked);
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn missing_file_uses_defaults_without_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"), fixed_clock());
        let result = store.load();
        assert!(!result.had_problems());
        assert!(!result.settings.allow_downloads);
        assert_eq!(result.settings.check_ttl_hours, 6);
    }

    #[test]
    fn old_files_get_default_download_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(
            dir.path(),
            r#"{ "SchemaVersion": 2, "AllowDownloads": true }"#,
        );
        let result = store.load();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.settings.downloads_folder, None);
        assert!(!result.settings.delete_downloads_after_install);
    }

    #[test]
    fn too_deep_settings_are_quarantined_not_a_crash() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(dir.path(), &"[".repeat(100_000));
        let result = store.load();
        assert!(!result.warnings.is_empty());
        assert_eq!(result.settings, AppSettings::default());
    }

    #[test]
    fn old_schema_adds_modland_once() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(
            dir.path(),
            r#"{ "SchemaVersion": 1, "DisabledSources": ["stmods"] }"#,
        );
        let result = store.load();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.settings.disabled_sources, vec!["modland", "stmods"]);
        assert_eq!(result.settings.schema_version, 2);
    }

    #[test]
    fn schema_two_without_sources_keeps_modland_enabled() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(dir.path(), r#"{ "SchemaVersion": 2 }"#);
        let result = store.load();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(result.settings.disabled_sources.is_empty());
        assert!(result.settings.is_source_enabled("modland"));
    }

    #[test]
    fn exact_true_enables_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(dir.path(), r#"{ "AllowDownloads": true }"#);
        let result = store.load();
        assert!(result.settings.allow_downloads);
        assert_eq!(result.settings.disabled_sources, vec!["modland"]);
    }

    #[test]
    fn wrong_types_are_reset_with_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(
            dir.path(),
            r#"{
                "AllowDownloads": "yes",
                "CheckTtlHours": "много",
                "KeepBackups": 7,
                "ModsFolder": 5,
                "DisabledSources": ["modland", 3]
            }"#,
        );
        let result = store.load();
        assert!(!result.settings.allow_downloads);
        assert_eq!(result.settings.check_ttl_hours, 6);
        assert_eq!(result.settings.keep_backups, 7);
        assert_eq!(result.settings.mods_folder, None);
        assert_eq!(result.settings.disabled_sources, vec!["modland"]);
        assert_eq!(result.warnings.len(), 4, "{:?}", result.warnings);
    }

    #[test]
    fn only_literal_true_enables_downloads() {
        for text in [
            r#"{ "AllowDownloads": 1 }"#,
            r#"{ "AllowDownloads": "true" }"#,
            r#"{ "AllowDownloads": "True" }"#,
            r#"{ "AllowDownloads": [true] }"#,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let store = store_with(dir.path(), text);
            let result = store.load();
            assert!(!result.settings.allow_downloads, "{text}");
            assert!(!result.warnings.is_empty(), "{text}");
        }
    }

    #[test]
    fn comments_unknown_fields_and_trailing_commas_are_tolerated() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(
            dir.path(),
            "{ // моё\n \"Unknown\": {\"a\":1}, \"KeepBackups\": 5, }",
        );
        let result = store.load();
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert_eq!(result.settings.keep_backups, 5);
    }

    #[test]
    fn corrupted_file_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(dir.path(), r#"{ "AllowDownloads": tr"#);
        let result = store.load();
        assert_eq!(result.warnings.len(), 1);
        assert!(!dir.path().join("settings.json").exists());
        let bad: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bad-"))
            .collect();
        assert_eq!(bad.len(), 1);
        assert!(!result.settings.allow_downloads);
    }

    #[test]
    fn second_quarantine_keeps_first_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{ "KeepBackups": tr"#).unwrap();
        let store = SettingsStore::new(path.clone(), fixed_clock());
        assert!(store.load().had_problems());
        fs::write(&path, r#"{ "KeepBackups": 5"#).unwrap();
        assert!(store.load().had_problems());

        let backups: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".bad-"))
            .map(|entry| fs::read_to_string(entry.path()).unwrap())
            .collect();
        assert_eq!(backups.len(), 2, "{backups:?}");
        assert!(
            backups.contains(&r#"{ "KeepBackups": tr"#.to_string()),
            "{backups:?}"
        );
        assert!(
            backups.contains(&r#"{ "KeepBackups": 5"#.to_string()),
            "{backups:?}"
        );
    }

    #[test]
    fn oversized_file_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        let result = SettingsStore::new(path.clone(), fixed_clock()).load();
        assert!(result.had_problems());
        assert!(!path.exists());
        assert_eq!(
            fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_name().to_string_lossy().contains(".bad-"))
                .count(),
            1
        );
    }

    #[test]
    fn parallel_saves_leave_readable_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(SettingsStore::new(
            dir.path().join("settings.json"),
            fixed_clock(),
        ));
        let threads: Vec<_> = (0..8)
            .map(|index| {
                let store = Arc::clone(&store);
                std::thread::spawn(move || {
                    for step in 0..20 {
                        let loaded = store.load();
                        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
                        let settings = AppSettings {
                            keep_backups: 1 + (index * 20 + step) % 20,
                            check_ttl_hours: 1 + (index % 24),
                            ..AppSettings::default()
                        };
                        store.save(&settings).unwrap();
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }

        let loaded = store.load();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert!(
            (1..=20).contains(&loaded.settings.keep_backups),
            "{}",
            loaded.settings.keep_backups
        );
        for entry in fs::read_dir(dir.path()).unwrap().filter_map(Result::ok) {
            assert!(
                !entry
                    .file_name()
                    .to_string_lossy()
                    .to_ascii_lowercase()
                    .ends_with(".tmp"),
                "{:?}",
                entry.file_name()
            );
        }
    }

    #[test]
    fn non_object_roots_use_defaults() {
        for text in ["[1,2]", "", "null", "42"] {
            let dir = tempfile::tempdir().unwrap();
            let store = store_with(dir.path(), text);
            let result = store.load();
            assert!(result.had_problems(), "{text}");
            assert!(!result.settings.allow_downloads, "{text}");
        }
    }

    #[test]
    fn new_check_switches_default_on_for_old_settings() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(dir.path(), r#"{ "CheckTtlHours": 6 }"#);
        let settings = store.load().settings;
        assert!(settings.check_disabled_mods);
    }

    #[test]
    fn round_trip_preserves_all_fields() {
        let dir = tempfile::tempdir().unwrap();
        let store = SettingsStore::new(dir.path().join("settings.json"), fixed_clock());
        let settings = AppSettings {
            allow_downloads: true,
            mods_folder: Some("C:\\Моды".to_string()),
            user_folder: Some("C:\\BeamNG".to_string()),
            downloads_folder: Some("C:\\Скачанные обновления".to_string()),
            delete_downloads_after_install: true,
            check_ttl_hours: 12,
            auto_check_on_start: false,
            check_app_updates: false,
            auto_check_every_hours: 24,
            check_disabled_mods: false,
            disabled_sources: vec!["stmods".to_string()],
            keep_backups: 8,
            max_card_checks_per_source: 200,
            liquid_glass: false,
            mode: UiMode::Light,
            theme: BackgroundTheme::Forest,
            background: BackgroundStyle::Nebula,
            ui_animations: false,
            animated_background: false,
            fireflies: false,
            background_tilt: true,
            active_mods_warn_above: 40,
            first_run_done: true,
            slow_offer_declined: true,
            watch_folders: vec!["C:\\Загрузки".to_string()],
            window_width: 1280,
            window_height: 720,
            window_maximized: true,
            ..AppSettings::default()
        }
        .normalize();

        store.save(&settings).unwrap();
        let loaded = store.load();
        assert!(loaded.warnings.is_empty(), "{:?}", loaded.warnings);
        assert_eq!(loaded.settings, settings);
        assert!(!dir.path().join("settings.json.tmp").exists());
    }

    #[test]
    fn wrong_enum_values_fall_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store = store_with(
            dir.path(),
            r#"{ "Theme": 42, "Background": 42, "Mode": "Light", "WindowWidth": true }"#,
        );
        let result = store.load();
        assert_eq!(result.settings.theme, BackgroundTheme::Night);
        assert_eq!(result.settings.background, BackgroundStyle::Accent);
        assert_eq!(result.settings.mode, UiMode::Dark);
        assert_eq!(result.settings.window_width, 0);
        assert_eq!(result.warnings.len(), 2, "{:?}", result.warnings);
    }
}

use serde::{Serialize, Serializer};
use std::time::Duration;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundTheme {
    Night = 0,
    Deep = 1,
    Dusk = 2,
    Forest = 3,
    Graphite = 4,
}

impl BackgroundTheme {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Deep,
            2 => Self::Dusk,
            3 => Self::Forest,
            4 => Self::Graphite,
            _ => Self::Night,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl Serialize for BackgroundTheme {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(self.as_i32())
    }
}

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    Dark = 0,
    Light = 1,
    System = 2,
}

impl UiMode {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Light,
            2 => Self::System,
            _ => Self::Dark,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl Serialize for UiMode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_i32(self.as_i32())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AppSettings {
    pub schema_version: i32,
    pub allow_downloads: bool,
    pub mods_folder: Option<String>,
    pub user_folder: Option<String>,
    pub check_ttl_hours: i32,
    pub auto_check_on_start: bool,
    pub check_app_updates: bool,
    pub auto_check_every_hours: i32,
    pub disabled_sources: Vec<String>,
    pub keep_backups: i32,
    pub max_card_checks_per_source: i32,
    pub liquid_glass: bool,
    pub mode: UiMode,
    pub theme: BackgroundTheme,
    pub ui_animations: bool,
    pub animated_background: bool,
    pub fireflies: bool,
    pub background_tilt: bool,
    pub active_mods_warn_above: i32,
    pub first_run_done: bool,
    pub slow_offer_declined: bool,
    pub watch_folders: Vec<String>,
    pub window_width: i32,
    pub window_height: i32,
    pub window_maximized: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: 2,
            allow_downloads: false,
            mods_folder: None,
            user_folder: None,
            check_ttl_hours: 6,
            auto_check_on_start: true,
            check_app_updates: true,
            auto_check_every_hours: 0,
            disabled_sources: vec!["modland".to_string()],
            keep_backups: 3,
            max_card_checks_per_source: 150,
            liquid_glass: true,
            mode: UiMode::Dark,
            theme: BackgroundTheme::Night,
            ui_animations: true,
            animated_background: true,
            fireflies: true,
            background_tilt: false,
            active_mods_warn_above: 30,
            first_run_done: false,
            slow_offer_declined: false,
            watch_folders: Vec::new(),
            window_width: 0,
            window_height: 0,
            window_maximized: false,
        }
    }
}

impl AppSettings {
    pub fn normalize(&self) -> Self {
        Self {
            schema_version: 2,
            allow_downloads: self.allow_downloads,
            mods_folder: clean(self.mods_folder.as_deref()),
            user_folder: clean(self.user_folder.as_deref()),
            check_ttl_hours: self.check_ttl_hours.clamp(1, 168),
            auto_check_on_start: self.auto_check_on_start,
            check_app_updates: self.check_app_updates,
            auto_check_every_hours: if self.auto_check_every_hours <= 0 {
                0
            } else {
                self.auto_check_every_hours.clamp(1, 168)
            },
            disabled_sources: normalized_sources(&self.disabled_sources),
            keep_backups: self.keep_backups.clamp(1, 20),
            max_card_checks_per_source: self.max_card_checks_per_source.clamp(5, 500),
            liquid_glass: self.liquid_glass,
            mode: self.mode,
            theme: self.theme,
            ui_animations: self.ui_animations,
            animated_background: self.animated_background,
            fireflies: self.fireflies,
            background_tilt: self.background_tilt,
            active_mods_warn_above: self.active_mods_warn_above.clamp(0, 1000),
            first_run_done: self.first_run_done,
            slow_offer_declined: self.slow_offer_declined,
            watch_folders: normalized_folders(&self.watch_folders),
            window_width: if (1000..=20000).contains(&self.window_width) {
                self.window_width
            } else {
                0
            },
            window_height: if (680..=20000).contains(&self.window_height) {
                self.window_height
            } else {
                0
            },
            window_maximized: self.window_maximized,
        }
    }

    pub fn is_source_enabled(&self, id: &str) -> bool {
        !self.disabled_sources.iter().any(|source| source == id)
    }

    pub fn to_check_options(&self) -> CheckOptions {
        CheckOptions {
            ttl: Duration::from_secs(self.check_ttl_hours.clamp(1, 168) as u64 * 3600),
            max_cards: self.max_card_checks_per_source.clamp(5, 500),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckOptions {
    pub ttl: Duration,
    pub max_cards: i32,
}

fn clean(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_folder(value: &str) -> Option<String> {
    let mut trimmed = value.trim();
    while trimmed.len() > 3 && (trimmed.ends_with('\\') || trimmed.ends_with('/')) {
        trimmed = &trimmed[..trimmed.len() - 1];
    }
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalized_sources(values: &[String]) -> Vec<String> {
    let mut result: Vec<String> = values
        .iter()
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty())
        .collect();
    result.sort();
    result.dedup();
    result
}

fn normalized_folders(values: &[String]) -> Vec<String> {
    let mut result: Vec<String> = Vec::new();
    for value in values {
        let Some(folder) = clean_folder(value) else {
            continue;
        };
        if result
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&folder))
        {
            continue;
        }
        result.push(folder);
        if result.len() == 10 {
            break;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_old_app() {
        let settings = AppSettings::default();
        assert_eq!(settings.schema_version, 2);
        assert!(!settings.allow_downloads);
        assert_eq!(settings.check_ttl_hours, 6);
        assert_eq!(settings.disabled_sources, vec!["modland"]);
        assert_eq!(settings.keep_backups, 3);
        assert_eq!(settings.max_card_checks_per_source, 150);
        assert_eq!(settings.mode, UiMode::Dark);
        assert_eq!(settings.theme, BackgroundTheme::Night);
        assert!(settings.liquid_glass);
        assert!(settings.auto_check_on_start);
        assert!(settings.check_app_updates);
    }

    #[test]
    fn normalize_clamps_and_cleans() {
        let settings = AppSettings {
            check_ttl_hours: 0,
            auto_check_every_hours: -3,
            keep_backups: 999,
            max_card_checks_per_source: 1,
            active_mods_warn_above: 100_000,
            window_width: 999,
            window_height: 679,
            mods_folder: Some("   ".to_string()),
            user_folder: Some(" C:\\Games\\BeamNG ".to_string()),
            disabled_sources: vec![
                "STMods".to_string(),
                "modland".to_string(),
                "stmods".to_string(),
                " ".to_string(),
            ],
            watch_folders: vec!["C:\\Downloads\\".to_string(), "c:\\downloads".to_string()],
            ..AppSettings::default()
        }
        .normalize();

        assert_eq!(settings.check_ttl_hours, 1);
        assert_eq!(settings.auto_check_every_hours, 0);
        assert_eq!(settings.keep_backups, 20);
        assert_eq!(settings.max_card_checks_per_source, 5);
        assert_eq!(settings.active_mods_warn_above, 1000);
        assert_eq!(settings.window_width, 0);
        assert_eq!(settings.window_height, 0);
        assert_eq!(settings.mods_folder, None);
        assert_eq!(settings.user_folder.as_deref(), Some("C:\\Games\\BeamNG"));
        assert_eq!(settings.disabled_sources, vec!["modland", "stmods"]);
        assert_eq!(settings.watch_folders, vec!["C:\\Downloads"]);
    }

    #[test]
    fn enums_serialize_as_numbers() {
        let json = serde_json::to_string(&AppSettings::default()).unwrap();
        assert!(json.contains("\"Mode\":0"), "{json}");
        assert!(json.contains("\"Theme\":0"), "{json}");
        assert!(json.contains("\"SchemaVersion\":2"), "{json}");
    }
}

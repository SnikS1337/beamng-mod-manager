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

/// Палитра фона: акцентная или самостоятельный градиент.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackgroundStyle {
    Accent = 0,
    Aurora = 1,
    Sunset = 2,
    Ocean = 3,
    Nebula = 4,
    Mono = 5,
}

impl BackgroundStyle {
    pub fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Aurora,
            2 => Self::Sunset,
            3 => Self::Ocean,
            4 => Self::Nebula,
            5 => Self::Mono,
            _ => Self::Accent,
        }
    }

    pub fn as_i32(self) -> i32 {
        self as i32
    }
}

impl Serialize for BackgroundStyle {
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
    /// `None` — системные «Загрузки».
    pub downloads_folder: Option<String>,
    pub delete_downloads_after_install: bool,
    pub check_ttl_hours: i32,
    pub auto_check_on_start: bool,
    pub check_app_updates: bool,
    pub auto_check_every_hours: i32,
    /// Проверять обновления и для модов, которые сейчас выключены.
    pub check_disabled_mods: bool,
    pub disabled_sources: Vec<String>,
    /// Включённые сайты из [`OPT_IN_SOURCES`]: они по умолчанию выключены, и
    /// [`Self::disabled_sources`] про них не говорит.
    pub enabled_sources: Vec<String>,
    pub keep_backups: i32,
    pub max_card_checks_per_source: i32,
    pub liquid_glass: bool,
    pub mode: UiMode,
    pub theme: BackgroundTheme,
    pub background: BackgroundStyle,
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
            downloads_folder: None,
            delete_downloads_after_install: false,
            check_ttl_hours: 6,
            auto_check_on_start: true,
            check_app_updates: true,
            auto_check_every_hours: 0,
            check_disabled_mods: true,
            // Все сайты включены: Modland и ModHub за Cloudflare иногда просят
            // проверку «вы человек?», но без них поиск и проверка беднее.
            disabled_sources: Vec::new(),
            enabled_sources: Vec::new(),
            keep_backups: 3,
            max_card_checks_per_source: 150,
            liquid_glass: true,
            mode: UiMode::Dark,
            theme: BackgroundTheme::Night,
            background: BackgroundStyle::Accent,
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
            downloads_folder: clean(self.downloads_folder.as_deref()),
            delete_downloads_after_install: self.delete_downloads_after_install,
            check_ttl_hours: self.check_ttl_hours.clamp(1, 168),
            auto_check_on_start: self.auto_check_on_start,
            check_app_updates: self.check_app_updates,
            auto_check_every_hours: if self.auto_check_every_hours <= 0 {
                0
            } else {
                self.auto_check_every_hours.clamp(1, 168)
            },
            check_disabled_mods: self.check_disabled_mods,
            disabled_sources: normalized_sources(&self.disabled_sources)
                .into_iter()
                .filter(|id| !is_opt_in(id))
                .collect(),
            enabled_sources: normalized_sources(&self.enabled_sources)
                .into_iter()
                .filter(|id| is_opt_in(id))
                .collect(),
            keep_backups: self.keep_backups.clamp(1, 20),
            max_card_checks_per_source: self.max_card_checks_per_source.clamp(5, 500),
            liquid_glass: self.liquid_glass,
            mode: self.mode,
            theme: self.theme,
            background: self.background,
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

    /// Сайт выключен: снят человеком или из тех, что включают сами.
    pub fn source_off(&self, id: &str) -> bool {
        let id = id.trim().to_lowercase();
        if is_opt_in(&id) {
            !self
                .enabled_sources
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&id))
        } else {
            self.disabled_sources
                .iter()
                .any(|known| known.eq_ignore_ascii_case(&id))
        }
    }

    /// Все выключенные сайты — для проверки, поиска и окна настроек.
    pub fn off_sources(&self) -> Vec<String> {
        let mut off: Vec<String> = self
            .disabled_sources
            .iter()
            .map(|id| id.trim().to_lowercase())
            .filter(|id| !id.is_empty() && !is_opt_in(id))
            .collect();
        off.extend(
            OPT_IN_SOURCES
                .iter()
                .filter(|id| self.source_off(id))
                .map(|id| id.to_string()),
        );
        off.sort();
        off.dedup();
        off
    }

    /// Обратно из списка «выключено» окна настроек: сайты по выбору — в
    /// [`Self::enabled_sources`], остальные — в [`Self::disabled_sources`].
    pub fn set_off_sources(&mut self, off: &[String]) {
        let off = normalized_sources(off);
        self.enabled_sources = OPT_IN_SOURCES
            .iter()
            .filter(|id| !off.iter().any(|known| known == *id))
            .map(|id| id.to_string())
            .collect();
        self.disabled_sources = off.into_iter().filter(|id| !is_opt_in(id)).collect();
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

/// Сайты, которые по умолчанию выключены: их включают в настройках сами.
/// Telegram-канал — не сайт с каталогом, и скачивать из него можно только в Telegram.
pub const OPT_IN_SOURCES: &[&str] = &["telegram"];

fn is_opt_in(id: &str) -> bool {
    OPT_IN_SOURCES
        .iter()
        .any(|known| known.eq_ignore_ascii_case(id.trim()))
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
            .any(|existing| existing.to_lowercase() == folder.to_lowercase())
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
    fn opt_in_sources_are_off_until_turned_on() {
        let mut settings = AppSettings::default();
        assert!(settings.source_off("telegram"), "по умолчанию выключен");
        assert!(!settings.source_off("modland"));
        assert_eq!(settings.off_sources(), ["telegram"]);

        // Окно настроек присылает весь список «выключено».
        settings.set_off_sources(&["modland".to_string()]);
        assert!(!settings.source_off("Telegram"));
        assert!(settings.source_off("modland"));
        assert_eq!(settings.enabled_sources, ["telegram"]);
        assert_eq!(settings.disabled_sources, ["modland"]);
        assert_eq!(settings.off_sources(), ["modland"]);

        settings.set_off_sources(&[" Telegram ".to_string(), "stmods".to_string()]);
        assert!(settings.source_off("telegram"));
        assert!(settings.enabled_sources.is_empty());
        assert_eq!(settings.disabled_sources, ["stmods"]);
    }

    #[test]
    fn normalize_keeps_lists_apart() {
        let settings = AppSettings {
            disabled_sources: vec!["telegram".to_string(), "STMods".to_string()],
            enabled_sources: vec!["Telegram".to_string(), "modland".to_string()],
            ..AppSettings::default()
        }
        .normalize();
        // «Выключен» для сайта по выбору решает только список включённых.
        assert_eq!(settings.disabled_sources, ["stmods"]);
        assert_eq!(settings.enabled_sources, ["telegram"]);
        assert!(!settings.source_off("telegram"));
    }

    #[test]
    fn defaults() {
        let settings = AppSettings::default();
        assert_eq!(settings.schema_version, 2);
        assert!(!settings.allow_downloads);
        assert_eq!(settings.check_ttl_hours, 6);
        assert!(settings.disabled_sources.is_empty(), "все сайты включены");
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
            downloads_folder: Some("   ".to_string()),
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
        assert_eq!(settings.downloads_folder, None);
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

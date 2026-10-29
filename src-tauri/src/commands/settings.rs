//! Чтение и сохранение настроек приложения.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use bmm_checks::UpdateCheckOptions;
use bmm_domain::settings::{AppSettings, BackgroundStyle, BackgroundTheme, UiMode};

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub allow_downloads: bool,
    pub mods_folder: Option<String>,
    pub user_folder: Option<String>,
    pub downloads_folder: Option<String>,
    pub delete_downloads_after_install: bool,
    pub check_ttl_hours: i32,
    pub auto_check_on_start: bool,
    pub check_app_updates: bool,
    pub auto_check_every_hours: i32,
    pub check_disabled_mods: bool,
    pub disabled_sources: Vec<String>,
    pub keep_backups: i32,
    pub max_card_checks_per_source: i32,
    pub liquid_glass: bool,
    #[ts(type = "UiMode")]
    pub mode: String,
    #[ts(type = "BackgroundTheme")]
    pub theme: String,
    #[ts(type = "BackgroundStyle")]
    pub background: String,
    pub ui_animations: bool,
    pub animated_background: bool,
    pub fireflies: bool,
    pub background_tilt: bool,
    pub active_mods_warn_above: i32,
    pub first_run_done: bool,
    pub watch_folders: Vec<String>,
}

impl SettingsDto {
    fn from_settings(settings: &AppSettings) -> Self {
        Self {
            allow_downloads: settings.allow_downloads,
            mods_folder: settings.mods_folder.clone(),
            user_folder: settings.user_folder.clone(),
            downloads_folder: settings.downloads_folder.clone(),
            delete_downloads_after_install: settings.delete_downloads_after_install,
            check_ttl_hours: settings.check_ttl_hours,
            auto_check_on_start: settings.auto_check_on_start,
            check_app_updates: settings.check_app_updates,
            auto_check_every_hours: settings.auto_check_every_hours,
            check_disabled_mods: settings.check_disabled_mods,
            disabled_sources: settings.disabled_sources.clone(),
            keep_backups: settings.keep_backups,
            max_card_checks_per_source: settings.max_card_checks_per_source,
            liquid_glass: settings.liquid_glass,
            mode: match settings.mode {
                UiMode::Dark => "dark",
                UiMode::Light => "light",
                UiMode::System => "system",
            }
            .to_string(),
            theme: match settings.theme {
                BackgroundTheme::Night => "night",
                BackgroundTheme::Deep => "deep",
                BackgroundTheme::Dusk => "dusk",
                BackgroundTheme::Forest => "forest",
                BackgroundTheme::Graphite => "graphite",
            }
            .to_string(),
            background: match settings.background {
                BackgroundStyle::Accent => "accent",
                BackgroundStyle::Aurora => "aurora",
                BackgroundStyle::Sunset => "sunset",
                BackgroundStyle::Ocean => "ocean",
                BackgroundStyle::Nebula => "nebula",
                BackgroundStyle::Mono => "mono",
            }
            .to_string(),
            ui_animations: settings.ui_animations,
            animated_background: settings.animated_background,
            fireflies: settings.fireflies,
            background_tilt: settings.background_tilt,
            active_mods_warn_above: settings.active_mods_warn_above,
            first_run_done: settings.first_run_done,
            watch_folders: settings.watch_folders.clone(),
        }
    }

    fn apply_to(&self, settings: &mut AppSettings) {
        settings.allow_downloads = self.allow_downloads;
        settings.mods_folder = self.mods_folder.clone();
        settings.user_folder = self.user_folder.clone();
        settings.downloads_folder = self.downloads_folder.clone();
        settings.delete_downloads_after_install = self.delete_downloads_after_install;
        settings.check_ttl_hours = self.check_ttl_hours;
        settings.auto_check_on_start = self.auto_check_on_start;
        settings.check_app_updates = self.check_app_updates;
        settings.auto_check_every_hours = self.auto_check_every_hours;
        settings.check_disabled_mods = self.check_disabled_mods;
        settings.disabled_sources = self.disabled_sources.clone();
        settings.keep_backups = self.keep_backups;
        settings.max_card_checks_per_source = self.max_card_checks_per_source;
        settings.liquid_glass = self.liquid_glass;
        settings.mode = match self.mode.as_str() {
            "light" => UiMode::Light,
            "system" => UiMode::System,
            _ => UiMode::Dark,
        };
        settings.theme = match self.theme.as_str() {
            "deep" => BackgroundTheme::Deep,
            "dusk" => BackgroundTheme::Dusk,
            "forest" => BackgroundTheme::Forest,
            "graphite" => BackgroundTheme::Graphite,
            _ => BackgroundTheme::Night,
        };
        settings.background = match self.background.as_str() {
            "aurora" => BackgroundStyle::Aurora,
            "sunset" => BackgroundStyle::Sunset,
            "ocean" => BackgroundStyle::Ocean,
            "nebula" => BackgroundStyle::Nebula,
            "mono" => BackgroundStyle::Mono,
            _ => BackgroundStyle::Accent,
        };
        settings.ui_animations = self.ui_animations;
        settings.animated_background = self.animated_background;
        settings.fireflies = self.fireflies;
        settings.background_tilt = self.background_tilt;
        settings.active_mods_warn_above = self.active_mods_warn_above;
        settings.first_run_done = self.first_run_done;
        settings.watch_folders = self.watch_folders.clone();
    }
}

#[tauri::command]
pub fn app_data_folder(state: tauri::State<'_, Arc<AppState>>) -> String {
    state.paths.root().display().to_string()
}

#[tauri::command]
pub async fn settings_load(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<SettingsDto, AppError> {
    Ok(SettingsDto::from_settings(&state.settings.load().settings))
}

#[tauri::command]
pub async fn settings_save(
    state: tauri::State<'_, Arc<AppState>>,
    settings: SettingsDto,
) -> Result<SettingsDto, AppError> {
    save_settings(&state, &settings)
}

/// Сохранить то, что пришло из окна, поверх текущих настроек. Поля, которых в окне
/// нет (размер окна, отказ от медленной проверки), не трогаются. Возвращает то, что
/// реально легло на диск — после приведения к допустимым значениям.
static SAVE_LOCK: Mutex<()> = Mutex::new(());

pub(crate) fn save_settings(
    state: &AppState,
    settings: &SettingsDto,
) -> Result<SettingsDto, AppError> {
    // Два окна/два быстрых сохранения не должны делать read-modify-write
    // из одного и того же старого снимка и терять второе изменение.
    let _save = SAVE_LOCK.lock().unwrap_or_else(|error| error.into_inner());
    let loaded = state.settings.load();
    if loaded.save_blocked {
        return Err(AppError::new(
            "settings",
            "Не удалось прочитать файл настроек. Чтобы не потерять данные, сохранение остановлено: проверьте доступ к файлу и перезапустите программу.",
        ));
    }
    let mut current = loaded.settings;
    settings.apply_to(&mut current);
    sanitize_folders(&mut current);
    state
        .settings
        .save(&current)
        .map_err(|error| AppError::new("settings", error.to_string()))?;
    let saved = state.settings.load().settings;
    if let Some(runtime) = &state.update {
        runtime
            .coordinator
            .update_check_options(UpdateCheckOptions::from_settings(&saved));
    }
    Ok(SettingsDto::from_settings(&saved))
}

/// Папка, которую можно держать в настройках: полный путь, не корень диска и не
/// системный каталог. Иначе удаление скачанных архивов и `create_dir_all`
/// получают слишком широкие права.
pub(crate) fn acceptable_folder(path: &Path) -> bool {
    if !path.is_absolute()
        || path.parent().is_none()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return false;
    }
    let lower = path.to_string_lossy().to_lowercase();
    let roots: Vec<String> = [
        "SystemRoot",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
    ]
    .iter()
    .filter_map(std::env::var_os)
    .map(|value| value.to_string_lossy().to_lowercase())
    .filter(|root| !root.is_empty())
    .collect();
    !roots.iter().any(|root| {
        lower == *root
            || lower.starts_with(&format!("{root}\\"))
            || lower.starts_with(&format!("{root}/"))
    })
}

fn sanitize_folder(raw: &str) -> Option<String> {
    let path = PathBuf::from(raw.trim());
    acceptable_folder(&path).then(|| path.display().to_string())
}

fn sanitize_folders(settings: &mut AppSettings) {
    settings.downloads_folder = settings
        .downloads_folder
        .as_deref()
        .and_then(sanitize_folder);
    let mut seen = HashSet::new();
    settings.watch_folders = settings
        .watch_folders
        .iter()
        .filter_map(|folder| sanitize_folder(folder))
        .filter(|folder| seen.insert(folder.to_lowercase()))
        .take(10)
        .collect();
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmm_domain::settings_store::SettingsStore;

    #[test]
    fn unsafe_folders_are_dropped_from_settings() {
        let mut settings = AppSettings {
            downloads_folder: Some("C:\\".to_string()),
            watch_folders: vec![
                "relative".to_string(),
                "D:\\Downloads".to_string(),
                "d:\\downloads".to_string(),
            ],
            ..AppSettings::default()
        };
        sanitize_folders(&mut settings);
        assert_eq!(
            settings.downloads_folder, None,
            "корень диска не папка загрузок"
        );
        assert_eq!(settings.watch_folders, vec!["D:\\Downloads".to_string()]);
        assert!(!acceptable_folder(Path::new("relative")));
        assert!(!acceptable_folder(Path::new("D:\\Downloads\\..")));
    }

    #[test]
    fn every_setting_survives_the_window_round_trip() {
        // Настройки должны сохраняться между запусками: поле, забытое в переводе для окна,
        // молча сбрасывалось бы при каждом сохранении.
        let mut changed = AppSettings::default();
        changed.allow_downloads = !changed.allow_downloads;
        changed.mods_folder = Some("D:/Games/BeamNG/mods".to_string());
        changed.user_folder = Some("D:/Games/BeamNG".to_string());
        changed.downloads_folder = Some("D:/Downloads/BeamNG".to_string());
        changed.delete_downloads_after_install = true;
        changed.check_ttl_hours = 7;
        changed.auto_check_on_start = !changed.auto_check_on_start;
        changed.check_app_updates = !changed.check_app_updates;
        changed.auto_check_every_hours = 5;
        changed.disabled_sources = vec!["modland".to_string()];
        changed.keep_backups = 9;
        changed.max_card_checks_per_source = 33;
        changed.liquid_glass = !changed.liquid_glass;
        changed.mode = UiMode::Light;
        changed.theme = BackgroundTheme::Forest;
        changed.background = BackgroundStyle::Nebula;
        changed.ui_animations = !changed.ui_animations;
        changed.animated_background = !changed.animated_background;
        changed.fireflies = !changed.fireflies;
        changed.background_tilt = !changed.background_tilt;
        changed.active_mods_warn_above = 123;
        changed.first_run_done = !changed.first_run_done;
        changed.watch_folders = vec!["C:/Downloads".to_string()];

        let dto = SettingsDto::from_settings(&changed);
        let mut restored = AppSettings::default();
        dto.apply_to(&mut restored);
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            serde_json::to_value(&changed).unwrap()
        );
    }

    #[test]
    fn save_keeps_fields_the_window_does_not_know() {
        let temp = tempfile::tempdir().unwrap();
        let state = crate::test_support::test_state(&temp, temp.path());
        let mut stored = state.settings.load().settings;
        stored.window_width = 1600;
        stored.window_height = 900;
        stored.window_maximized = true;
        stored.slow_offer_declined = true;
        state.settings.save(&stored).unwrap();

        let mut dto = SettingsDto::from_settings(&stored);
        dto.theme = "forest".to_string();
        save_settings(&state, &dto).unwrap();

        let saved = state.settings.load().settings;
        assert_eq!(saved.theme, BackgroundTheme::Forest);
        assert_eq!((saved.window_width, saved.window_height), (1600, 900));
        assert!(saved.window_maximized);
        assert!(saved.slow_offer_declined);
    }

    #[test]
    fn save_returns_what_really_landed_on_disk() {
        // Число в окне можно стереть или ввести мимо границ: на диск попадает
        // допустимое, и окно сразу показывает именно его, а не введённое.
        let temp = tempfile::tempdir().unwrap();
        let state = crate::test_support::test_state(&temp, temp.path());
        let mut dto = SettingsDto::from_settings(&state.settings.load().settings);
        dto.check_ttl_hours = -5;
        dto.keep_backups = 0;
        dto.max_card_checks_per_source = 100_000;
        dto.active_mods_warn_above = 5000;
        dto.auto_check_every_hours = -3;
        dto.disabled_sources = vec![
            " Modland ".to_string(),
            "modland".to_string(),
            String::new(),
        ];
        dto.theme = "незнакомая".to_string();
        dto.mode = String::new();

        let back = save_settings(&state, &dto).unwrap();
        assert_eq!(back.check_ttl_hours, 1);
        assert_eq!(back.keep_backups, 1);
        assert_eq!(back.max_card_checks_per_source, 500);
        assert_eq!(back.active_mods_warn_above, 1000);
        assert_eq!(back.auto_check_every_hours, 0);
        assert_eq!(back.disabled_sources, vec!["modland".to_string()]);
        assert_eq!((back.theme.as_str(), back.mode.as_str()), ("night", "dark"));

        // И после перезапуска — то же самое.
        let reopened = SettingsStore::with_system_clock(state.paths.settings());
        let again = SettingsDto::from_settings(&reopened.load().settings);
        assert_eq!(
            serde_json::to_value(&again).unwrap(),
            serde_json::to_value(&back).unwrap()
        );
    }

    #[test]
    fn every_theme_and_mode_keeps_its_name() {
        for theme in ["night", "deep", "dusk", "forest", "graphite"] {
            for mode in ["dark", "light", "system"] {
                let mut dto = SettingsDto::from_settings(&AppSettings::default());
                dto.theme = theme.to_string();
                dto.mode = mode.to_string();
                let mut settings = AppSettings::default();
                dto.apply_to(&mut settings);
                let back = SettingsDto::from_settings(&settings);
                assert_eq!((back.theme.as_str(), back.mode.as_str()), (theme, mode));
            }
        }
    }
}

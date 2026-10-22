//! Чтение и сохранение настроек приложения.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use bmm_checks::UpdateCheckOptions;
use bmm_domain::settings::{AppSettings, BackgroundTheme, UiMode};

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
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
    pub mode: String,
    pub theme: String,
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
            check_ttl_hours: settings.check_ttl_hours,
            auto_check_on_start: settings.auto_check_on_start,
            check_app_updates: settings.check_app_updates,
            auto_check_every_hours: settings.auto_check_every_hours,
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
        settings.check_ttl_hours = self.check_ttl_hours;
        settings.auto_check_on_start = self.auto_check_on_start;
        settings.check_app_updates = self.check_app_updates;
        settings.auto_check_every_hours = self.auto_check_every_hours;
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
    let mut current = state.settings.load().settings;
    settings.apply_to(&mut current);
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

use serde::Serialize;
use tauri::State;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapDto {
    pub app_version: String,
    pub os: &'static str,
    pub arch: &'static str,
    pub data_dir: String,
    pub settings_exists: bool,
    pub library_exists: bool,
}

#[tauri::command]
pub fn app_bootstrap(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<BootstrapDto, AppError> {
    let paths = &state.paths;
    Ok(BootstrapDto {
        app_version: app.package_info().version.to_string(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        data_dir: paths.root().display().to_string(),
        settings_exists: paths.settings().exists(),
        library_exists: paths.library().exists(),
    })
}

//! Обновление самой программы: проверка latest.json, загрузка и перезапуск.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use bmm_app_release::{
    AppVersion, PrepareOptions, UpdateStage, UpdateState, check_url, prepare_update, read_skipped,
    write_in_place,
};

use crate::error::AppError;
use crate::state::{AppState, AppUpdateRuntime};

/// Сбой проверки не должен навсегда отравлять состояние обновления.
fn lock_runtime(runtime: &Mutex<AppUpdateRuntime>) -> MutexGuard<'_, AppUpdateRuntime> {
    runtime.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateDto {
    pub current_version: String,
    pub state: UpdateState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppUpdateProgress {
    progress: f64,
}

fn current_version() -> AppVersion {
    AppVersion::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|| AppVersion::new(0, 4, 0, 0))
}

pub(crate) fn auto_check(state: &Arc<AppState>) {
    let Some(runtime) = state.app_update.clone() else {
        return;
    };
    let mut guard = lock_runtime(&runtime);
    let local = current_version();
    let skipped = read_skipped(&guard.skipped_path);
    guard.state = check_url(
        &local,
        &guard.manifest_url,
        &guard.downloader,
        skipped.as_ref(),
    );
}

fn dto(state: UpdateState) -> AppUpdateDto {
    AppUpdateDto {
        current_version: current_version().display(),
        state,
    }
}

#[tauri::command]
pub async fn app_update_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppUpdateDto, AppError> {
    let runtime = state
        .app_update
        .clone()
        .ok_or_else(|| AppError::new("update", "Обновление программы недоступно."))?;
    let state = lock_runtime(&runtime).state.clone();
    Ok(dto(state))
}

#[tauri::command]
pub async fn app_update_check(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppUpdateDto, AppError> {
    let runtime = state
        .app_update
        .clone()
        .ok_or_else(|| AppError::new("update", "Обновление программы недоступно."))?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = lock_runtime(&runtime);
        guard.cancel.reset();
        let local = current_version();
        let skipped = read_skipped(&guard.skipped_path);
        let next = check_url(
            &local,
            &guard.manifest_url,
            &guard.downloader,
            skipped.as_ref(),
        );
        guard.state = next.clone();
        dto(next)
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))
}

#[tauri::command]
pub async fn app_update_download(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppUpdateDto, AppError> {
    let runtime = state
        .app_update
        .clone()
        .ok_or_else(|| AppError::new("update", "Обновление программы недоступно."))?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut guard = lock_runtime(&runtime);
        let Some(manifest) = guard.state.manifest.clone() else {
            guard.state = UpdateState::failed("Сначала проверьте обновление.");
            return dto(guard.state.clone());
        };
        guard.cancel.reset();
        guard.state.stage = UpdateStage::Downloading;
        guard.state.progress = 0.0;
        guard.state.error = None;
        let app_for_progress = app.clone();
        let mut progress = |value: f64| {
            let _ =
                app_for_progress.emit("app-update://event", AppUpdateProgress { progress: value });
        };
        let downloaded = guard.downloader.download_into(
            &manifest,
            &guard.update_dir,
            &mut progress,
            &guard.cancel,
        );
        let result: Result<(), String> = match downloaded {
            Ok(downloaded) => match std::env::current_exe() {
                Ok(current) => prepare_update(
                    &downloaded.path,
                    &guard.update_dir,
                    &manifest,
                    &current_version(),
                    &current,
                    &|_| None,
                    &PrepareOptions::default(),
                )
                .map(|prepared| {
                    guard.prepared = Some(prepared);
                    guard.state.stage = UpdateStage::ReadyToRestart;
                    guard.state.progress = 1.0;
                    guard.state.error = None;
                })
                .map_err(|error| error.to_string()),
                Err(error) => Err(error.to_string()),
            },
            Err(error) => Err(error.to_string()),
        };
        if let Err(error) = result {
            guard.prepared = None;
            guard.state.stage = UpdateStage::Failed;
            guard.state.error = Some(error);
        }
        dto(guard.state.clone())
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))
}

#[tauri::command]
pub async fn app_update_cancel(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppUpdateDto, AppError> {
    let runtime = state
        .app_update
        .clone()
        .ok_or_else(|| AppError::new("update", "Обновление программы недоступно."))?;
    let mut guard = lock_runtime(&runtime);
    guard.cancel.cancel();
    guard.state.stage = UpdateStage::Failed;
    guard.state.error = Some("Загрузка обновления отменена.".to_string());
    Ok(dto(guard.state.clone()))
}

#[tauri::command]
pub async fn app_update_restart(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), AppError> {
    let runtime = state
        .app_update
        .clone()
        .ok_or_else(|| AppError::new("update", "Обновление программы недоступно."))?;
    let guard = lock_runtime(&runtime);
    let prepared = guard
        .prepared
        .as_ref()
        .ok_or_else(|| AppError::new("update", "Обновление ещё не скачано."))?;
    let mut command = std::process::Command::new(&prepared.new_exe);
    command
        .arg("--install-update")
        .arg(std::process::id().to_string())
        .arg("--target")
        .arg(&prepared.current_exe)
        .arg("--backup")
        .arg(&prepared.backup);
    command
        .spawn()
        .map_err(|error| AppError::new("update", error.to_string()))?;
    drop(guard);
    app.exit(0);
    Ok(())
}

pub struct InstallerArgs {
    old_pid: u32,
    target: PathBuf,
    backup: PathBuf,
}

pub fn installer_args() -> Option<InstallerArgs> {
    let args: Vec<String> = std::env::args().collect();
    let value = |flag: &str| {
        args.iter()
            .position(|arg| *arg == flag)
            .and_then(|index| args.get(index + 1))
            .cloned()
    };
    Some(InstallerArgs {
        old_pid: value("--install-update")?.parse().ok()?,
        target: PathBuf::from(value("--target")?),
        backup: PathBuf::from(value("--backup")?),
    })
}

pub fn run_installer(arguments: InstallerArgs) {
    wait_for_exit(arguments.old_pid, Duration::from_secs(30));
    let self_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return,
    };
    if write_in_place(&self_exe, &arguments.target, &arguments.backup).is_err() {
        let _ = std::process::Command::new(&arguments.target)
            .arg("--update-failed")
            .spawn();
        return;
    }
    if std::process::Command::new(&arguments.target)
        .spawn()
        .is_err()
    {
        bmm_app_release::restore_in_place(&arguments.backup, &arguments.target);
        let _ = std::process::Command::new(&arguments.target).spawn();
    }
}

#[cfg(windows)]
fn wait_for_exit(pid: u32, timeout: Duration) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::OpenProcess;

    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    let start = Instant::now();
    while start.elapsed() < timeout {
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return;
        }
        unsafe { CloseHandle(handle) };
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(not(windows))]
fn wait_for_exit(_pid: u32, _timeout: Duration) {
    std::thread::sleep(Duration::from_secs(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_installer_arguments() {
        let args = [
            "BeamModManager.exe",
            "--install-update",
            "42",
            "--target",
            "C:\\app\\BeamModManager.exe",
            "--backup",
            "C:\\app\\previous.exe",
        ];
        let value = |flag: &str| {
            args.iter()
                .position(|arg| *arg == flag)
                .and_then(|index| args.get(index + 1))
                .map(|value| value.to_string())
        };
        assert_eq!(value("--install-update").unwrap(), "42");
        assert_eq!(value("--target").unwrap(), "C:\\app\\BeamModManager.exe");
        assert_eq!(value("--backup").unwrap(), "C:\\app\\previous.exe");
    }

    #[test]
    fn current_version_is_parsed() {
        assert!(current_version().display().starts_with("1.0"));
    }
}

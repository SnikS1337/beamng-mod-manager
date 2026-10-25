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

/// Значение флага: `--flag value`.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == flag)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

/// Аргументы режима установщика. `--backup` передаёт только Rust-версия; C#-версия 0.3.x
/// запускает новую с `--install-update <pid> --target <exe>`, и копия прежней тогда кладётся
/// туда же, куда её клала сама 0.3.x: `%LOCALAPPDATA%\BeamModManager\update\previous.exe`.
fn parse_installer_args(
    args: &[String],
    default_backup: impl FnOnce() -> Option<PathBuf>,
) -> Option<InstallerArgs> {
    let old_pid = flag_value(args, "--install-update")?.parse().ok()?;
    let target = PathBuf::from(flag_value(args, "--target")?);
    let backup = match flag_value(args, "--backup") {
        Some(path) => PathBuf::from(path),
        None => default_backup()?,
    };
    Some(InstallerArgs {
        old_pid,
        target,
        backup,
    })
}

pub fn installer_args() -> Option<InstallerArgs> {
    let args: Vec<String> = std::env::args().collect();
    parse_installer_args(&args, || {
        bmm_domain::paths::AppPaths::resolve()
            .ok()
            .map(|paths| paths.update_dir().join("previous.exe"))
    })
}

/// Код выхода новой версии, которая сама вернула прежнюю: откатывать второй раз не нужно.
const ROLLED_BACK_EXIT_CODE: i32 = 3;
/// Сколько ждать, пока новая версия покажет окно.
const START_WAIT: Duration = Duration::from_secs(60);

/// Новая версия вышла, не показав окна. 0 — закрыли обычным образом, 3 — уже откатилась сама.
fn start_failed(exit_code: Option<i32>) -> bool {
    !matches!(exit_code, Some(0) | Some(ROLLED_BACK_EXIT_CODE))
}

#[cfg_attr(not(windows), allow(dead_code))]
fn started_event_name(installer_pid: u32) -> String {
    // Как в 0.3.x: так прежняя и новая версии понимают друг друга в обе стороны.
    format!("Local\\BeamModManager.Started.{installer_pid}")
}

/// Новая версия, запущенная из папки обновления: ждёт выхода прежней, записывает себя в тот же
/// файл (прежнюю — в копию), запускает программу и ждёт её окна до минуты. Программа упала
/// раньше — прежняя возвращается из копии и запускается с `--rolled-back`.
pub fn run_installer(arguments: InstallerArgs) {
    wait_for_exit(arguments.old_pid, Duration::from_secs(30));
    let self_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return,
    };
    if let Err(error) = write_in_place(&self_exe, &arguments.target, &arguments.backup) {
        let _ = std::process::Command::new(&arguments.target)
            .arg("--update-failed")
            .arg(error.to_string())
            .spawn();
        return;
    }
    let started = StartedSignal::create(std::process::id());
    let child = std::process::Command::new(&arguments.target)
        .arg("--after-update")
        .arg(std::process::id().to_string())
        .spawn();
    let mut child = match child {
        Ok(child) => child,
        Err(_) => {
            roll_back(&arguments);
            return;
        }
    };
    let begin = Instant::now();
    while begin.elapsed() < START_WAIT {
        if started.wait(Duration::from_millis(250)) {
            return;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if start_failed(status.code()) {
                    roll_back(&arguments);
                }
                return;
            }
            Ok(None) => {}
            Err(_) => return,
        }
    }
}

fn roll_back(arguments: &InstallerArgs) {
    if bmm_app_release::restore_in_place(&arguments.backup, &arguments.target) {
        let _ = std::process::Command::new(&arguments.target)
            .arg("--rolled-back")
            .arg(current_version().display())
            .spawn();
    }
}

/// Запуск после самообновления: `--after-update <pid установщика>`.
pub fn after_update_parent() -> Option<u32> {
    let args: Vec<String> = std::env::args().collect();
    flag_value(&args, "--after-update")?.parse().ok()
}

/// Новая версия не запустилась и вернула эту: её больше не предлагаем.
pub fn remember_rolled_back(skipped_path: &std::path::Path) {
    let args: Vec<String> = std::env::args().collect();
    if let Some(version) = flag_value(&args, "--rolled-back").and_then(|v| AppVersion::parse(&v)) {
        let _ = bmm_app_release::write_skipped(skipped_path, version);
    }
}

/// Окно показалось: сказать установщику «запустилась» и убрать прежний exe и скачанное.
/// До этого из них возвращали бы прежнюю версию.
pub fn on_healthy(update_dir: PathBuf) {
    if let Some(parent) = after_update_parent() {
        StartedSignal::set(parent);
    }
    std::thread::spawn(move || {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        // Программа, запущенная прямо из папки обновления, свою папку не удаляет.
        if exe.starts_with(&update_dir) {
            return;
        }
        // Установщик выходит сразу после сигнала; даём ему отпустить свой exe.
        std::thread::sleep(Duration::from_secs(3));
        bmm_app_release::cleanup(&exe, &update_dir);
    });
}

#[cfg(windows)]
struct StartedSignal(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(windows)]
impl StartedSignal {
    fn create(installer_pid: u32) -> Self {
        use windows_sys::Win32::System::Threading::CreateEventW;
        let name = wide(&started_event_name(installer_pid));
        Self(unsafe { CreateEventW(std::ptr::null(), 1, 0, name.as_ptr()) })
    }

    fn wait(&self, timeout: Duration) -> bool {
        use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
        use windows_sys::Win32::System::Threading::WaitForSingleObject;
        if self.0.is_null() {
            std::thread::sleep(timeout);
            return false;
        }
        unsafe { WaitForSingleObject(self.0, timeout.as_millis() as u32) == WAIT_OBJECT_0 }
    }

    fn set(installer_pid: u32) {
        use windows_sys::Win32::Foundation::CloseHandle;
        use windows_sys::Win32::System::Threading::{OpenEventW, SetEvent};
        const EVENT_MODIFY_STATE: u32 = 0x0002;
        let name = wide(&started_event_name(installer_pid));
        let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr()) };
        if handle.is_null() {
            return;
        }
        unsafe {
            SetEvent(handle);
            CloseHandle(handle);
        }
    }
}

#[cfg(windows)]
impl Drop for StartedSignal {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(self.0) };
        }
    }
}

#[cfg(not(windows))]
struct StartedSignal;

#[cfg(not(windows))]
impl StartedSignal {
    fn create(_installer_pid: u32) -> Self {
        Self
    }

    fn wait(&self, timeout: Duration) -> bool {
        std::thread::sleep(timeout);
        false
    }

    fn set(_installer_pid: u32) {}
}

/// Ждём, пока прежний процесс действительно выйдет. `OpenProcess` в цикле не годится:
/// дескриптор вышедшего процесса открывается, пока его держит кто-то ещё.
#[cfg(windows)]
fn wait_for_exit(pid: u32, timeout: Duration) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

    const SYNCHRONIZE: u32 = 0x0010_0000;
    let handle = unsafe { OpenProcess(SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return; // уже вышел
    }
    unsafe {
        WaitForSingleObject(handle, timeout.as_millis() as u32);
        CloseHandle(handle);
    }
}

#[cfg(not(windows))]
fn wait_for_exit(_pid: u32, _timeout: Duration) {
    std::thread::sleep(Duration::from_secs(1));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    #[test]
    fn update_banner_and_rust_agree_on_fields_and_stages() {
        // Плашка обновления читает этот снимок; лишнее или пропавшее поле — пустая плашка.
        let manifest = format!(
            r#"{{"version":"9.9.9","url":"https://github.com/x/y/releases/download/v9.9.9/a.exe","size":5000000,"sha256":"{}","notes":"Исправления","published":"2026-10-20T10:00:00+03:00"}}"#,
            "a".repeat(64)
        );
        let state = bmm_app_release::check_json(&current_version(), &manifest, None);
        let value = serde_json::to_value(dto(state)).unwrap();
        crate::test_support::assert_matches_ts(&value, "appUpdate.ts", "AppUpdateSnapshot");
        crate::test_support::assert_matches_ts(&value["state"], "appUpdate.ts", "AppUpdateState");
        let manifest = &value["state"]["manifest"];
        crate::test_support::assert_matches_ts(manifest, "appUpdate.ts", "AppUpdateManifest");
        // Версия — строкой «9.9.9», а не разобранным числом: окно её только печатает.
        assert_eq!(manifest["version"], "9.9.9");
        assert_eq!(value["state"]["stage"], "available");

        let stages = crate::test_support::ts_union("appUpdate.ts", "AppUpdateStage");
        for stage in [
            UpdateStage::Idle,
            UpdateStage::Checking,
            UpdateStage::UpToDate,
            UpdateStage::Available,
            UpdateStage::Downloading,
            UpdateStage::ReadyToRestart,
            UpdateStage::Failed,
        ] {
            let name = serde_json::to_value(stage).unwrap();
            assert!(
                stages.contains(name.as_str().unwrap()),
                "окно не знает стадию {name}"
            );
        }
    }

    #[test]
    fn parses_installer_arguments() {
        let args = strings(&[
            "BeamModManager.exe",
            "--install-update",
            "42",
            "--target",
            "C:\\app\\BeamModManager.exe",
            "--backup",
            "C:\\app\\previous.exe",
        ]);
        let parsed = parse_installer_args(&args, || None).unwrap();
        assert_eq!(parsed.old_pid, 42);
        assert_eq!(parsed.target, PathBuf::from("C:\\app\\BeamModManager.exe"));
        assert_eq!(parsed.backup, PathBuf::from("C:\\app\\previous.exe"));
    }

    #[test]
    fn legacy_launch_without_backup_uses_default_copy() {
        // Так новую версию запускает C# 0.3.12: без --backup.
        let args = strings(&[
            "BeamModManager.exe",
            "--install-update",
            "7",
            "--target",
            "C:\\app\\BeamModManager.exe",
        ]);
        let parsed = parse_installer_args(&args, || {
            Some(PathBuf::from("D:\\data\\update\\previous.exe"))
        })
        .unwrap();
        assert_eq!(parsed.old_pid, 7);
        assert_eq!(
            parsed.backup,
            PathBuf::from("D:\\data\\update\\previous.exe")
        );
    }

    #[test]
    fn ordinary_launch_is_not_installer() {
        assert!(parse_installer_args(&strings(&["BeamModManager.exe"]), || None).is_none());
        let bad_pid = strings(&["x", "--install-update", "abc", "--target", "t"]);
        assert!(parse_installer_args(&bad_pid, || Some(PathBuf::from("b"))).is_none());
    }

    #[test]
    fn crash_before_window_means_rollback() {
        assert!(!start_failed(Some(0)));
        assert!(!start_failed(Some(ROLLED_BACK_EXIT_CODE)));
        assert!(start_failed(Some(101)));
        assert!(start_failed(Some(1)));
        assert!(start_failed(None));
    }

    #[test]
    fn started_event_matches_legacy_name() {
        assert_eq!(
            started_event_name(1234),
            "Local\\BeamModManager.Started.1234"
        );
    }

    #[test]
    fn current_version_is_parsed() {
        assert!(current_version().display().starts_with("1.0"));
    }
}

//! Обновление самой программы: проверка latest.json, загрузка и перезапуск.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chrono::SecondsFormat;
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

use bmm_app_release::{
    AppVersion, ManifestInput, PrepareOptions, UpdateStage, UpdateState, check_with_cancel,
    prepare_update, read_skipped, write_in_place,
};

use crate::error::AppError;
use crate::state::{AppState, AppUpdateRuntime};

/// Сбой проверки не должен навсегда отравлять состояние обновления.
fn lock_runtime(runtime: &Mutex<AppUpdateRuntime>) -> MutexGuard<'_, AppUpdateRuntime> {
    runtime.lock().unwrap_or_else(|poison| poison.into_inner())
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "AppUpdateSnapshot")]
pub struct AppUpdateDto {
    pub current_version: String,
    pub state: AppUpdateStateDto,
}

/// То же, что `UpdateState` из `bmm-app-release`, но с датами строками:
/// окно разбирает их само, а типы для окна рождаются здесь.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "AppUpdateState")]
pub struct AppUpdateStateDto {
    #[ts(type = "AppUpdateStage")]
    pub stage: String,
    pub manifest: Option<AppUpdateManifestDto>,
    pub progress: f64,
    pub error: Option<String>,
    pub checked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "AppUpdateManifest")]
pub struct AppUpdateManifestDto {
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub notes: Option<String>,
    pub published: Option<String>,
}

impl AppUpdateStateDto {
    fn from_state(state: UpdateState) -> Self {
        Self {
            stage: match state.stage {
                UpdateStage::Idle => "idle",
                UpdateStage::Checking => "checking",
                UpdateStage::UpToDate => "upToDate",
                UpdateStage::Available => "available",
                UpdateStage::Downloading => "downloading",
                UpdateStage::ReadyToRestart => "readyToRestart",
                UpdateStage::Failed => "failed",
            }
            .to_string(),
            manifest: state.manifest.map(|manifest| AppUpdateManifestDto {
                version: manifest.version.display(),
                url: manifest.url.to_string(),
                size: manifest.size,
                sha256: manifest.sha256,
                notes: manifest.notes,
                published: manifest
                    .published
                    .map(|at| at.to_rfc3339_opts(SecondsFormat::AutoSi, true)),
            }),
            progress: state.progress,
            error: state.error,
            checked_at: state
                .checked_at
                .map(|at| at.to_rfc3339_opts(SecondsFormat::AutoSi, true)),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppUpdateProgress {
    progress: f64,
}

/// Итог проверки уходит в окно: автопроверка не возвращает результат командой.
const STATE_EVENT: &str = "app-update://state";

fn current_version() -> AppVersion {
    AppVersion::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|| AppVersion::new(0, 4, 0, 0))
}

pub(crate) fn auto_check(app: &AppHandle, state: &Arc<AppState>) {
    let Some(runtime) = state.app_update.clone() else {
        return;
    };
    let snapshot = check_update(&runtime);
    let _ = app.emit(STATE_EVENT, snapshot);
}

fn check_update(runtime: &Mutex<AppUpdateRuntime>) -> AppUpdateDto {
    // Сеть не держим под замком: иначе состояние и «Отменить» ждут до 30 секунд.
    let (local, manifest_url, downloader, skipped_path, cancel) = {
        let mut guard = lock_runtime(runtime);
        // Пока идёт скачивание или другая проверка, эту не начинаем.
        if matches!(
            guard.state.stage,
            UpdateStage::Downloading | UpdateStage::Checking
        ) {
            return dto(guard.state.clone());
        }
        guard.cancel.reset();
        guard.state.stage = UpdateStage::Checking;
        guard.state.error = None;
        (
            current_version(),
            guard.manifest_url.clone(),
            guard.downloader.clone(),
            guard.skipped_path.clone(),
            guard.cancel.clone(),
        )
    };
    let skipped = read_skipped(&skipped_path);
    let next = check_with_cancel(
        &local,
        ManifestInput::url(&manifest_url),
        Some(&downloader),
        skipped.as_ref(),
        &cancel,
    );

    let mut guard = lock_runtime(runtime);
    // Пока шла сеть, состояние могли сменить: отмена, скачивание или новая проверка.
    if guard.state.stage != UpdateStage::Checking {
        return dto(guard.state.clone());
    }
    if cancel.is_cancelled() {
        guard.state = UpdateState::failed("Проверка обновления отменена.");
        return dto(guard.state.clone());
    }
    guard.state = next;
    dto(guard.state.clone())
}

fn dto(state: UpdateState) -> AppUpdateDto {
    AppUpdateDto {
        current_version: current_version().display(),
        state: AppUpdateStateDto::from_state(state),
    }
}

fn cancel_message(stage: UpdateStage) -> Option<&'static str> {
    match stage {
        UpdateStage::Checking => Some("Проверка обновления отменена."),
        UpdateStage::Downloading => Some("Загрузка обновления отменена."),
        _ => None,
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
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<AppUpdateDto, AppError> {
    let runtime = state
        .app_update
        .clone()
        .ok_or_else(|| AppError::new("update", "Обновление программы недоступно."))?;
    let snapshot = tauri::async_runtime::spawn_blocking(move || check_update(&runtime))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?;
    let _ = app.emit(STATE_EVENT, snapshot.clone());
    Ok(snapshot)
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
        let app_for_progress = app.clone();
        let mut progress = |value: f64| {
            let _ =
                app_for_progress.emit("app-update://event", AppUpdateProgress { progress: value });
        };
        run_download(&runtime, &mut progress)
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))
}

/// Скачивание и подготовка обновления. Блокировку состояния не держим на время сети:
/// иначе «Отменить» и чтение состояния ждали бы конца загрузки.
fn run_download(runtime: &Mutex<AppUpdateRuntime>, progress: &mut dyn FnMut(f64)) -> AppUpdateDto {
    let (manifest, downloader, update_dir, cancel, attempt) = {
        let mut guard = lock_runtime(runtime);
        if matches!(
            guard.state.stage,
            UpdateStage::Downloading | UpdateStage::Checking
        ) {
            return dto(guard.state.clone());
        }
        let Some(manifest) = guard.state.manifest.clone() else {
            guard.state = UpdateState::failed("Сначала проверьте обновление.");
            return dto(guard.state.clone());
        };
        // Новая попытка получает свой токен отмены: старый поток не «оживёт»
        // от reset() и не станет писать в тот же файл.
        guard.attempt = guard.attempt.wrapping_add(1);
        let attempt = guard.attempt;
        guard.cancel = bmm_app_release::CancelToken::new();
        guard.state.stage = UpdateStage::Downloading;
        guard.state.progress = 0.0;
        guard.state.error = None;
        (
            manifest,
            guard.downloader.clone(),
            guard.update_dir.clone(),
            guard.cancel.clone(),
            attempt,
        )
    };

    let downloaded = downloader.download_into_named(
        &manifest,
        &update_dir,
        &format!("update-{attempt}.zip"),
        progress,
        &cancel,
    );
    if cancel.is_cancelled() {
        return dto(lock_runtime(runtime).state.clone());
    }
    let prepared = match downloaded {
        Ok(downloaded) => match std::env::current_exe() {
            Ok(current) => prepare_update(
                &downloaded.path,
                &update_dir,
                &manifest,
                &current_version(),
                &current,
                &|_| None,
                &PrepareOptions::default(),
            )
            .map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        },
        Err(error) => Err(error.to_string()),
    };

    let mut guard = lock_runtime(runtime);
    // Отмену могли нажать, пока шла подготовка файлов. Её состояние важнее результата.
    // Номер попытки отсекает старый поток, который проснулся после новой загрузки.
    if cancel.is_cancelled() || guard.attempt != attempt {
        return dto(guard.state.clone());
    }
    match prepared {
        Ok(prepared) => {
            guard.prepared = Some(prepared);
            guard.state.stage = UpdateStage::ReadyToRestart;
            guard.state.progress = 1.0;
            guard.state.error = None;
        }
        Err(error) => {
            guard.prepared = None;
            guard.state.stage = UpdateStage::Failed;
            guard.state.error = Some(error);
        }
    }
    dto(guard.state.clone())
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
    let Some(message) = cancel_message(guard.state.stage) else {
        return Ok(dto(guard.state.clone()));
    };
    guard.cancel.cancel();
    guard.prepared = None;
    guard.state.stage = UpdateStage::Failed;
    guard.state.error = Some(message.to_string());
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
    let mut guard = lock_runtime(&runtime);
    let prepared = guard
        .prepared
        .take()
        .ok_or_else(|| AppError::new("update", "Обновление ещё не скачано."))?;
    let mut command = std::process::Command::new(&prepared.new_exe);
    command
        .arg("--install-update")
        .arg(std::process::id().to_string())
        .arg("--target")
        .arg(&prepared.current_exe)
        .arg("--backup")
        .arg(&prepared.backup);
    if let Err(error) = command.spawn() {
        // Установщик не запустился — оставляем обновление готовым к повтору.
        guard.prepared = Some(prepared);
        return Err(AppError::new("update", error.to_string()));
    }
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

/// Прежняя версия сообщает, почему новая не записалась: `--update-failed <текст>`.
pub fn update_failed_message() -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    flag_value(&args, "--update-failed")
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
    use std::io::{self, Read};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread;
    use std::time::{Duration, Instant};

    use bmm_app_release::{
        CancelToken, HttpResponse, HttpTransport, TransportError, UpdateDownloader, UpdateManifest,
    };

    use super::*;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| arg.to_string()).collect()
    }

    fn manifest(size: u64) -> UpdateManifest {
        UpdateManifest {
            version: AppVersion::parse("9.9.9").unwrap(),
            url: "https://github.com/x/y/releases/download/v9.9.9/app.exe"
                .parse()
                .unwrap(),
            size,
            sha256: "a".repeat(64),
            notes: None,
            published: None,
        }
    }

    struct SlowTransport {
        calls: Arc<AtomicUsize>,
        stop: Arc<AtomicBool>,
    }

    struct SlowBody {
        remaining: u64,
        stop: Arc<AtomicBool>,
    }

    impl Read for SlowBody {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if self.remaining == 0 || self.stop.load(Ordering::SeqCst) {
                return Ok(0);
            }
            thread::sleep(Duration::from_millis(5));
            let size = (buffer.len() as u64).min(self.remaining).min(4096) as usize;
            buffer[..size].fill(b'x');
            self.remaining -= size as u64;
            Ok(size)
        }
    }

    impl HttpTransport for SlowTransport {
        fn send(&self, _url: &url::Url) -> Result<HttpResponse, TransportError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(HttpResponse {
                status: 200,
                content_length: Some(4 * 1024 * 1024),
                location: None,
                body: Box::new(SlowBody {
                    remaining: 4 * 1024 * 1024,
                    stop: self.stop.clone(),
                }),
            })
        }
    }

    /// Запрос «завис», пока тест не разрешит ответ. Нужен, чтобы проверить,
    /// что состояние обновления и отмена доступны во время сети.
    struct BlockingTransport {
        started: std::sync::mpsc::Sender<()>,
        release: Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl HttpTransport for BlockingTransport {
        fn send(&self, _url: &url::Url) -> Result<HttpResponse, TransportError> {
            let _ = self.started.send(());
            let _ = self.release.lock().unwrap().recv();
            let body = format!(
                r#"{{"version":"9.9.9","url":"https://github.com/x/y/releases/download/v9.9.9/a.exe","size":1000000,"sha256":"{}"}}"#,
                "a".repeat(64)
            );
            Ok(HttpResponse {
                status: 200,
                content_length: Some(body.len() as u64),
                location: None,
                body: Box::new(std::io::Cursor::new(body.into_bytes())),
            })
        }
    }

    fn runtime_with(
        temp: &tempfile::TempDir,
        downloader: UpdateDownloader,
        state: UpdateState,
    ) -> Arc<Mutex<AppUpdateRuntime>> {
        Arc::new(Mutex::new(AppUpdateRuntime {
            downloader,
            state,
            prepared: None,
            cancel: CancelToken::new(),
            attempt: 0,
            update_dir: temp.path().join("update"),
            skipped_path: temp.path().join("update-skip.txt"),
            manifest_url: "https://github.com/x/y".parse().unwrap(),
        }))
    }

    fn update_runtime(
        temp: &tempfile::TempDir,
        transport: Arc<SlowTransport>,
    ) -> Arc<Mutex<AppUpdateRuntime>> {
        Arc::new(Mutex::new(AppUpdateRuntime {
            downloader: UpdateDownloader::new(transport),
            state: UpdateState {
                stage: UpdateStage::Available,
                manifest: Some(manifest(4 * 1024 * 1024)),
                progress: 0.0,
                error: None,
                checked_at: None,
            },
            prepared: None,
            cancel: CancelToken::new(),
            attempt: 0,
            update_dir: temp.path().join("update"),
            skipped_path: temp.path().join("update-skip.txt"),
            manifest_url: "https://github.com/x/y".parse().unwrap(),
        }))
    }

    #[test]
    fn cancelling_a_download_does_not_wait_for_the_network_lock() {
        let temp = tempfile::tempdir().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = update_runtime(
            &temp,
            Arc::new(SlowTransport {
                calls: calls.clone(),
                stop: stop.clone(),
            }),
        );
        let worker = {
            let runtime = runtime.clone();
            thread::spawn(move || run_download(&runtime, &mut |_| {}))
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if calls.load(Ordering::SeqCst) > 0 {
                assert_eq!(
                    lock_runtime(&runtime).state.stage,
                    UpdateStage::Downloading,
                    "состояние доступно во время загрузки"
                );
                break;
            }
            assert!(Instant::now() < deadline, "загрузка не началась");
            thread::sleep(Duration::from_millis(5));
        }

        // Так же, как команда «Отменить»: состояние и флаг доступны во время загрузки.
        {
            let mut guard = lock_runtime(&runtime);
            guard.cancel.cancel();
            guard.state.stage = UpdateStage::Failed;
            guard.state.error = Some("Загрузка обновления отменена.".to_string());
        }
        stop.store(true, Ordering::SeqCst);

        let result = worker.join().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(result.state.stage, "failed");
        assert_eq!(
            result.state.error.as_deref(),
            Some("Загрузка обновления отменена.")
        );
        assert!(result.state.manifest.is_some(), "описание не теряется");
    }

    #[test]
    fn checking_does_not_reset_a_download_in_progress() {
        let temp = tempfile::tempdir().unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let runtime = update_runtime(
            &temp,
            Arc::new(SlowTransport {
                calls: calls.clone(),
                stop: Arc::new(AtomicBool::new(true)),
            }),
        );
        {
            let mut guard = lock_runtime(&runtime);
            guard.state.stage = UpdateStage::Downloading;
            guard.cancel.cancel();
        }
        let result = check_update(&runtime);
        assert_eq!(result.state.stage, "downloading");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "сеть не трогали");
        assert!(
            lock_runtime(&runtime).cancel.is_cancelled(),
            "чужую отмену не сбросили"
        );
    }

    #[test]
    fn cancel_text_matches_activity() {
        assert_eq!(
            cancel_message(UpdateStage::Checking),
            Some("Проверка обновления отменена.")
        );
        assert_eq!(
            cancel_message(UpdateStage::Downloading),
            Some("Загрузка обновления отменена.")
        );
        assert_eq!(cancel_message(UpdateStage::Available), None);
    }

    #[test]
    fn checking_releases_state_lock_and_keeps_cancelled_state() {
        let temp = tempfile::tempdir().unwrap();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let runtime = runtime_with(
            &temp,
            UpdateDownloader::new(Arc::new(BlockingTransport {
                started: started_tx,
                release: Mutex::new(release_rx),
            })),
            UpdateState::idle(),
        );
        let worker = {
            let runtime = runtime.clone();
            thread::spawn(move || check_update(&runtime))
        };
        started_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("проверка не дошла до сети");

        // Раньше check_update держал замок до конца сети: это ожидание зависало.
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut guard = loop {
            match runtime.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::WouldBlock) if Instant::now() < deadline => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(std::sync::TryLockError::WouldBlock) => {
                    let _ = release_tx.send(());
                    let _ = worker.join();
                    panic!("проверка держит замок во время сетевого запроса");
                }
                Err(std::sync::TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
            }
        };
        assert_eq!(
            guard.state.stage,
            UpdateStage::Checking,
            "во время сети видна стадия проверки"
        );

        // Так же, как команда «Отменить»: состояние и флаг доступны без ожидания.
        guard.cancel.cancel();
        guard.state.stage = UpdateStage::Failed;
        guard.state.error = Some("Проверка обновления отменена.".to_string());
        drop(guard);
        release_tx.send(()).unwrap();

        let result = worker.join().unwrap();
        assert_eq!(result.state.stage, "failed");
        assert_eq!(
            result.state.error.as_deref(),
            Some("Проверка обновления отменена."),
            "результат сети не затирает отмену"
        );
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
        let manifest = &value["state"]["manifest"];
        // Версия — строкой «9.9.9», а не разобранным числом: окно её только печатает.
        assert_eq!(manifest["version"], "9.9.9");
        assert_eq!(value["state"]["stage"], "available");

        let stages = crate::test_support::ts_union("unions.ts", "AppUpdateStage");
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

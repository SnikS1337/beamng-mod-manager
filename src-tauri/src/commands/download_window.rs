//! Окно загрузки обновления: слева страница мода/файлообменника,
//! справа — панель с этапами. Оба вида — child webview одного нативного окна.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use tauri::webview::{DownloadEvent, NewWindowResponse};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl,
};
use ts_rs::TS;
use url::Url;

use crate::error::AppError;

pub(crate) const WINDOW_LABEL: &str = "download";
pub(crate) const PANEL_LABEL: &str = "download-panel";
const SITE_LABEL: &str = "download-site";
const SITE_X: f64 = 24.0;
const SITE_Y: f64 = 116.0;
/// Слева и справа от страницы: отступ, правая панель и ещё один отступ.
const SITE_SIDE_SPACE: f64 = 448.0;
/// На узком окне панель уже, иначе странице не остаётся места.
const SITE_SIDE_SPACE_NARROW: f64 = 360.0;
/// Сверху (шапка и адрес) и снизу (подпись) от страницы.
const SITE_VERTICAL_SPACE: f64 = 176.0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub mod_name: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub page_url: String,
    pub preview_url: Option<String>,
    #[serde(default)]
    #[ts(type = "ModRole | null")]
    pub role: Option<String>,
    /// Какой мод обновляем. Скачался тот же файл, что уже стоит, — панель предложит
    /// больше не показывать эту версию для этого мода.
    #[serde(default)]
    #[ts(optional)]
    pub mod_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadPanelEvent {
    step: u8,
    title: String,
    detail: String,
    error: Option<String>,
}

static CONTEXT: Mutex<Option<DownloadRequest>> = Mutex::new(None);
/// Обновление пачкой: страницы модов по очереди, установка — одна, в конце.
static BATCH: Mutex<Vec<DownloadRequest>> = Mutex::new(Vec::new());
/// Загрузки окна: их бывает несколько сразу — человек может открыть следующий мод,
/// пока качается прошлый. У каждой свой мод, свой ход и свой итог.
static JOBS: LazyLock<Mutex<Jobs>> = LazyLock::new(|| Mutex::new(Jobs::default()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum DownloadJobState {
    Downloading,
    Done,
    Failed,
}

/// Загрузка окна: файл, мод, ради которого она началась, и чем кончилась.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJob {
    pub path: String,
    pub file_name: String,
    pub request: Option<DownloadRequest>,
    pub state: DownloadJobState,
    pub error: Option<String>,
    pub id: u64,
}

/// Сколько закончившихся загрузок помнить: панели хватает, память не растёт.
const JOBS_KEEP: usize = 64;
/// Id не сбрасывается вместе с окном: старый cleaner не должен принять новую
/// загрузку с тем же путём за свою.
static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);
/// Поколение окна: события разрушенного WebView не должны менять новую сессию.
static SESSION_ID: AtomicU64 = AtomicU64::new(1);
/// Смена поколения и очистка сессии атомарны вместе. Открытия держат OPEN_GATE,
/// затем берут этот gate; clear_session не берёт OPEN_GATE, поэтому обратного
/// порядка блокировок нет.
static SESSION_GATE: Mutex<()> = Mutex::new(());
static REGISTRATION_GATE: Mutex<()> = Mutex::new(());
static NEXT_NAV_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVE_NAV_ID: AtomicU64 = AtomicU64::new(0);
static CURRENT_SITE: Mutex<Option<String>> = Mutex::new(None);
static DOWNLOAD_SESSIONS: LazyLock<Mutex<HashMap<String, DownloadOwnership>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static SITE_DOWNLOADS: LazyLock<Mutex<HashMap<u64, Vec<SiteDownload>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DOWNLOAD_REGISTRATIONS: LazyLock<Mutex<HashMap<u64, VecDeque<DownloadRegistration>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static WINDOW_DESTROYED: Mutex<Option<(u64, Arc<tokio::sync::Notify>)>> = Mutex::new(None);
static RETIRING: Mutex<Option<Arc<tokio::sync::Notify>>> = Mutex::new(None);
static RETIRING_SESSION_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
struct DownloadOwnership {
    session_id: u64,
    nav_id: u64,
    job_id: u64,
    terminal_handler: bool,
}

#[derive(Debug, Clone)]
struct SiteDownload {
    path: PathBuf,
    url: Url,
}

#[derive(Debug, Clone)]
pub(crate) enum DownloadRegistration {
    Accepted(PathBuf, u64),
    Rejected,
}

pub(crate) fn path_key(path: &Path) -> String {
    let text = path.display().to_string();
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::UI::WindowsAndMessaging::CharLowerW;

        let normalized = text.replace('\\', "/");
        let mut wide: Vec<u16> = Path::new(&normalized).as_os_str().encode_wide().collect();
        wide.push(0);
        // SAFETY: буфер содержит завершающий NUL, CharLowerW возвращает тот же буфер.
        let lowered = unsafe { CharLowerW(wide.as_mut_ptr()) };
        if lowered.is_null() {
            return normalized.to_ascii_lowercase();
        }
        let len = wide
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(wide.len());
        String::from_utf16(&wide[..len]).unwrap_or_else(|_| normalized.to_ascii_lowercase())
    }
    #[cfg(not(windows))]
    {
        text
    }
}

#[derive(Debug, Default)]
pub(crate) struct Jobs {
    items: Vec<DownloadJob>,
}

impl Jobs {
    /// Окно начало качать файл для этого мода. Тот же путь — новая загрузка.
    fn start(&mut self, path: &Path, request: Option<DownloadRequest>) -> u64 {
        let key = path_key(path);
        self.items
            .retain(|job| path_key(Path::new(&job.path)) != key);
        while self.items.len() >= JOBS_KEEP {
            match self
                .items
                .iter()
                .position(|job| job.state != DownloadJobState::Downloading)
            {
                Some(index) => {
                    self.items.remove(index);
                }
                None => break,
            }
        }
        let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);
        self.items.push(DownloadJob {
            id,
            file_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: path.display().to_string(),
            request,
            state: DownloadJobState::Downloading,
            error: None,
        });
        id
    }

    /// Загрузка кончилась. `None` — файл не наш или его итог уже записан: второй
    /// раз один архив панели не отдаём.
    fn finish(&mut self, path: &Path, outcome: Result<(), String>) -> Option<DownloadJob> {
        let key = path_key(path);
        let job = self.items.iter_mut().find(|job| {
            path_key(Path::new(&job.path)) == key && job.state == DownloadJobState::Downloading
        })?;
        match outcome {
            Ok(()) => job.state = DownloadJobState::Done,
            Err(error) => {
                job.state = DownloadJobState::Failed;
                job.error = Some(error);
            }
        }
        Some(job.clone())
    }

    /// Уточнить причину обрыва — только у той самой загрузки, а не у новой под тем же
    /// именем, начатой за это время.
    fn explain(&mut self, id: u64, error: String) -> Option<DownloadJob> {
        let job = self
            .items
            .iter_mut()
            .find(|job| job.id == id && job.state == DownloadJobState::Failed)?;
        job.error = Some(error);
        Some(job.clone())
    }

    /// Файлы, которые ещё качаются.
    fn downloading(&self) -> Vec<PathBuf> {
        self.items
            .iter()
            .filter(|job| job.state == DownloadJobState::Downloading)
            .map(|job| PathBuf::from(&job.path))
            .collect()
    }

    fn downloading_with_ids(&self) -> Vec<(PathBuf, u64)> {
        self.items
            .iter()
            .filter(|job| job.state == DownloadJobState::Downloading)
            .map(|job| (PathBuf::from(&job.path), job.id))
            .collect()
    }

    fn ready(&self) -> Vec<DownloadJob> {
        self.items
            .iter()
            .filter(|job| job.state == DownloadJobState::Done)
            .cloned()
            .collect()
    }

    fn snapshot(&self) -> Vec<DownloadJob> {
        self.items.clone()
    }
}

fn jobs() -> std::sync::MutexGuard<'static, Jobs> {
    JOBS.lock().unwrap_or_else(|error| error.into_inner())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InactiveRemoval {
    Removed,
    Replaced,
    Failed,
}

fn remove_inactive_download(path: &Path, expected_id: u64, jobs: &Jobs) -> InactiveRemoval {
    let key = path_key(path);
    if jobs
        .items
        .iter()
        .any(|job| path_key(Path::new(&job.path)) == key && job.id != expected_id)
    {
        return InactiveRemoval::Replaced;
    }
    match std::fs::remove_file(path) {
        Ok(()) => InactiveRemoval::Removed,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => InactiveRemoval::Removed,
        Err(_) => InactiveRemoval::Failed,
    }
}

pub(crate) fn cleanup_failed_download(path: &Path) {
    let key = path_key(path);
    let Some(id) = jobs()
        .snapshot()
        .into_iter()
        .rev()
        .find(|job| path_key(Path::new(&job.path)) == key && job.state == DownloadJobState::Failed)
        .map(|job| job.id)
    else {
        return;
    };
    let _ = remove_inactive_download(path, id, &jobs());
}

/// Список загрузок — панели окна и главному окну: там видно, что что-то качается.
fn emit_jobs(app: &AppHandle) {
    let list = jobs().snapshot();
    let _ = app.emit_to(PANEL_LABEL, "download://jobs", list.clone());
    let _ = app.emit_to(MAIN_LABEL, "download://jobs", list);
}

/// Ход загрузки — туда же: в окно и в главное окно. Байты по ходу отдаёт только
/// WebView2 на Windows.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn emit_progress(
    app: &AppHandle,
    progress: crate::commands::download_progress::DownloadProgress,
    session_id: u64,
    nav_id: u64,
    job_id: Option<u64>,
) {
    if !progress_belongs_to_operation(Path::new(&progress.path), session_id, nav_id, job_id) {
        return;
    }
    let _ = app.emit_to(PANEL_LABEL, "download://progress", progress.clone());
    let _ = app.emit_to(MAIN_LABEL, "download://progress", progress);
}

const MAIN_LABEL: &str = "main";

fn run_on_main_thread<T, F>(app: &AppHandle, task: F) -> Result<T, AppError>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let (sender, receiver) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let _ = sender.send(task());
    })
    .map_err(|error| AppError::new("main", error.to_string()))?;
    receiver
        .recv()
        .map_err(|_| AppError::new("main", "Не удалось дождаться главного потока."))
}

fn current_request() -> Option<DownloadRequest> {
    CONTEXT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

/// Загрузка начата на той странице, что сейчас открыта в окне. Сообщения об ошибке
/// прошлого мода не должны лечь в шаги текущего.
fn for_current_mod(started_for: Option<&str>, current: Option<&str>) -> bool {
    started_for == current
}

fn page_of(request: Option<&DownloadRequest>) -> Option<&str> {
    request.map(|request| request.page_url.as_str())
}

#[tauri::command]
pub fn download_panel_state() -> Option<DownloadRequest> {
    current_request()
}

/// Докачанные архивы для панели: на случай, если события пришли до её загрузки.
#[tauri::command]
pub fn download_ready() -> Vec<DownloadJob> {
    jobs().ready()
}

/// Все загрузки окна: панели и главному окну после перезагрузки страницы.
#[tauri::command]
pub fn download_jobs() -> Vec<DownloadJob> {
    jobs().snapshot()
}

struct SessionGuard {
    id: u64,
    window_exists: bool,
    destroyed: Arc<tokio::sync::Notify>,
    _gate: std::sync::MutexGuard<'static, ()>,
}

impl SessionGuard {
    fn renew_for_new_window(&mut self) {
        self.id = SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1;
        self.window_exists = false;
        self.destroyed = Arc::new(tokio::sync::Notify::new());
        *WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner()) =
            Some((self.id, Arc::clone(&self.destroyed)));
    }
}

fn enter_session(request: &DownloadRequest, window_exists: bool) -> SessionGuard {
    let gate = SESSION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    enter_session_locked(request, window_exists, gate)
}

fn enter_session_locked(
    request: &DownloadRequest,
    window_exists: bool,
    gate: std::sync::MutexGuard<'static, ()>,
) -> SessionGuard {
    let (id, destroyed) = if window_exists {
        let id = SESSION_ID.load(Ordering::Relaxed);
        let stored = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .filter(|(stored, _)| *stored == id)
            .map(|(_, destroyed)| Arc::clone(destroyed));
        let destroyed = stored.unwrap_or_else(|| {
            let destroyed = Arc::new(tokio::sync::Notify::new());
            *WINDOW_DESTROYED
                .lock()
                .unwrap_or_else(|error| error.into_inner()) = Some((id, Arc::clone(&destroyed)));
            destroyed
        });
        (id, destroyed)
    } else {
        let id = SESSION_ID.fetch_add(1, Ordering::Relaxed) + 1;
        let destroyed = Arc::new(tokio::sync::Notify::new());
        *WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some((id, Arc::clone(&destroyed)));
        (id, destroyed)
    };
    if !window_exists {
        *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = Some(request.clone());
    }
    SessionGuard {
        id,
        window_exists,
        destroyed,
        _gate: gate,
    }
}

/// Сессия окна закончилась: панель, контекст и загрузки больше не нужны.
/// Незаконченная загрузка после этого снова видна сторожу «Загрузок».
fn clear_session_locked(
    expected: u64,
    forget_downloads: bool,
    _gate: std::sync::MutexGuard<'static, ()>,
) -> bool {
    if SESSION_ID.load(Ordering::Relaxed) != expected {
        return false;
    }
    SESSION_ID.fetch_add(1, Ordering::Relaxed);
    *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = None;
    *jobs() = Jobs::default();
    DOWNLOAD_SESSIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    SITE_DOWNLOADS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    DOWNLOAD_REGISTRATIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    BATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    ACTIVE_NAV_ID.store(0, Ordering::Relaxed);
    *CURRENT_SITE
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    if forget_downloads {
        crate::commands::downloads::forget_caught();
    }
    true
}

#[cfg(test)]
fn clear_session_for_test(expected: u64) -> bool {
    // Общий CAUGHT трогают параллельные downloads-тесты.
    let gate = SESSION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    clear_session_locked(expected, false, gate)
}

/// Оборвать загрузки, которые ещё идут, и убрать недокачанные файлы. Человек сам
/// решил закрыть окно или программу — иначе файл качался бы дальше без хозяина
/// и пропадал при выходе.
fn cancel_download_list(
    app: &AppHandle,
    unfinished: Vec<(PathBuf, u64)>,
    site_label: Option<String>,
) {
    if unfinished.is_empty() {
        return;
    }
    if let Some(site) = site_label.and_then(|label| app.get_webview(&label)) {
        crate::commands::download_progress::cancel_all(&site);
    }
    // WebView2 обычно сам удаляет оборванный файл; если он ещё держит его, пробуем
    // ещё несколько раз. Новую загрузку с тем же путём не удаляем: её id другой.
    std::thread::spawn(move || {
        let mut replaced = HashSet::new();
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(700));
            let mut left = false;
            for (path, id) in &unfinished {
                if replaced.contains(path) {
                    continue;
                }
                match remove_inactive_download(path, *id, &jobs()) {
                    InactiveRemoval::Removed => {}
                    InactiveRemoval::Replaced => {
                        replaced.insert(path.clone());
                    }
                    InactiveRemoval::Failed => left = true,
                }
            }
            if !left {
                break;
            }
        }
    });
}

pub(crate) fn cancel_downloads(app: &AppHandle) {
    let site_label = CURRENT_SITE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    cancel_download_list(app, jobs().downloading_with_ids(), site_label);
}

fn current_window_destroyed() -> Option<Arc<tokio::sync::Notify>> {
    WINDOW_DESTROYED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .as_ref()
        .map(|(_, destroyed)| Arc::clone(destroyed))
}

fn forget_window_destroyed(expected: u64) {
    let mut destroyed = WINDOW_DESTROYED
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if destroyed
        .as_ref()
        .is_some_and(|(stored, _)| *stored == expected)
    {
        *destroyed = None;
    }
}

fn clear_retiring(destroyed: &Arc<tokio::sync::Notify>) {
    let mut retiring = RETIRING.lock().unwrap_or_else(|error| error.into_inner());
    if retiring
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, destroyed))
    {
        *retiring = None;
        RETIRING_SESSION_ID.store(0, Ordering::Relaxed);
    }
}

fn mark_retiring(session: &SessionGuard) {
    *RETIRING.lock().unwrap_or_else(|error| error.into_inner()) =
        Some(Arc::clone(&session.destroyed));
    RETIRING_SESSION_ID.store(session.id, Ordering::Relaxed);
}

fn destroy_retiring_window(app: &AppHandle, session: &SessionGuard) {
    mark_retiring(session);
    if let Some(window) = app.get_window(WINDOW_LABEL) {
        let _ = window.destroy();
    }
}

async fn wait_for_window_destruction(
    app: &AppHandle,
    destroyed: &Arc<tokio::sync::Notify>,
) -> Result<(), AppError> {
    loop {
        tokio::time::timeout(std::time::Duration::from_secs(5), destroyed.notified())
            .await
            .map_err(|_| AppError::new("window", "Окно загрузки ещё закрывается."))?;
        if app.get_window(WINDOW_LABEL).is_none() {
            clear_retiring(destroyed);
            return Ok(());
        }
    }
}

async fn wait_for_retiring_window(app: &AppHandle) -> Result<(), AppError> {
    let destroyed = RETIRING
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let Some(destroyed) = destroyed else {
        return Ok(());
    };
    if RETIRING_SESSION_ID.load(Ordering::Relaxed) != SESSION_ID.load(Ordering::Relaxed) {
        clear_retiring(&destroyed);
        return Ok(());
    }
    if let Some(window) = app.get_window(WINDOW_LABEL) {
        let _ = window.destroy();
    }
    wait_for_window_destruction(app, &destroyed).await
}

#[tauri::command]
pub async fn download_close(app: AppHandle) -> Result<(), AppError> {
    let _gate = OPEN_GATE.lock().await;
    let window = app.get_window(WINDOW_LABEL);
    let site_label = CURRENT_SITE
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone();
    let destroyed =
        current_window_destroyed().unwrap_or_else(|| Arc::new(tokio::sync::Notify::new()));
    if window.is_some() {
        *RETIRING.lock().unwrap_or_else(|error| error.into_inner()) = Some(Arc::clone(&destroyed));
        RETIRING_SESSION_ID.store(SESSION_ID.load(Ordering::Relaxed), Ordering::Relaxed);
    }
    let unfinished = tauri::async_runtime::spawn_blocking(move || {
        let session = SESSION_GATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let _registration = REGISTRATION_GATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let unfinished = jobs().downloading_with_ids();
        clear_session_locked(SESSION_ID.load(Ordering::Relaxed), true, session);
        unfinished
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))?;
    cancel_download_list(&app, unfinished, site_label);
    emit_jobs(&app);
    if let Some(window) = window {
        window
            .destroy()
            .map_err(|error| AppError::new("window", error.to_string()))?;
        wait_for_window_destruction(&app, &destroyed).await?;
    }
    Ok(())
}

/// Главное окно: «прервать загрузки и выйти».
#[tauri::command]
pub fn download_cancel_all(app: AppHandle) {
    cancel_downloads(&app);
}

/// Главное окно: показать окно загрузки.
#[tauri::command]
pub fn download_focus(app: AppHandle) {
    if let Some(window) = app.get_window(WINDOW_LABEL) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn batch_snapshot() -> Vec<DownloadRequest> {
    BATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

/// Очередь пачки для панели (пустая — обычное обновление одного мода).
fn blocking_batch_snapshot() -> Vec<DownloadRequest> {
    let _session = SESSION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    batch_snapshot()
}

#[tauri::command]
pub async fn download_batch() -> Vec<DownloadRequest> {
    tauri::async_runtime::spawn_blocking(blocking_batch_snapshot)
        .await
        .unwrap_or_default()
}

/// Окно ведёт пачку: новый мод встаёт в её конец, а не уводит окно со страницы,
/// которую человек сейчас качает. `false` — пачки нет, мод открывается как обычно.
fn add_to_batch(batch: &mut Vec<DownloadRequest>, request: &DownloadRequest) -> bool {
    if batch.is_empty() {
        return false;
    }
    if !batch
        .iter()
        .any(|queued| queued.page_url == request.page_url)
    {
        batch.push(request.clone());
    }
    true
}

#[tauri::command]
pub async fn download_open(app: AppHandle, request: DownloadRequest) -> Result<(), AppError> {
    let _gate = OPEN_GATE.lock().await;
    wait_for_retiring_window(&app).await?;
    let url = Url::parse(&request.page_url)
        .map_err(|_| AppError::new("url", "Некорректный адрес страницы мода."))?;
    if !page_url_allowed(&url) {
        return Err(AppError::new(
            "url",
            "Открывать можно только внешние страницы http и https.",
        ));
    }
    let window_exists = app.get_window(WINDOW_LABEL).is_some();
    let gate = SESSION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let queued = {
        let mut batch = BATCH.lock().unwrap_or_else(|error| error.into_inner());
        add_to_batch(&mut batch, &request)
    };
    if queued && window_exists {
        let _ = app.emit_to(PANEL_LABEL, "download://batch-added", request);
        download_focus(app);
        return Ok(());
    }
    if queued {
        // Окна нет, а очередь осталась: сессия кончилась нечисто — начинаем заново.
        BATCH
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }
    let session = enter_session_locked(&request, window_exists, gate);
    // Загрузки прошлых модов идут дальше: окно только открывает новую страницу.
    open_request_locked(app, request, session)
}

/// «Обновить все»: окно откроет первую страницу, панель поведёт по остальным.
#[tauri::command]
pub async fn download_open_batch(
    app: AppHandle,
    requests: Vec<DownloadRequest>,
) -> Result<(), AppError> {
    let first = requests
        .first()
        .cloned()
        .ok_or_else(|| AppError::new("batch", "Нечего обновлять."))?;
    if let Some(bad) = requests.iter().find(|request| {
        Url::parse(&request.page_url)
            .ok()
            .is_none_or(|url| !page_url_allowed(&url))
    }) {
        return Err(AppError::new(
            "url",
            format!("Некорректный адрес страницы «{}».", bad.mod_name),
        ));
    }
    let _gate = OPEN_GATE.lock().await;
    wait_for_retiring_window(&app).await?;
    let session = enter_session(&first, app.get_window(WINDOW_LABEL).is_some());
    *BATCH.lock().unwrap_or_else(|error| error.into_inner()) = requests;
    let batch = batch_snapshot();
    let _ = app.emit_to(PANEL_LABEL, "download://batch", batch);
    open_request_locked(app, first, session)
}

/// Следующая (или любая) страница пачки — без сброса очереди.
#[tauri::command]
pub async fn download_batch_goto(app: AppHandle, index: usize) -> Result<(), AppError> {
    let _gate = OPEN_GATE.lock().await;
    wait_for_retiring_window(&app).await?;
    let gate = SESSION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let request = BATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(index)
        .cloned()
        .ok_or_else(|| AppError::new("batch", "Такого мода в очереди нет."))?;
    let session = enter_session_locked(&request, app.get_window(WINDOW_LABEL).is_some(), gate);
    open_request_locked(app, request, session)
}

/// Открытия окна по очереди. Два «Обновить» подряд шли одновременно: второе
/// записывало свой мод в контекст, пока первое ещё создавало окно со своей
/// страницей, — архив первого мода приписывался второму, а само второе открытие
/// падало на «окно уже есть».
static OPEN_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn open_request_locked(
    app: AppHandle,
    request: DownloadRequest,
    mut session: SessionGuard,
) -> Result<(), AppError> {
    let url = Url::parse(&request.page_url)
        .map_err(|_| AppError::new("url", "Некорректный адрес страницы мода."))?;
    if !page_url_allowed(&url) {
        return Err(AppError::new(
            "url",
            "Открывать можно только внешние страницы http и https.",
        ));
    }
    let settings = app
        .state::<std::sync::Arc<crate::state::AppState>>()
        .settings
        .load()
        .settings;
    let app_root = app
        .state::<std::sync::Arc<crate::state::AppState>>()
        .paths
        .root()
        .to_path_buf();
    let (mods_dir, _) = crate::commands::library::resolve_folder(&settings);
    let downloads =
        crate::commands::downloads::download_folder(&settings, &app_root, mods_dir.as_deref())
            .ok_or_else(|| AppError::new("downloads", "Не нашли папку, куда сохранить архив."))?;
    crate::commands::downloads::expect_folder(&downloads);

    if session.window_exists && app.get_window(WINDOW_LABEL).is_none() {
        session.renew_for_new_window();
        *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = Some(request.clone());
    }
    let session_id = session.id;
    let nav_id = NEXT_NAV_ID.fetch_add(1, Ordering::Relaxed);
    let site_label = format!("{SITE_LABEL}-{nav_id}");
    let (window_width, window_height) = window_size(&app);
    if let Some(window) = app.get_window(WINDOW_LABEL) {
        let previous_nav_id = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let previous_site = CURRENT_SITE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let main_app = app.clone();
        let main_window = window.clone();
        let site_request = request.clone();
        let main_site_label = site_label.clone();
        let site = run_on_main_thread(&app, move || {
            create_site(
                &main_app,
                &main_window,
                SiteSpec {
                    request: site_request,
                    url,
                    session_id,
                    nav_id,
                    label: main_site_label,
                    width: window_width,
                    height: window_height,
                },
            )
        })
        .and_then(|site| site)?;
        *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = Some(request.clone());
        if let Some(previous) = previous_site.filter(|label| label != &site_label) {
            fail_site_downloads(&app, previous_nav_id, session_id);
            if let Some(old_site) = app.get_webview(&previous) {
                let _ = old_site.close();
            }
        }
        let _ = window.show();
        let _ = window.set_focus();
        let _ = app.emit_to(PANEL_LABEL, "download://context", request);
        let _ = site;
        return Ok(());
    }

    let window = tauri::Window::builder(&app, WINDOW_LABEL)
        .title("Скачивание")
        .inner_size(window_width, window_height)
        .resizable(false)
        .decorations(false)
        // Пока панель грузится, окно не вспыхивает белым.
        .background_color(tauri::window::Color(14, 15, 17, 255))
        .visible(false)
        .center()
        .build()
        .map_err(|error| AppError::new("window", error.to_string()))?;
    // Окно могли закрыть не кнопкой панели (Alt+F4, выход из программы):
    // метки перехвата не должны пережить сессию окна.
    let close_app = app.clone();
    let destroyed_for_window = Arc::clone(&session.destroyed);
    window.on_window_event(move |event| match event {
        // Alt+F4 при идущей загрузке: сначала спросить в панели, иначе файл
        // качался бы дальше без окна и пропадал при выходе из программы.
        tauri::WindowEvent::CloseRequested { api, .. }
            if !jobs().downloading().is_empty() || !jobs().ready().is_empty() =>
        {
            api.prevent_close();
            let _ = close_app.emit_to(PANEL_LABEL, "download://close-asked", ());
        }
        tauri::WindowEvent::CloseRequested { .. } => {
            *RETIRING.lock().unwrap_or_else(|error| error.into_inner()) =
                Some(Arc::clone(&destroyed_for_window));
            RETIRING_SESSION_ID.store(session_id, Ordering::Relaxed);
        }
        tauri::WindowEvent::Destroyed => {
            let app = close_app.clone();
            let destroyed = Arc::clone(&destroyed_for_window);
            *RETIRING.lock().unwrap_or_else(|error| error.into_inner()) =
                Some(Arc::clone(&destroyed));
            RETIRING_SESSION_ID.store(session_id, Ordering::Relaxed);
            tauri::async_runtime::spawn_blocking(move || {
                let session = SESSION_GATE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let _registration = REGISTRATION_GATE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if clear_session_locked(session_id, true, session) {
                    emit_jobs(&app);
                }
                forget_window_destroyed(session_id);
                drop(_registration);
                clear_retiring(&destroyed);
                destroyed.notify_one();
            });
        }
        _ => {}
    });

    let panel = match window.add_child(
        // Панель всегда во всё окно: при масштабе 125 % размеры округлялись,
        // и снизу оставалась белая полоса окна.
        crate::with_browser_args!(WebviewBuilder::new(PANEL_LABEL, panel_url(&app))).auto_resize(),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(window_width, window_height),
    ) {
        Ok(panel) => panel,
        Err(error) => {
            // Окно уже зарегистрировано: без уборки повтор откроет пустую панель.
            destroy_retiring_window(&app, &session);
            drop(session);
            return Err(AppError::new("panel", error.to_string()));
        }
    };
    let main_app = app.clone();
    let main_window = window.clone();
    let site_result = run_on_main_thread(&app, move || {
        create_site(
            &main_app,
            &main_window,
            SiteSpec {
                request,
                url,
                session_id,
                nav_id,
                label: site_label,
                width: window_width,
                height: window_height,
            },
        )
    })
    .and_then(|site| site);
    let site = match site_result {
        Ok(site) => site,
        Err(error) => {
            destroy_retiring_window(&app, &session);
            drop(session);
            return Err(error);
        }
    };
    if let Err(error) = window.show() {
        destroy_retiring_window(&app, &session);
        drop(session);
        return Err(AppError::new("window", error.to_string()));
    }
    if let Err(error) = site.set_focus() {
        destroy_retiring_window(&app, &session);
        drop(session);
        return Err(AppError::new("site", error.to_string()));
    }
    let _ = panel;
    Ok(())
}

fn begin_download_registration(nav_id: u64) {
    let _registration = REGISTRATION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    DOWNLOAD_REGISTRATIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .entry(nav_id)
        .or_default()
        .push_back(DownloadRegistration::Rejected);
}

fn accept_download_registration(nav_id: u64, path: &Path, job_id: u64) {
    let _registration = REGISTRATION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut registrations = DOWNLOAD_REGISTRATIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let queue = registrations.entry(nav_id).or_default();
    if let Some(registration) = queue.back_mut() {
        *registration = DownloadRegistration::Accepted(path.to_path_buf(), job_id);
    } else {
        queue.push_back(DownloadRegistration::Accepted(path.to_path_buf(), job_id));
    }
}

pub(crate) fn take_download_registration(nav_id: u64) -> Option<DownloadRegistration> {
    let _registration = REGISTRATION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut registrations = DOWNLOAD_REGISTRATIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let queue = registrations.get_mut(&nav_id)?;
    let registration = queue.pop_front();
    if queue.is_empty() {
        registrations.remove(&nav_id);
    }
    registration
}

fn register_download(
    path: &Path,
    owner: DownloadRequest,
    url: &Url,
    session_id: u64,
    nav_id: u64,
) -> bool {
    if let Some(job_id) = register_download_inner(path, owner, url, session_id, nav_id, true) {
        accept_download_registration(nav_id, path, job_id);
        true
    } else {
        false
    }
}

fn register_download_inner(
    path: &Path,
    owner: DownloadRequest,
    url: &Url,
    session_id: u64,
    nav_id: u64,
    mark_caught: bool,
) -> Option<u64> {
    let _registration = REGISTRATION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if SESSION_ID.load(Ordering::Relaxed) != session_id
        || ACTIVE_NAV_ID.load(Ordering::Relaxed) != nav_id
    {
        return None;
    }
    let duplicate = SITE_DOWNLOADS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&nav_id)
        .is_some_and(|downloads| downloads.iter().any(|download| download.url == *url));
    if duplicate {
        return None;
    }
    if mark_caught {
        crate::commands::downloads::note_caught(path);
    }
    let job_id = jobs().start(path, Some(owner));
    let key = path_key(path);
    DOWNLOAD_SESSIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(
            key.clone(),
            DownloadOwnership {
                session_id,
                nav_id,
                job_id,
                terminal_handler: cfg!(windows),
            },
        );
    SITE_DOWNLOADS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .entry(nav_id)
        .or_default()
        .push(SiteDownload {
            path: path.to_path_buf(),
            url: url.clone(),
        });
    Some(job_id)
}

#[cfg(test)]
fn register_download_for_test(
    path: &Path,
    owner: DownloadRequest,
    session_id: u64,
    nav_id: u64,
) -> bool {
    let url = Url::parse("https://example.com/download.zip").unwrap();
    register_download_url_for_test(path, &url, owner, session_id, nav_id)
}

#[cfg(test)]
fn register_download_url_for_test(
    path: &Path,
    url: &Url,
    owner: DownloadRequest,
    session_id: u64,
    nav_id: u64,
) -> bool {
    register_download_inner(path, owner, url, session_id, nav_id, false).is_some()
}

struct ActiveSite {
    nav_id: u64,
    previous_nav_id: u64,
    previous_label: Option<String>,
    committed: bool,
}

impl ActiveSite {
    fn new(nav_id: u64, label: String) -> Self {
        let _registration = REGISTRATION_GATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let previous_nav_id = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let previous_label = CURRENT_SITE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        ACTIVE_NAV_ID.store(nav_id, Ordering::Relaxed);
        *CURRENT_SITE
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = Some(label.clone());
        Self {
            nav_id,
            previous_nav_id,
            previous_label,
            committed: false,
        }
    }

    fn commit(mut self) {
        self.committed = true;
    }

    fn rollback(&mut self) {
        if self.committed {
            return;
        }
        let _registration = REGISTRATION_GATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if ACTIVE_NAV_ID.load(Ordering::Relaxed) != self.nav_id {
            return;
        }
        ACTIVE_NAV_ID.store(self.previous_nav_id, Ordering::Relaxed);
        *CURRENT_SITE
            .lock()
            .unwrap_or_else(|error| error.into_inner()) = self.previous_label.clone();
    }
}

impl Drop for ActiveSite {
    fn drop(&mut self) {
        self.rollback();
    }
}

struct SiteSpec {
    request: DownloadRequest,
    url: Url,
    session_id: u64,
    nav_id: u64,
    label: String,
    width: f64,
    height: f64,
}

fn create_site(
    app: &AppHandle,
    window: &tauri::Window,
    spec: SiteSpec,
) -> Result<tauri::Webview, AppError> {
    let SiteSpec {
        request,
        url,
        session_id,
        nav_id,
        label,
        width: window_width,
        height: window_height,
    } = spec;
    let download_app = app.clone();
    let owner = request.clone();
    let site_label = label;
    let callback_site_label = site_label.clone();
    let active_site = ActiveSite::new(nav_id, site_label.clone());
    let site_builder = crate::with_browser_args!(WebviewBuilder::new(
        site_label.clone(),
        WebviewUrl::External(url)
    ))
    .initialization_script(site_helpers())
    .on_navigation(|url| url.scheme() == "about" || bmm_sources::net::redirect_allowed(url))
    .on_download(move |_webview, event| {
        match event {
            DownloadEvent::Requested { url, destination } => {
                if SESSION_ID.load(Ordering::Relaxed) != session_id
                    || ACTIVE_NAV_ID.load(Ordering::Relaxed) != nav_id
                {
                    return false;
                }
                begin_download_registration(nav_id);
                let name = download_name(&url, destination);
                if !accepts_download(&url, &name) {
                    emit_state(
                        &download_app,
                        1,
                        "Это не архив мода",
                        &format!("Сайт предложил скачать «{name}» — такой файл менеджер не берёт."),
                        Some("Нужна кнопка, которая скачивает zip, rar или 7z.".to_string()),
                    );
                    return false;
                }
                let state = download_app.state::<std::sync::Arc<crate::state::AppState>>();
                let settings = state.settings.load().settings;
                let app_root = state.paths.root().to_path_buf();
                let (mods_dir, _) = crate::commands::library::resolve_folder(&settings);
                let Some(downloads) = crate::commands::downloads::download_folder(
                    &settings,
                    &app_root,
                    mods_dir.as_deref(),
                ) else {
                    emit_state(
                        &download_app,
                        2,
                        "Не удалось начать загрузку",
                        "Не нашли папку, куда сохранить архив.",
                        Some("Проверьте доступ к папке загрузки.".to_string()),
                    );
                    return false;
                };
                crate::commands::downloads::expect_folder(&downloads);
                let busy = jobs().downloading();
                let path = unique_path(&downloads, &name, &busy);
                if std::fs::create_dir_all(&downloads).is_err() {
                    emit_state(
                        &download_app,
                        2,
                        "Не удалось начать загрузку",
                        "Не получилось создать папку загрузки.",
                        Some("Проверьте доступ к папке данных программы.".to_string()),
                    );
                    return false;
                }
                if !register_download(&path, owner.clone(), &url, session_id, nav_id) {
                    return false;
                }
                emit_jobs(&download_app);
                *destination = path;
                emit_state(
                    &download_app,
                    2,
                    "Файл скачивается",
                    "Дождитесь окончания загрузки — менеджер заберёт файл сам.",
                    None,
                );
            }
            DownloadEvent::Finished {
                url: _url,
                path,
                success: _success,
            } => {
                if let Some(path) = path.as_deref() {
                    if let Some(job_id) = tauri_finish_job(path, session_id, nav_id) {
                        if _success {
                            finished_ok(
                                &download_app,
                                Some(path),
                                session_id,
                                nav_id,
                                Some(job_id),
                            );
                        } else {
                            finished_failed(
                                &download_app,
                                path.to_path_buf(),
                                session_id,
                                nav_id,
                                Some(job_id),
                            );
                        }
                    }
                } else {
                    #[cfg(windows)]
                    if let Some(path) = site_download_path(nav_id, &_url, session_id)
                        && let Some(job_id) = tauri_finish_job(&path, session_id, nav_id)
                    {
                        if _success {
                            finished_ok(
                                &download_app,
                                Some(&path),
                                session_id,
                                nav_id,
                                Some(job_id),
                            );
                        } else {
                            finished_failed(&download_app, path, session_id, nav_id, Some(job_id));
                        }
                    }
                    #[cfg(not(windows))]
                    finished_without_path(&download_app, &_url, _success, session_id, nav_id);
                }
            }
            _ => {}
        }
        true
    })
    .on_new_window({
        let app = app.clone();
        move |url, _features| {
            if SESSION_ID.load(Ordering::Relaxed) != session_id
                || ACTIVE_NAV_ID.load(Ordering::Relaxed) != nav_id
            {
                return NewWindowResponse::Deny;
            }
            if is_download_url(&url) {
                let same_site = app
                    .get_webview(&callback_site_label)
                    .and_then(|site| site.url().ok())
                    .and_then(|current| current.host_str().map(str::to_string))
                    .is_some_and(|host| {
                        url.host_str()
                            .is_some_and(|next| next.eq_ignore_ascii_case(&host))
                    });
                let file_host =
                    bmm_domain::hosts::host_matches(&url, bmm_domain::hosts::KNOWN_FILE_HOSTS);
                if (same_site || file_host)
                    && let Some(site) = app.get_webview(&callback_site_label)
                {
                    let _ = site.navigate(url);
                }
            }
            NewWindowResponse::Deny
        }
    });
    let site_width = (window_width - side_space(window_width)).max(320.0);
    let site_height = (window_height - SITE_VERTICAL_SPACE).max(300.0);
    let site = window
        .add_child(
            site_builder,
            LogicalPosition::new(SITE_X, SITE_Y),
            LogicalSize::new(site_width, site_height),
        )
        .map_err(|error| AppError::new("site", error.to_string()))?;
    if !crate::commands::download_progress::attach(&site, app.clone(), session_id, nav_id) {
        let _ = site.close();
        return Err(AppError::new(
            "site",
            "Не удалось подключить контроль загрузок WebView2.",
        ));
    }
    active_site.commit();
    Ok(site)
}

/// Сколько ждать причину обрыва после сообщения о неудаче.
const FAILURE_GRACE: std::time::Duration = std::time::Duration::from_millis(300);

fn download_belongs_to_operation(path: &Path, session_id: u64, nav_id: u64) -> bool {
    DOWNLOAD_SESSIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&path_key(path))
        .is_some_and(|owner| owner.session_id == session_id && owner.nav_id == nav_id)
}

fn progress_belongs_to_operation(
    path: &Path,
    session_id: u64,
    nav_id: u64,
    expected_job_id: Option<u64>,
) -> bool {
    let sessions = DOWNLOAD_SESSIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    match sessions.get(&path_key(path)) {
        Some(owner) => {
            owner.session_id == session_id
                && owner.nav_id == nav_id
                && Some(owner.job_id) == expected_job_id
        }
        None => expected_job_id.is_none(),
    }
}

fn tauri_finish_job(path: &Path, session_id: u64, nav_id: u64) -> Option<u64> {
    let owner = DOWNLOAD_SESSIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&path_key(path))
        .copied()?;
    (owner.session_id == session_id && owner.nav_id == nav_id && !owner.terminal_handler)
        .then_some(owner.job_id)
}

pub(crate) fn disable_native_terminal(path: &Path, session_id: u64, nav_id: u64, job_id: u64) {
    let mut sessions = DOWNLOAD_SESSIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if let Some(owner) = sessions.get_mut(&path_key(path))
        && owner.session_id == session_id
        && owner.nav_id == nav_id
        && owner.job_id == job_id
    {
        owner.terminal_handler = false;
    }
}

fn forget_site_download(nav_id: u64, path: &Path) {
    let key = path_key(path);
    let mut downloads = SITE_DOWNLOADS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let empty = if let Some(site_downloads) = downloads.get_mut(&nav_id) {
        site_downloads.retain(|download| path_key(&download.path) != key);
        site_downloads.is_empty()
    } else {
        false
    };
    if empty {
        downloads.remove(&nav_id);
    }
}

fn take_site_downloads(nav_id: u64) -> Vec<PathBuf> {
    let _registration = REGISTRATION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    SITE_DOWNLOADS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&nav_id)
        .unwrap_or_default()
        .into_iter()
        .map(|download| download.path)
        .collect()
}

fn fail_site_downloads(app: &AppHandle, nav_id: u64, session_id: u64) {
    for path in take_site_downloads(nav_id) {
        finished_failed(app, path, session_id, nav_id, None);
    }
}

fn site_download_path(nav_id: u64, url: &Url, session_id: u64) -> Option<PathBuf> {
    let path = {
        let downloads = SITE_DOWNLOADS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        downloads
            .get(&nav_id)?
            .iter()
            .find(|download| download.url == *url)
            .map(|download| download.path.clone())
    }?;
    download_belongs_to_operation(&path, session_id, nav_id).then_some(path)
}

fn finish_owned_locked(
    path: &Path,
    session_id: u64,
    nav_id: u64,
    expected_job_id: Option<u64>,
    outcome: Result<(), String>,
) -> Option<DownloadJob> {
    let owner = DOWNLOAD_SESSIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(&path_key(path))
        .copied()?;
    if owner.session_id != session_id
        || owner.nav_id != nav_id
        || expected_job_id.is_some_and(|expected| expected != owner.job_id)
    {
        return None;
    }
    let job = jobs().finish(path, outcome);
    DOWNLOAD_SESSIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&path_key(path));
    forget_site_download(nav_id, path);
    job
}

#[cfg(not(windows))]
fn finish_interrupted(
    nav_id: u64,
    url: &Url,
    session_id: u64,
    outcome: Result<(), String>,
) -> Option<(PathBuf, DownloadJob)> {
    let _registration = REGISTRATION_GATE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let path = site_download_path(nav_id, url, session_id)?;
    let job = finish_owned_locked(&path, session_id, nav_id, None, outcome)?;
    Some((path, job))
}

pub(crate) fn finished_ok(
    app: &AppHandle,
    path: Option<&Path>,
    session_id: u64,
    nav_id: u64,
    expected_job_id: Option<u64>,
) {
    let Some(path) = path else { return };
    let job = {
        let _registration = REGISTRATION_GATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        finish_owned_locked(path, session_id, nav_id, expected_job_id, Ok(()))
    };
    let Some(job) = job else {
        return; // не наша загрузка
    };
    emit_jobs(app);
    crate::commands::downloads::note_finished(path);
    if is_archive_name(Path::new(&job.path)) {
        let _ = app.emit_to(PANEL_LABEL, "download://ready", job);
    }
}

/// Тексты обрыва: своя причина, причина WebView2 или общая.
fn failure_texts(
    reason: Option<crate::commands::download_progress::StopReason>,
) -> (&'static str, &'static str, String) {
    match reason {
        Some(reason) => {
            let (title, detail) = reason.texts();
            (title, detail, title.to_string())
        }
        None => (
            "Загрузка не удалась",
            "Файл не скачался. Попробуйте ещё раз или откройте страницу в браузере.",
            "Сайт не отдал файл.".to_string(),
        ),
    }
}

pub(crate) fn finished_failed(
    app: &AppHandle,
    path: PathBuf,
    session_id: u64,
    nav_id: u64,
    expected_job_id: Option<u64>,
) {
    // Итог — сразу: панель не должна ждать, а новая загрузка под тем же именем —
    // получить чужой обрыв.
    let (_, _, placeholder) = failure_texts(None);
    let job = {
        let _registration = REGISTRATION_GATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        finish_owned_locked(&path, session_id, nav_id, expected_job_id, Err(placeholder))
    };
    let Some(job) = job else {
        return; // не наша загрузка или окно уже закрыли
    };
    publish_finished_failure(app, &path, job, session_id, nav_id);
}

#[cfg(not(windows))]
fn finished_without_path(app: &AppHandle, url: &Url, success: bool, session_id: u64, nav_id: u64) {
    let outcome = if success {
        Ok(())
    } else {
        Err(failure_texts(None).2)
    };
    let Some((path, job)) = finish_interrupted(nav_id, url, session_id, outcome) else {
        return;
    };
    if success {
        emit_jobs(app);
        crate::commands::downloads::note_finished(&path);
        if is_archive_name(Path::new(&job.path)) {
            let _ = app.emit_to(PANEL_LABEL, "download://ready", job);
        }
    } else {
        publish_finished_failure(app, &path, job, session_id, nav_id);
    }
}

fn publish_finished_failure(
    app: &AppHandle,
    path: &Path,
    job: DownloadJob,
    session_id: u64,
    nav_id: u64,
) {
    // Недокачанный файл в папке загрузок не нужен, но новая загрузка с тем же
    // путём уже могла начаться — её защищает другой id.
    let _ = remove_inactive_download(path, job.id, &jobs());
    // Причину WebView2 сообщает своим событием, и оно может прийти чуть позже: без
    // неё панель писала бы «сайт не отдал файл» при удалённом из «Загрузок» файле.
    let app = app.clone();
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        std::thread::sleep(FAILURE_GRACE);
        let _session = SESSION_GATE
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if SESSION_ID.load(Ordering::Relaxed) != session_id
            || ACTIVE_NAV_ID.load(Ordering::Relaxed) != nav_id
        {
            return;
        }
        let reason = crate::commands::download_progress::take_stop_reason(&path, Some(job.id));
        let (title, detail, error) = failure_texts(reason);
        let Some(job) = jobs().explain(job.id, error.clone()) else {
            return;
        };
        emit_jobs(&app);
        if delayed_failure_is_current(&job, session_id, nav_id) {
            emit_state(&app, 2, title, detail, Some(error));
        }
    });
}

fn delayed_failure_is_current(job: &DownloadJob, session_id: u64, nav_id: u64) -> bool {
    if SESSION_ID.load(Ordering::Relaxed) != session_id
        || ACTIVE_NAV_ID.load(Ordering::Relaxed) != nav_id
    {
        return false;
    }
    let current = current_request();
    for_current_mod(page_of(job.request.as_ref()), page_of(current.as_ref()))
}

/// Окно не должно вылезать за рабочую область даже на маленьком экране.
fn window_size(app: &AppHandle) -> (f64, f64) {
    let (width, height) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let scale = monitor.scale_factor();
            let work = monitor.work_area().size;
            (work.width as f64 / scale, work.height as f64 / scale)
        })
        .unwrap_or((1240.0, 820.0));
    let width = (width - 32.0).clamp(760.0, 1240.0).min(width - 16.0);
    let height = (height - 32.0).clamp(560.0, 820.0).min(height - 16.0);
    (width.max(480.0), height.max(420.0))
}

fn side_space(window_width: f64) -> f64 {
    if window_width < 1000.0 {
        SITE_SIDE_SPACE_NARROW
    } else {
        SITE_SIDE_SPACE
    }
}

/// В dev панель — маршрут Vite (`/download`), в собранной программе — готовый asset.
/// Маршрут панели загрузки в интерфейсе (`src/routes/download`).
const PANEL_ROUTE: &str = "download";

fn panel_url(app: &AppHandle) -> WebviewUrl {
    // Адрес дев-сервера — только под `tauri dev`. Отладочная сборка
    // (`tauri build --debug`) идёт без сервера: по cfg!(debug_assertions) панель
    // загрузки открывалась пустой.
    if tauri::is_dev()
        && let Some(dev) = &app.config().build.dev_url
        && let Ok(url) = dev.join(PANEL_ROUTE)
    {
        return WebviewUrl::External(url);
    }
    // Именно «download», а не «download.html»: файл Tauri найдёт и так (download →
    // download.html), а роутер SvelteKit знает путь /download. По адресу
    // /download.html панель показывала «404 Not Found» вместо шагов.
    WebviewUrl::App(PANEL_ROUTE.into())
}

/// `blob:` и `data:` тоже бывают загрузками, хотя в них нет обычного адреса файла.
fn is_download_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "blob" | "data")
}

/// Скачивать из окна можно только архив мода по обычной ссылке браузера. Адрес —
/// под тем же запретом, что и переходы: ссылка «Скачать» на странице мода не должна
/// дёргать localhost или устройство в домашней сети. `blob:` и `data:` собирает сама
/// страница — сети за ними нет.
fn accepts_download(url: &Url, name: &str) -> bool {
    let address_ok = match url.scheme() {
        "http" | "https" => bmm_sources::net::redirect_allowed(url),
        _ => true,
    };
    is_download_url(url) && address_ok && is_archive_name(Path::new(name))
}

/// Страницу мода открываем только во внешнем браузере WebView2: localhost,
/// частные адреса и служебные схемы не должны попадать в него из выдачи сайта.
fn page_url_allowed(url: &Url) -> bool {
    bmm_sources::net::redirect_allowed(url)
}

fn is_archive_name(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "zip" | "rar" | "7z"
            )
        })
}

/// Ссылки «скачать» на страницах модов часто открываются в новой вкладке. Встроенному
/// окну нужна та же вкладка, иначе WebView2 теряет загрузку; рекламу и чужие popup не трогаем.
/// Скрипт для страницы мода: переходы «в новое окно» к файлу — в этом же окне.
const SITE_HELPERS: &str = include_str!("site_helpers.js");

fn site_helpers() -> String {
    let hosts = serde_json::to_string(bmm_domain::hosts::KNOWN_FILE_HOSTS)
        .unwrap_or_else(|_| "[]".to_string());
    SITE_HELPERS.replace("__FILE_HOSTS__", &hosts)
}

fn emit_state(app: &AppHandle, step: u8, title: &str, detail: &str, error: Option<String>) {
    let _ = app.emit_to(
        PANEL_LABEL,
        "download://state",
        DownloadPanelEvent {
            step,
            title: title.to_string(),
            detail: detail.to_string(),
            error,
        },
    );
}

fn download_name(url: &Url, destination: &Path) -> String {
    let from_destination = destination
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string);
    let from_url = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .map(str::to_string);
    // Иногда WebView2 предлагает имя без расширения, а настоящий ZIP виден в адресе.
    let raw = match (from_destination, from_url) {
        (Some(destination), Some(url_name))
            if !is_archive_name(Path::new(&destination))
                && is_archive_name(Path::new(&url_name)) =>
        {
            url_name
        }
        (Some(destination), _) => destination,
        (None, Some(url_name)) => url_name,
        (None, None) => "mod.zip".to_string(),
    };
    let raw = raw.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let safe: String = raw
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    let safe = safe.trim().trim_matches('.').to_string();
    if safe.is_empty() {
        "mod.zip".to_string()
    } else {
        avoid_reserved(preserve_extension(&safe, 160))
    }
}

/// Обрезка не должна съедать расширение: иначе длинное имя перестаёт считаться
/// архивом и WebView2 кладёт файл мимо перехвата.
fn preserve_extension(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let path = Path::new(name);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    if extension.is_empty() {
        return name.chars().take(max).collect();
    }
    let stem_max = max.saturating_sub(extension.chars().count());
    let stem: String = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("mod")
        .chars()
        .take(stem_max)
        .collect();
    if stem.is_empty() {
        format!("mod{extension}")
    } else {
        format!("{stem}{extension}")
    }
}

fn avoid_reserved(name: String) -> String {
    let stem = Path::new(&name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        format!("_{name}")
    } else {
        name
    }
}

fn unique_path(dir: &Path, name: &str, busy: &[PathBuf]) -> PathBuf {
    let free =
        |path: &Path| !path.exists() && !busy.iter().any(|taken| path_key(taken) == path_key(path));
    let candidate = dir.join(name);
    if free(&candidate) {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("mod");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    for index in 1..1000 {
        let next = dir.join(format!("{stem} ({index}){extension}"));
        if free(&next) {
            return next;
        }
    }
    dir.join(format!("{stem}-{}{extension}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    static SESSION_TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn window_opens_one_request_at_a_time() {
        // Два «Обновить» подряд шли одновременно, и архив первого мода приписывался
        // второму. Очередь — первым делом, до записи контекста.
        let source = include_str!("download_window.rs");
        for (start, end) in [
            (
                "pub async fn download_open(",
                "pub async fn download_open_batch(",
            ),
            (
                "pub async fn download_open_batch(",
                "pub async fn download_batch_goto(",
            ),
        ] {
            let body = &source[source.find(start).unwrap()..];
            let body = &body[..body.find(end).unwrap()];
            let gate = body.find("OPEN_GATE.lock().await").unwrap();
            let open = body.find("open_request_locked(").unwrap();
            assert!(gate < open, "очередь и контекст меняются под одним замком");
        }
        // Окно загрузки не восстанавливает прошлое место и видимость.
        let lib = include_str!("../lib.rs");
        let denylist = &lib[lib.find(".with_denylist(").unwrap()..];
        let denylist = &denylist[..denylist.find("])").unwrap()];
        assert!(denylist.contains("download_window::WINDOW_LABEL"));
    }

    #[test]
    fn direct_close_marks_retiring_before_destroyed() {
        let source = include_str!("download_window.rs");
        let callback = source
            .split("tauri::WindowEvent::CloseRequested")
            .nth(2)
            .unwrap();
        let callback = &callback[..callback.find("tauri::WindowEvent::Destroyed").unwrap()];
        assert!(callback.contains("RETIRING"));
    }

    #[test]
    fn destroyed_window_marks_retiring_before_async_cleanup() {
        let source = include_str!("download_window.rs");
        let callback = source
            .split("tauri::WindowEvent::Destroyed")
            .nth(1)
            .unwrap();
        let callback = &callback[..callback.find("_ => {}").unwrap()];
        let retiring = callback.find("RETIRING").unwrap();
        let cleanup = callback.find("spawn_blocking").unwrap();
        assert!(retiring < cleanup);
    }

    #[test]
    fn main_thread_never_waits_for_the_session_gate() {
        let source = include_str!("download_window.rs");
        let batch = &source[source.find("pub async fn download_batch(").unwrap()..];
        let batch = &batch[..batch.find("fn add_to_batch(").unwrap()];
        assert!(batch.contains("spawn_blocking"));

        let close = &source[source.find("pub async fn download_close(").unwrap()..];
        let close = &close[..close.find("fn add_to_batch(").unwrap()];
        assert!(close.contains("OPEN_GATE.lock().await"));
        assert!(close.contains(".destroy()"));

        let callback = source
            .split("tauri::WindowEvent::Destroyed")
            .nth(1)
            .unwrap();
        let callback = &callback[..callback.find("_ => {}").unwrap()];
        assert!(callback.contains("spawn_blocking"));
    }

    #[test]
    fn retiring_wait_checks_the_session_generation() {
        let source = include_str!("download_window.rs");
        let wait = source
            .split("async fn wait_for_retiring_window")
            .nth(1)
            .unwrap();
        assert!(wait.contains("RETIRING_SESSION_ID"));
        assert!(wait.contains("clear_retiring(&destroyed)"));
    }

    #[test]
    fn site_switch_finishes_previous_downloads_before_close() {
        let source = include_str!("download_window.rs");
        let open = source.split("fn open_request_locked").nth(1).unwrap();
        let fail = open.find("fail_site_downloads").unwrap();
        let close = open.find("old_site.close()").unwrap();
        assert!(fail < close);
    }

    #[test]
    fn close_guard_includes_ready_archives() {
        let source = include_str!("download_window.rs");
        let callback = source
            .split_once("tauri::WindowEvent::CloseRequested")
            .unwrap()
            .1;
        let callback = &callback[..callback.find("tauri::WindowEvent::CloseRequested").unwrap()];
        assert!(callback.contains("jobs().ready().is_empty()"));
    }

    #[test]
    fn window_wait_does_not_return_before_destroyed_cleanup() {
        let source = include_str!("download_window.rs");
        let wait = source
            .split("async fn wait_for_window_destruction")
            .nth(1)
            .unwrap();
        let wait = &wait[..wait.find("async fn wait_for_retiring_window").unwrap()];
        let notified = wait.find("destroyed.notified()").unwrap();
        let success = wait.find("return Ok(())").unwrap();
        assert!(
            notified < success,
            "исчезновения окна в manager недостаточно: cleanup приходит в Destroyed"
        );
    }

    #[test]
    fn site_creation_waits_for_main_thread_attach() {
        let source = include_str!("download_window.rs");
        let helper = source.split("fn run_on_main_thread").nth(1).unwrap();
        let helper = &helper[..helper.find("fn current_request").unwrap()];
        assert!(helper.contains(".recv()"));
        let open = source.split("fn open_request_locked").nth(1).unwrap();
        assert!(open.contains("run_on_main_thread(&app"));
    }

    #[test]
    fn active_site_guard_restores_previous_state_on_drop() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_nav = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let previous_site = CURRENT_SITE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        ACTIVE_NAV_ID.store(10, Ordering::Relaxed);
        *CURRENT_SITE.lock().unwrap() = Some("old-site".to_string());

        {
            let active = ActiveSite::new(11, "new-site".to_string());
            assert_eq!(ACTIVE_NAV_ID.load(Ordering::Relaxed), 11);
            assert_eq!(
                CURRENT_SITE
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .as_deref(),
                Some("new-site")
            );
            drop(active);
        }

        assert_eq!(ACTIVE_NAV_ID.load(Ordering::Relaxed), 10);
        assert_eq!(
            CURRENT_SITE
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_deref(),
            Some("old-site")
        );
        ACTIVE_NAV_ID.store(previous_nav, Ordering::Relaxed);
        *CURRENT_SITE.lock().unwrap() = previous_site;
    }

    #[test]
    fn failed_open_marks_the_window_as_retiring() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_batch = batch_snapshot();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_retiring = RETIRING
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_retiring_session = RETIRING_SESSION_ID.load(Ordering::Relaxed);
        let session = enter_session(&request("failed"), false);

        mark_retiring(&session);

        assert!(
            RETIRING
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .as_ref()
                .is_some_and(|destroyed| Arc::ptr_eq(destroyed, &session.destroyed))
        );
        drop(session);
        *CONTEXT.lock().unwrap() = previous_context;
        *BATCH.lock().unwrap() = previous_batch;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
        *RETIRING.lock().unwrap() = previous_retiring;
        RETIRING_SESSION_ID.store(previous_retiring_session, Ordering::Relaxed);
    }

    #[cfg(windows)]
    #[test]
    fn pathless_finish_is_not_guessed_on_windows() {
        let source = include_str!("download_window.rs");
        let callback = source.split("DownloadEvent::Finished").nth(1).unwrap();
        let callback = &callback[..callback.find("_ => {}").unwrap()];
        assert!(callback.contains("#[cfg(windows)]"));
        assert!(callback.contains("#[cfg(not(windows))]"));

        let progress = include_str!("download_progress.rs");
        let attach = progress.split("pub(crate) fn attach").nth(1).unwrap();
        let attach = &attach[..attach.find("// Сторож зависших").unwrap()];
        assert!(attach.contains("session_id"));
        assert!(attach.contains("nav_id"));
        assert!(attach.contains("finished_failed"));
    }

    #[test]
    fn download_owner_is_bound_to_the_site_that_requested_it() {
        let source = include_str!("download_window.rs");
        let body = &source[source.find("fn create_site(").unwrap()..];
        let body = &body[..body.find("/// Сколько ждать причину обрыва").unwrap()];
        assert!(!body.contains("current_request()"));
        assert!(body.contains("owner.clone()"));
    }

    #[test]
    fn single_add_uses_append_event_without_resetting_collected() {
        let source = include_str!("download_window.rs");
        let body = &source[source.find("pub async fn download_open(").unwrap()..];
        let body = &body[..body.find("pub async fn download_open_batch(").unwrap()];
        assert!(
            body.contains("\"download://batch-added\""),
            "одиночное добавление не должно выглядеть как новая пачка"
        );
        let append_return = body.find("return Ok(());").unwrap();
        let enter = body.find("enter_session_locked(").unwrap();
        assert!(
            enter > append_return,
            "append-only ветка не должна менять текущий контекст"
        );
    }

    #[test]
    fn batch_assignment_and_open_share_the_gate() {
        let source = include_str!("download_window.rs");
        let body = &source[source.find("pub async fn download_open_batch(").unwrap()..];
        let body = &body[..body.find("pub async fn download_batch_goto(").unwrap()];
        let gate = body.find("OPEN_GATE.lock().await").unwrap();
        let batch = body.find("*BATCH.lock()").unwrap();
        let open = body.find("open_request_locked(").unwrap();
        assert!(
            gate < batch && batch < open,
            "очередь и окно меняются под одним замком"
        );
    }

    #[test]
    fn download_name_stays_a_plain_file_name() {
        let url = Url::parse("https://example.com/files/mod.zip").unwrap();
        assert_eq!(
            download_name(&url, Path::new(r"C:\Downloads\bad:name?.zip")),
            "bad_name_.zip"
        );
        assert_eq!(download_name(&url, Path::new("")), "mod.zip");
        assert_eq!(
            download_name(&url, Path::new(r"..\..\evil.zip")),
            "evil.zip"
        );
        assert_eq!(
            download_name(&url, Path::new(r"C:\Downloads\download")),
            "mod.zip",
            "расширение берём из адреса"
        );
    }

    #[test]
    fn long_download_name_keeps_archive_extension() {
        let url = Url::parse("https://example.com/files/mod.zip").unwrap();
        let long = format!("{}.zip", "a".repeat(300));
        let name = download_name(&url, Path::new(&long));
        assert!(name.ends_with(".zip"), "{name}");
        assert!(name.chars().count() <= 160, "{name}");
        assert_eq!(
            download_name(&url, Path::new("CON.zip")),
            "_CON.zip",
            "зарезервированное имя Windows"
        );
    }

    #[test]
    fn only_archives_are_caught() {
        assert!(is_archive_name(Path::new("mod.zip")));
        assert!(is_archive_name(Path::new("mod.RAR")));
        assert!(is_archive_name(Path::new("mod.7z")));
        assert!(!is_archive_name(Path::new("setup.exe")));
        assert!(!is_archive_name(Path::new("image.png")));
    }

    #[test]
    fn only_mod_archives_are_downloaded_from_the_window() {
        let page = Url::parse("https://modsfire.com/d/abc").unwrap();
        assert!(accepts_download(&page, "BMW_M2_v1.5.zip"));
        assert!(accepts_download(&page, "map.7z"));
        // Установщики, картинки, документы и архив без расширения — нет.
        for name in [
            "setup.exe",
            "update.msi",
            "preview.png",
            "readme.pdf",
            "mod",
        ] {
            assert!(!accepts_download(&page, name), "{name}");
        }
        let local = Url::parse("file:///C:/mods/mod.zip").unwrap();
        assert!(!accepts_download(&local, "mod.zip"));
    }

    #[test]
    fn page_script_knows_the_file_hosts() {
        let script = site_helpers();
        assert!(!script.contains("__FILE_HOSTS__"));
        assert!(script.contains("\"modsfire.com\""));
    }

    #[test]
    fn error_of_a_previous_mod_does_not_land_in_the_current_steps() {
        let a = Some("https://stmods.org/beamng_drive/mods/a/");
        let b = Some("https://stmods.org/beamng_drive/mods/b/");
        assert!(for_current_mod(a, a));
        assert!(
            !for_current_mod(a, b),
            "оборвалась после перехода к другому моду"
        );
        assert!(!for_current_mod(a, None), "окно уже без мода");
        assert!(for_current_mod(None, None));
    }

    #[test]
    fn delayed_failure_stops_matching_when_the_window_generation_changes() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_nav = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let old = enter_session(&request("same"), false);
        let old_session = old.id;
        let old_nav = 41;
        ACTIVE_NAV_ID.store(old_nav, Ordering::Relaxed);
        let path = PathBuf::from("same.zip");
        let mut active = Jobs::default();
        active.start(&path, Some(request("same")));
        let failed = active
            .finish(&path, Err("Сайт оборвал загрузку.".to_string()))
            .unwrap();
        *jobs() = active;
        assert!(delayed_failure_is_current(&failed, old_session, old_nav));

        drop(old);
        let new = enter_session(&request("same"), false);
        let new_session = new.id;
        ACTIVE_NAV_ID.store(42, Ordering::Relaxed);

        assert!(
            !delayed_failure_is_current(&failed, old_session, old_nav),
            "та же страница в новом окне не принадлежит старой ошибке"
        );

        drop(new);
        assert!(clear_session_for_test(new_session));
        ACTIVE_NAV_ID.store(previous_nav, Ordering::Relaxed);
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    fn request(page: &str) -> DownloadRequest {
        DownloadRequest {
            mod_name: page.to_string(),
            installed_version: None,
            latest_version: None,
            page_url: format!("https://stmods.org/beamng_drive/mods/{page}/"),
            preview_url: None,
            role: None,
            mod_id: None,
        }
    }

    #[test]
    fn jobs_keep_the_original_path() {
        let mut jobs = Jobs::default();
        let path = PathBuf::from(r"C:\Temp\İ.zip");
        jobs.start(&path, None);
        assert_eq!(jobs.items[0].path, path.display().to_string());
        assert!(jobs.finish(&path, Ok(())).is_some());
    }

    #[test]
    fn late_finish_from_a_previous_site_does_not_own_a_new_download() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let previous_downloads = DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_site_downloads = SITE_DOWNLOADS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_nav = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("same.zip");
        let session = enter_session(&request("owner"), false);
        let session_id = session.id;

        ACTIVE_NAV_ID.store(1, Ordering::Relaxed);
        assert!(register_download_for_test(
            &path,
            request("old"),
            session_id,
            1
        ));
        let old_job_id = jobs().snapshot()[0].id;
        {
            let _registration = REGISTRATION_GATE
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            assert!(finish_owned_locked(&path, session_id, 1, None, Err("old".into())).is_some());
        }

        ACTIVE_NAV_ID.store(2, Ordering::Relaxed);
        assert!(register_download_for_test(
            &path,
            request("new"),
            session_id,
            2
        ));
        {
            let _registration = REGISTRATION_GATE
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            assert!(finish_owned_locked(&path, session_id, 1, None, Err("late".into())).is_none());
        }
        assert_eq!(jobs().downloading(), vec![path.clone()]);
        {
            let _registration = REGISTRATION_GATE
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            assert!(
                finish_owned_locked(&path, session_id, 2, Some(old_job_id), Err("late".into()))
                    .is_none()
            );
        }
        assert_eq!(jobs().downloading(), vec![path.clone()]);
        {
            let _registration = REGISTRATION_GATE
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            assert!(finish_owned_locked(&path, session_id, 2, None, Ok(())).is_some());
        }

        drop(session);
        assert!(clear_session_for_test(session_id));
        ACTIVE_NAV_ID.store(previous_nav, Ordering::Relaxed);
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *DOWNLOAD_SESSIONS.lock().unwrap() = previous_downloads;
        *SITE_DOWNLOADS.lock().unwrap() = previous_site_downloads;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    #[test]
    fn site_downloads_keep_the_original_path() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous = SITE_DOWNLOADS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let path = PathBuf::from(r"C:\Temp\İ.zip");
        SITE_DOWNLOADS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                77,
                vec![SiteDownload {
                    path: path.clone(),
                    url: Url::parse("https://example.com/mod.zip").unwrap(),
                }],
            );

        assert_eq!(take_site_downloads(77), vec![path]);
        *SITE_DOWNLOADS.lock().unwrap() = previous;
    }

    #[test]
    fn two_downloads_at_once_keep_their_own_mods() {
        // Было: второй мод уводил окно, архив первого считался «чужим» и терялся,
        // а ход обоих шёл в одну полоску.
        let mut jobs = Jobs::default();
        let first = PathBuf::from("first.zip");
        let second = PathBuf::from("second.zip");
        jobs.start(&first, Some(request("a")));
        jobs.start(&second, Some(request("b")));
        assert_eq!(jobs.downloading(), vec![first.clone(), second.clone()]);

        let done = jobs.finish(&second, Ok(())).expect("наша загрузка");
        assert_eq!(done.request.unwrap().mod_name, "b");
        assert_eq!(done.state, DownloadJobState::Done);
        assert_eq!(
            jobs.downloading(),
            vec![first.clone()],
            "первая ещё качается"
        );

        let late = jobs.finish(&first, Ok(())).expect("первая тоже наша");
        assert_eq!(late.request.unwrap().mod_name, "a", "архив — своему моду");
        assert_eq!(jobs.ready().len(), 2);
        assert!(jobs.downloading().is_empty());
    }

    #[test]
    fn job_ids_do_not_reset_between_window_sessions() {
        let mut first = Jobs::default();
        let mut second = Jobs::default();
        first.start(Path::new("mod.zip"), None);
        second.start(Path::new("mod.zip"), None);
        assert_ne!(first.items[0].id, second.items[0].id);
    }

    #[test]
    fn stale_session_clear_cannot_erase_a_new_session() {
        use std::sync::{Arc, Barrier, mpsc};
        use std::time::{Duration, Instant};

        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let previous_site = CURRENT_SITE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_nav = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let expected = SESSION_ID.load(Ordering::Relaxed);
        *CONTEXT.lock().unwrap() = Some(request("old"));
        *jobs() = Jobs::default();

        // Держим общий gate: новая сессия не должна сменить поколение и контекст,
        // пока старый callback очищает их. Barrier запускает оба Worker вместе.
        let gate = SESSION_GATE.lock().unwrap();
        let barrier = Arc::new(Barrier::new(3));
        let (clear_done_tx, clear_done_rx) = mpsc::channel();
        let (open_done_tx, open_done_rx) = mpsc::channel();
        let clear_barrier = Arc::clone(&barrier);
        let clear_thread = std::thread::spawn(move || {
            clear_barrier.wait();
            let cleared = clear_session_for_test(expected);
            clear_done_tx.send(cleared).unwrap();
        });
        let open_barrier = Arc::clone(&barrier);
        let open_thread = std::thread::spawn(move || {
            open_barrier.wait();
            let replacement = request("new");
            let session = enter_session(&replacement, false);
            jobs().start(Path::new("replacement.zip"), Some(replacement));
            open_done_tx.send(session.id).unwrap();
        });
        barrier.wait();

        let deadline = Instant::now() + Duration::from_secs(1);
        let clear_finished_early = clear_done_rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok();
        let open_finished_early = open_done_rx
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok();
        let finished_while_gated = clear_finished_early.is_some() || open_finished_early.is_some();
        drop(gate);

        clear_thread.join().unwrap();
        open_thread.join().unwrap();
        clear_finished_early
            .or_else(|| clear_done_rx.recv().ok())
            .unwrap();
        open_finished_early
            .or_else(|| open_done_rx.recv().ok())
            .unwrap();
        let current = current_request();
        let replacement_jobs = jobs().snapshot();
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *CURRENT_SITE.lock().unwrap() = previous_site;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
        ACTIVE_NAV_ID.store(previous_nav, Ordering::Relaxed);

        assert!(
            !finished_while_gated,
            "смена сессии и очистка должны ждать общий gate"
        );
        assert!(
            SESSION_ID.load(Ordering::Relaxed) > expected,
            "новая сессия получает новое поколение"
        );
        assert_eq!(current.unwrap().mod_name, "new");
        assert_eq!(replacement_jobs.len(), 1);
        assert_eq!(
            replacement_jobs[0].request.as_ref().unwrap().mod_name,
            "new",
            "старый callback не удаляет загрузки новой сессии"
        );
    }

    #[test]
    fn batch_read_waits_for_the_session_guard() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let session = enter_session(&request("held"), false);
        *BATCH.lock().unwrap() = vec![request("queued")];
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let read = std::thread::spawn(move || {
            let batch = blocking_batch_snapshot();
            done_tx.send(batch).unwrap();
        });

        assert!(matches!(
            done_rx.recv_timeout(std::time::Duration::from_millis(50)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        drop(session);
        read.join().unwrap();
        assert_eq!(
            done_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .unwrap()
                .len(),
            1
        );
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    #[test]
    fn replacement_window_gets_a_new_generation() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let mut session = enter_session(&request("old"), true);
        let old = session.id;

        session.renew_for_new_window();

        assert!(session.id > old);
        let current = session.id;
        drop(session);
        assert!(!clear_session_for_test(old));
        assert!(clear_session_for_test(current));
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
    }

    #[test]
    fn destroyed_notification_belongs_to_one_window_instance() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let mut session = enter_session(&request("old"), false);
        let old = Arc::clone(&session.destroyed);

        session.renew_for_new_window();

        assert!(!Arc::ptr_eq(&old, &session.destroyed));
        let stored = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
            .unwrap();
        assert_eq!(stored.0, session.id);
        assert!(Arc::ptr_eq(&stored.1, &session.destroyed));
        drop(session);
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    #[test]
    fn download_registered_after_cleanup_is_rejected() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let previous_downloads = DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_site = CURRENT_SITE
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_nav = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let temp = tempfile::tempdir().unwrap();
        let session = enter_session(&request("owner"), false);
        let session_id = session.id;
        let nav_id = 77;
        ACTIVE_NAV_ID.store(nav_id, Ordering::Relaxed);

        assert!(register_download_for_test(
            &temp.path().join("first.zip"),
            request("owner"),
            session_id,
            nav_id,
        ));
        assert_eq!(jobs().snapshot().len(), 1);
        drop(session);
        assert!(clear_session_for_test(session_id));
        assert!(!register_download_for_test(
            &temp.path().join("late.zip"),
            request("owner"),
            session_id,
            nav_id,
        ));
        assert!(jobs().snapshot().is_empty());

        ACTIVE_NAV_ID.store(previous_nav, Ordering::Relaxed);
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *DOWNLOAD_SESSIONS.lock().unwrap() = previous_downloads;
        *CURRENT_SITE.lock().unwrap() = previous_site;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    #[test]
    fn late_finish_from_an_old_session_does_not_own_a_new_download() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_downloads = DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_site_downloads = SITE_DOWNLOADS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_registrations = DOWNLOAD_REGISTRATIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_batch = batch_snapshot();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_nav = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("same.zip");

        let old = enter_session(&request("old"), false);
        let old_id = old.id;
        ACTIVE_NAV_ID.store(1, Ordering::Relaxed);
        assert!(register_download_for_test(&path, request("old"), old_id, 1,));
        drop(old);
        assert!(clear_session_for_test(old_id));

        let new = enter_session(&request("new"), false);
        let new_id = new.id;
        ACTIVE_NAV_ID.store(2, Ordering::Relaxed);
        assert!(register_download_for_test(&path, request("new"), new_id, 2,));
        assert!(!download_belongs_to_operation(&path, old_id, 1));
        assert!(download_belongs_to_operation(&path, new_id, 2));

        drop(new);
        assert!(clear_session_for_test(new_id));
        ACTIVE_NAV_ID.store(previous_nav, Ordering::Relaxed);
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *DOWNLOAD_SESSIONS.lock().unwrap() = previous_downloads;
        *SITE_DOWNLOADS.lock().unwrap() = previous_site_downloads;
        *DOWNLOAD_REGISTRATIONS.lock().unwrap() = previous_registrations;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    #[test]
    fn rejected_duplicate_keeps_the_operation_queue_aligned() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let nav_id = 901;
        let first = PathBuf::from("first.zip");
        begin_download_registration(nav_id);
        accept_download_registration(nav_id, &first, 1);
        begin_download_registration(nav_id);

        assert!(matches!(
            take_download_registration(nav_id),
            Some(DownloadRegistration::Accepted(path, _)) if path == first
        ));
        assert!(matches!(
            take_download_registration(nav_id),
            Some(DownloadRegistration::Rejected)
        ));
        assert!(take_download_registration(nav_id).is_none());
    }

    #[test]
    fn interrupted_operation_finishes_only_its_exact_path() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let previous_downloads = DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_site_downloads = SITE_DOWNLOADS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_nav = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.zip");
        let second = temp.path().join("second.zip");
        let first_url = Url::parse("https://example.com/first.zip").unwrap();
        let second_url = Url::parse("https://example.com/second.zip").unwrap();
        let session = enter_session(&request("owner"), false);
        let session_id = session.id;
        let nav_id = 55;
        ACTIVE_NAV_ID.store(nav_id, Ordering::Relaxed);

        assert!(register_download_url_for_test(
            &first,
            &first_url,
            request("a"),
            session_id,
            nav_id,
        ));
        assert!(register_download_url_for_test(
            &second,
            &second_url,
            request("b"),
            session_id,
            nav_id,
        ));
        let finished_job = {
            let _registration = REGISTRATION_GATE
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            finish_owned_locked(
                &first,
                session_id,
                nav_id,
                None,
                Err("Сайт оборвал загрузку.".to_string()),
            )
        }
        .expect("оборванная загрузка");

        assert_eq!(finished_job.state, DownloadJobState::Failed);
        assert_eq!(jobs().downloading(), vec![second.clone()]);
        let remaining = SITE_DOWNLOADS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&nav_id)
            .cloned()
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].path, second.clone());
        assert!(!download_belongs_to_operation(&first, session_id, nav_id));
        assert!(download_belongs_to_operation(&second, session_id, nav_id));

        drop(session);
        assert!(clear_session_for_test(session_id));
        ACTIVE_NAV_ID.store(previous_nav, Ordering::Relaxed);
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *DOWNLOAD_SESSIONS.lock().unwrap() = previous_downloads;
        *SITE_DOWNLOADS.lock().unwrap() = previous_site_downloads;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    #[test]
    fn the_same_url_cannot_start_two_untracked_operations() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        let previous_downloads = DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_site_downloads = SITE_DOWNLOADS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let previous_nav = ACTIVE_NAV_ID.load(Ordering::Relaxed);
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first.zip");
        let duplicate = temp.path().join("duplicate.zip");
        let url = Url::parse("https://example.com/mod.zip").unwrap();
        let session = enter_session(&request("owner"), false);
        let session_id = session.id;
        let nav_id = 56;
        ACTIVE_NAV_ID.store(nav_id, Ordering::Relaxed);

        assert!(register_download_url_for_test(
            &first,
            &url,
            request("owner"),
            session_id,
            nav_id,
        ));
        assert!(!register_download_url_for_test(
            &duplicate,
            &url,
            request("owner"),
            session_id,
            nav_id,
        ));
        assert_eq!(jobs().downloading(), vec![first.clone()]);
        assert_eq!(
            SITE_DOWNLOADS
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get(&nav_id)
                .map(Vec::len),
            Some(1)
        );

        drop(session);
        assert!(clear_session_for_test(session_id));
        ACTIVE_NAV_ID.store(previous_nav, Ordering::Relaxed);
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
        *DOWNLOAD_SESSIONS.lock().unwrap() = previous_downloads;
        *SITE_DOWNLOADS.lock().unwrap() = previous_site_downloads;
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    #[test]
    fn native_terminal_ownership_matches_the_platform() {
        let source = include_str!("download_window.rs");
        assert!(source.contains("terminal_handler: cfg!(windows)"));
    }

    #[test]
    fn tauri_completion_only_handles_operations_without_a_native_handler() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous = DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let path = Path::new("mod.zip");
        DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                path_key(path),
                DownloadOwnership {
                    session_id: 42,
                    nav_id: 7,
                    job_id: 9,
                    terminal_handler: true,
                },
            );
        assert!(tauri_finish_job(path, 42, 7).is_none());

        disable_native_terminal(path, 42, 7, 9);

        assert_eq!(tauri_finish_job(path, 42, 7), Some(9));
        *DOWNLOAD_SESSIONS.lock().unwrap() = previous;
    }

    #[test]
    fn progress_ownership_rejects_a_stale_job_for_the_same_path() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous = DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let path = Path::new("mod.zip");
        DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                path_key(path),
                DownloadOwnership {
                    session_id: 42,
                    nav_id: 7,
                    job_id: 9,
                    terminal_handler: true,
                },
            );

        assert!(progress_belongs_to_operation(path, 42, 7, Some(9)));
        assert!(!progress_belongs_to_operation(path, 42, 7, Some(8)));
        assert!(!progress_belongs_to_operation(path, 42, 7, None));
        assert!(!progress_belongs_to_operation(path, 41, 7, Some(9)));

        *DOWNLOAD_SESSIONS.lock().unwrap() = previous;
    }

    #[cfg(windows)]
    #[test]
    fn windows_path_key_uses_windows_case_rules() {
        assert_eq!(path_key(Path::new("Ä.zip")), path_key(Path::new("ä.zip")));
        assert_ne!(
            path_key(Path::new("İ.zip")),
            path_key(Path::new("i\u{307}.zip"))
        );
    }

    #[cfg(windows)]
    #[test]
    fn download_ownership_matches_windows_paths_case_insensitively() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous = DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        DOWNLOAD_SESSIONS
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                path_key(Path::new("Mod.zip")),
                DownloadOwnership {
                    session_id: 42,
                    nav_id: 7,
                    job_id: 9,
                    terminal_handler: true,
                },
            );

        assert!(download_belongs_to_operation(Path::new("mod.zip"), 42, 7));

        *DOWNLOAD_SESSIONS.lock().unwrap() = previous;
    }

    #[test]
    fn closing_session_clears_the_batch_queue() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_batch = batch_snapshot();
        *BATCH.lock().unwrap() = vec![request("stale")];

        assert!(clear_session_for_test(SESSION_ID.load(Ordering::Relaxed)));

        assert!(batch_snapshot().is_empty());
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *BATCH.lock().unwrap() = previous_batch;
    }

    #[test]
    fn session_guard_blocks_clear_until_open_finishes() {
        let _test_lock = SESSION_TEST_LOCK.lock().unwrap();
        let previous_context = current_request();
        let previous_jobs = jobs().snapshot();
        let previous_window = WINDOW_DESTROYED
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let session = enter_session(&request("held"), false);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let expected = session.id;
        let clear = std::thread::spawn(move || {
            done_tx.send(clear_session_for_test(expected)).unwrap();
        });

        assert!(matches!(
            done_rx.recv_timeout(std::time::Duration::from_millis(50)),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
        ));
        drop(session);
        let cleared = done_rx.recv_timeout(std::time::Duration::from_secs(1));
        assert_eq!(
            cleared,
            Ok(true),
            "expected {expected}, current {}",
            SESSION_ID.load(Ordering::Relaxed)
        );
        clear.join().unwrap();
        *CONTEXT.lock().unwrap() = previous_context;
        *jobs() = Jobs {
            items: previous_jobs,
        };
        *WINDOW_DESTROYED.lock().unwrap() = previous_window;
    }

    #[cfg(windows)]
    #[test]
    fn jobs_match_windows_paths_case_insensitively() {
        let mut jobs = Jobs::default();
        jobs.start(Path::new("Mod.zip"), None);
        assert!(jobs.finish(Path::new("mod.zip"), Ok(())).is_some());
    }

    #[test]
    fn cleanup_does_not_remove_a_new_download_at_the_same_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        std::fs::write(&path, b"old partial").unwrap();
        let mut jobs = Jobs::default();
        jobs.start(&path, None);
        let old_id = jobs.items[0].id;
        assert!(jobs.finish(&path, Err("cancelled".to_string())).is_some());
        std::fs::write(&path, b"new download").unwrap();
        jobs.start(&path, None);
        assert!(matches!(
            remove_inactive_download(&path, old_id, &jobs),
            InactiveRemoval::Replaced
        ));
        assert!(path.exists(), "новую загрузку нельзя удалить");
    }

    #[test]
    fn cleanup_does_not_remove_a_done_replacement_at_the_same_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        std::fs::write(&path, b"old partial").unwrap();
        let mut jobs = Jobs::default();
        jobs.start(&path, None);
        let old_id = jobs.items[0].id;

        std::fs::write(&path, b"finished replacement").unwrap();
        jobs.start(&path, None);
        let replacement = jobs.finish(&path, Ok(())).expect("наша загрузка");
        assert_eq!(replacement.state, DownloadJobState::Done);

        assert!(matches!(
            remove_inactive_download(&path, old_id, &jobs),
            InactiveRemoval::Replaced
        ));
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"finished replacement",
            "завершённую replacement нельзя удалить"
        );
    }

    #[test]
    fn finished_download_is_handed_over_once() {
        let mut jobs = Jobs::default();
        let path = PathBuf::from("mod.zip");
        assert!(jobs.finish(&path, Ok(())).is_none(), "не наша загрузка");
        jobs.start(&path, None);
        assert!(jobs.finish(&path, Ok(())).is_some());
        assert!(jobs.finish(&path, Ok(())).is_none(), "второй раз не отдаём");
    }

    #[test]
    fn failed_download_keeps_its_reason() {
        let mut jobs = Jobs::default();
        let path = PathBuf::from("mod.zip");
        jobs.start(&path, Some(request("a")));
        let failed = jobs
            .finish(&path, Err("Загрузка зависла".to_string()))
            .unwrap();
        assert_eq!(failed.state, DownloadJobState::Failed);
        assert_eq!(failed.error.as_deref(), Some("Загрузка зависла"));
        assert!(jobs.ready().is_empty(), "оборванный архив ставить нечего");
        // Тот же файл скачали заново — это новая загрузка.
        jobs.start(&path, Some(request("a")));
        assert_eq!(jobs.downloading(), vec![path]);
        assert_eq!(jobs.snapshot().len(), 1);
    }

    #[test]
    fn late_reason_goes_to_its_own_download_only() {
        // Обрыв уточняется чуть позже; за это время под тем же именем могла начаться
        // новая загрузка — её помечать нельзя.
        let mut jobs = Jobs::default();
        let path = PathBuf::from("mod.zip");
        jobs.start(&path, Some(request("a")));
        let failed = jobs
            .finish(&path, Err("Сайт не отдал файл.".to_string()))
            .unwrap();
        jobs.start(&path, Some(request("a")));
        assert!(
            jobs.explain(failed.id, "Файл загрузки пропал".to_string())
                .is_none()
        );
        assert_eq!(jobs.downloading(), vec![path.clone()], "новая идёт дальше");

        let mut jobs = Jobs::default();
        jobs.start(&path, None);
        let failed = jobs
            .finish(&path, Err("Сайт не отдал файл.".to_string()))
            .unwrap();
        let explained = jobs
            .explain(failed.id, "Файл загрузки пропал".to_string())
            .unwrap();
        assert_eq!(explained.error.as_deref(), Some("Файл загрузки пропал"));
    }

    #[test]
    fn failure_says_what_happened() {
        use crate::commands::download_progress::StopReason;
        let (title, _, error) = failure_texts(Some(StopReason::FileGone));
        assert_eq!(title, "Файл загрузки пропал");
        assert_eq!(error, title, "в строке загрузки — та же причина");
        let (title, _, error) = failure_texts(None);
        assert_eq!(title, "Загрузка не удалась");
        assert_eq!(error, "Сайт не отдал файл.");
    }

    #[test]
    fn many_finished_downloads_do_not_push_out_running_ones() {
        let mut jobs = Jobs::default();
        let running = PathBuf::from("running.zip");
        jobs.start(&running, None);
        for index in 0..(JOBS_KEEP * 2) {
            let path = PathBuf::from(format!("done-{index}.zip"));
            jobs.start(&path, None);
            jobs.finish(&path, Ok(()));
        }
        assert!(jobs.snapshot().len() <= JOBS_KEEP);
        assert_eq!(jobs.downloading(), vec![running]);
    }

    #[test]
    fn new_mod_during_a_batch_joins_its_queue() {
        let mut batch = Vec::new();
        assert!(!add_to_batch(&mut batch, &request("a")), "пачки нет");
        assert!(batch.is_empty());

        let mut batch = vec![request("a"), request("b")];
        assert!(add_to_batch(&mut batch, &request("c")));
        assert_eq!(batch.len(), 3);
        assert!(add_to_batch(&mut batch, &request("b")), "уже в очереди");
        assert_eq!(batch.len(), 3, "второй раз не добавляем");
    }

    #[test]
    fn archive_link_into_the_home_network_is_not_downloaded() {
        // Ссылка «Скачать» на странице мода вела бы на роутер или localhost.
        for raw in [
            "http://localhost/mod.zip",
            "http://localhost./mod.zip",
            "http://127.0.0.1:8080/mod.zip",
            "http://2130706433/mod.zip",
            "http://0.0.0.0/mod.zip",
            "http://192.168.1.1/mod.zip",
            "http://10.0.0.5/mod.zip",
            "http://172.16.0.1/mod.zip",
            "http://169.254.1.1/mod.zip",
            "http://[::1]/mod.zip",
            "http://[fe80::1]/mod.zip",
            "http://[fd00::1]/mod.zip",
            "http://[::ffff:127.0.0.1]/mod.zip",
            "http://mods.localhost/mod.zip",
        ] {
            let url = Url::parse(raw).unwrap();
            assert!(!accepts_download(&url, "mod.zip"), "{raw}");
        }
        // Архив, собранный самой страницей, сети не трогает.
        let blob = Url::parse("blob:https://modsfire.com/1b2c").unwrap();
        assert!(accepts_download(&blob, "mod.zip"));
    }

    #[test]
    fn panel_route_exists_in_the_interface() {
        // Адрес панели — путь маршрута SvelteKit, без «.html»: иначе роутер не
        // находит страницу и вместо шагов загрузки показывает 404.
        let route = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../src/routes")
            .join(PANEL_ROUTE)
            .join("+page.svelte");
        assert!(route.is_file(), "нет страницы {}", route.display());
        assert!(!PANEL_ROUTE.ends_with(".html"));
    }

    #[test]
    fn duplicate_download_gets_a_number() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("mod.zip"), b"old").unwrap();
        assert_eq!(
            unique_path(temp.path(), "mod.zip", &[]),
            temp.path().join("mod (1).zip")
        );
    }

    #[test]
    fn name_of_a_running_download_is_taken() {
        // Файл появляется не сразу: две загрузки с одним именем писали в один файл.
        let temp = tempfile::tempdir().unwrap();
        let running = vec![temp.path().join("mod.zip")];
        assert_eq!(
            unique_path(temp.path(), "mod.zip", &running),
            temp.path().join("mod (1).zip")
        );
        let both = vec![temp.path().join("mod.zip"), temp.path().join("mod (1).zip")];
        assert_eq!(
            unique_path(temp.path(), "mod.zip", &both),
            temp.path().join("mod (2).zip")
        );
    }

    #[test]
    fn browser_download_schemes_are_allowed() {
        for raw in [
            "https://example.com/mod.zip",
            "http://example.com/mod.zip",
            "blob:https://example.com/id",
            "data:application/zip;base64,AAAA",
        ] {
            assert!(is_download_url(&Url::parse(raw).unwrap()), "{raw}");
        }
        assert!(!is_download_url(
            &Url::parse("file:///C:/Windows/System32/calc.exe").unwrap()
        ));
    }

    #[test]
    fn page_urls_must_be_public_http() {
        for raw in [
            "https://stmods.org/beamng_drive/mods/example/",
            "http://example.com/page",
        ] {
            assert!(page_url_allowed(&Url::parse(raw).unwrap()), "{raw}");
        }
        for raw in [
            "http://127.0.0.1/service",
            "http://localhost/service",
            "http://192.168.1.10/router",
            "http://[::1]/service",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
        ] {
            assert!(!page_url_allowed(&Url::parse(raw).unwrap()), "{raw}");
        }
    }
}

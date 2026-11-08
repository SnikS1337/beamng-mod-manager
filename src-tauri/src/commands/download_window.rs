//! Окно загрузки обновления: слева страница мода/файлообменника,
//! справа — панель с этапами. Оба вида — child webview одного нативного окна.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

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
    /// Номер загрузки: путь бывает тот же у новой загрузки после старой.
    #[serde(skip)]
    #[ts(skip)]
    id: u64,
    pub path: String,
    pub file_name: String,
    pub request: Option<DownloadRequest>,
    pub state: DownloadJobState,
    pub error: Option<String>,
}

/// Сколько закончившихся загрузок помнить: панели хватает, память не растёт.
const JOBS_KEEP: usize = 64;

#[derive(Debug, Default)]
pub(crate) struct Jobs {
    items: Vec<DownloadJob>,
    next_id: u64,
}

impl Jobs {
    /// Окно начало качать файл для этого мода. Тот же путь — новая загрузка.
    fn start(&mut self, path: &Path, request: Option<DownloadRequest>) {
        let key = path.display().to_string();
        self.items.retain(|job| job.path != key);
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
        self.next_id += 1;
        self.items.push(DownloadJob {
            id: self.next_id,
            file_name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            path: key,
            request,
            state: DownloadJobState::Downloading,
            error: None,
        });
    }

    /// Загрузка кончилась. `None` — файл не наш или его итог уже записан: второй
    /// раз один архив панели не отдаём.
    fn finish(&mut self, path: &Path, outcome: Result<(), String>) -> Option<DownloadJob> {
        let key = path.display().to_string();
        let job = self
            .items
            .iter_mut()
            .find(|job| job.path == key && job.state == DownloadJobState::Downloading)?;
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
) {
    let _ = app.emit_to(PANEL_LABEL, "download://progress", progress.clone());
    let _ = app.emit_to(MAIN_LABEL, "download://progress", progress);
}

const MAIN_LABEL: &str = "main";

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

/// Сессия окна закончилась: панель, контекст и загрузки больше не нужны.
/// Незаконченная загрузка после этого снова видна сторожу «Загрузок».
fn clear_session() {
    BATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = None;
    *jobs() = Jobs::default();
    crate::commands::downloads::forget_caught();
}

/// Оборвать загрузки, которые ещё идут, и убрать недокачанные файлы. Человек сам
/// решил закрыть окно или программу — иначе файл качался бы дальше без хозяина
/// и пропадал при выходе.
pub(crate) fn cancel_downloads(app: &AppHandle) {
    let unfinished = jobs().downloading();
    if unfinished.is_empty() {
        return;
    }
    if let Some(site) = app.get_webview(SITE_LABEL) {
        crate::commands::download_progress::cancel_all(&site);
    }
    // WebView2 обычно сам удаляет оборванный файл; если он ещё держит его, пробуем
    // ещё несколько раз.
    std::thread::spawn(move || {
        for _ in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(700));
            let mut left = false;
            for path in &unfinished {
                if path.exists() && std::fs::remove_file(path).is_err() {
                    left = true;
                }
            }
            if !left {
                break;
            }
        }
    });
}

#[tauri::command]
pub fn download_close(app: AppHandle) {
    cancel_downloads(&app);
    clear_session();
    emit_jobs(&app);
    if let Some(window) = app.get_window(WINDOW_LABEL) {
        let _ = window.close();
    }
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

/// Очередь пачки для панели (пустая — обычное обновление одного мода).
#[tauri::command]
pub fn download_batch() -> Vec<DownloadRequest> {
    BATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
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
    let queued = {
        let url = Url::parse(&request.page_url)
            .map_err(|_| AppError::new("url", "Некорректный адрес страницы мода."))?;
        if !page_url_allowed(&url) {
            return Err(AppError::new(
                "url",
                "Открывать можно только внешние страницы http и https.",
            ));
        }
        let mut batch = BATCH.lock().unwrap_or_else(|error| error.into_inner());
        add_to_batch(&mut batch, &request)
    };
    if queued && app.get_window(WINDOW_LABEL).is_some() {
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
    // Загрузки прошлых модов идут дальше: окно только открывает новую страницу.
    open_request(app, request).await
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
    *BATCH.lock().unwrap_or_else(|error| error.into_inner()) = requests;
    let _ = app.emit_to(PANEL_LABEL, "download://batch", download_batch());
    open_request(app, first).await
}

/// Следующая (или любая) страница пачки — без сброса очереди.
#[tauri::command]
pub async fn download_batch_goto(app: AppHandle, index: usize) -> Result<(), AppError> {
    let request = BATCH
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get(index)
        .cloned()
        .ok_or_else(|| AppError::new("batch", "Такого мода в очереди нет."))?;
    open_request(app, request).await
}

/// Открытия окна по очереди. Два «Обновить» подряд шли одновременно: второе
/// записывало свой мод в контекст, пока первое ещё создавало окно со своей
/// страницей, — архив первого мода приписывался второму, а само второе открытие
/// падало на «окно уже есть».
static OPEN_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn open_request(app: AppHandle, request: DownloadRequest) -> Result<(), AppError> {
    let _gate = OPEN_GATE.lock().await;
    let url = Url::parse(&request.page_url)
        .map_err(|_| AppError::new("url", "Некорректный адрес страницы мода."))?;
    if !page_url_allowed(&url) {
        return Err(AppError::new(
            "url",
            "Открывать можно только внешние страницы http и https.",
        ));
    }
    *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = Some(request.clone());
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

    if let Some(window) = app.get_window(WINDOW_LABEL) {
        if let Some(site) = app.get_webview(SITE_LABEL) {
            let _ = site.navigate(url);
        }
        let _ = window.show();
        let _ = window.set_focus();
        let _ = app.emit_to(PANEL_LABEL, "download://context", request);
        return Ok(());
    }

    let (window_width, window_height) = window_size(&app);
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
    window.on_window_event(move |event| match event {
        // Alt+F4 при идущей загрузке: сначала спросить в панели, иначе файл
        // качался бы дальше без окна и пропадал при выходе из программы.
        tauri::WindowEvent::CloseRequested { api, .. } if !jobs().downloading().is_empty() => {
            api.prevent_close();
            let _ = close_app.emit_to(PANEL_LABEL, "download://close-asked", ());
        }
        tauri::WindowEvent::Destroyed => {
            clear_session();
            emit_jobs(&close_app);
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
            let _ = window.close();
            return Err(AppError::new("panel", error.to_string()));
        }
    };
    let download_app = app.clone();
    let site_builder = crate::with_browser_args!(WebviewBuilder::new(
        SITE_LABEL,
        WebviewUrl::External(url)
    ))
    .initialization_script(site_helpers())
    // Первый адрес проверен выше; каждый следующий переход и редирект тоже
    // не должен уводить в localhost/частную сеть.
    .on_navigation(|url| url.scheme() == "about" || bmm_sources::net::redirect_allowed(url))
    .on_download(move |_webview, event| {
        match event {
            DownloadEvent::Requested { url, destination } => {
                let name = download_name(&url, destination);
                if !accepts_download(&url, &name) {
                    // Окно — только для архивов модов. Установщик или другой файл
                    // с той же страницы молча лёг бы в «Загрузки»; не качаем его.
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
                // Файл ещё не создан, пока качается: занятые имена — и у идущих загрузок,
                // иначе две загрузки с одним именем писали в один файл.
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
                crate::commands::downloads::note_caught(&path);
                // Причина обрыва прошлой загрузки в этот файл не должна достаться этой.
                let _ = crate::commands::download_progress::take_stop_reason(&path);
                let owner = current_request();
                jobs().start(&path, owner);
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
            DownloadEvent::Finished { path, success, .. } => {
                if success {
                    finished_ok(&download_app, path.as_deref());
                } else {
                    finished_failed(&download_app, path);
                }
            }
            _ => {}
        }
        true
    })
    .on_new_window({
        let app = app.clone();
        move |url, _features| {
            if is_download_url(&url) {
                let same_site = app
                    .get_webview(SITE_LABEL)
                    .and_then(|site| site.url().ok())
                    .and_then(|current| current.host_str().map(str::to_string))
                    .is_some_and(|host| {
                        url.host_str()
                            .is_some_and(|next| next.eq_ignore_ascii_case(&host))
                    });
                let file_host =
                    bmm_domain::hosts::host_matches(&url, bmm_domain::hosts::KNOWN_FILE_HOSTS);
                if (same_site || file_host)
                    && let Some(site) = app.get_webview(SITE_LABEL)
                {
                    let _ = site.navigate(url);
                }
            }
            NewWindowResponse::Deny
        }
    });
    let site_width = (window_width - side_space(window_width)).max(320.0);
    let site_height = (window_height - SITE_VERTICAL_SPACE).max(300.0);
    let site = match window.add_child(
        site_builder,
        LogicalPosition::new(SITE_X, SITE_Y),
        LogicalSize::new(site_width, site_height),
    ) {
        Ok(site) => site,
        Err(error) => {
            let _ = window.close();
            return Err(AppError::new("site", error.to_string()));
        }
    };
    crate::commands::download_progress::attach(&site, app.clone());

    if let Err(error) = window.show() {
        let _ = window.close();
        return Err(AppError::new("window", error.to_string()));
    }
    if let Err(error) = site.set_focus() {
        let _ = window.close();
        return Err(AppError::new("site", error.to_string()));
    }
    let _ = panel;
    Ok(())
}

/// Сколько ждать причину обрыва после сообщения о неудаче.
const FAILURE_GRACE: std::time::Duration = std::time::Duration::from_millis(300);

fn finished_ok(app: &AppHandle, path: Option<&Path>) {
    let Some(path) = path else { return };
    let Some(job) = jobs().finish(path, Ok(())) else {
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

fn finished_failed(app: &AppHandle, path: Option<PathBuf>) {
    let Some(path) = path else {
        let (title, detail, error) = failure_texts(None);
        emit_state(app, 2, title, detail, Some(error));
        return;
    };
    // Итог — сразу: панель не должна ждать, а новая загрузка под тем же именем —
    // получить чужой обрыв.
    let (_, _, placeholder) = failure_texts(None);
    let Some(job) = jobs().finish(&path, Err(placeholder)) else {
        return; // не наша загрузка или окно уже закрыли
    };
    // Недокачанный файл в папке загрузок не нужен.
    let _ = std::fs::remove_file(&path);
    // Причину WebView2 сообщает своим событием, и оно может прийти чуть позже: без
    // неё панель писала бы «сайт не отдал файл» при удалённом из «Загрузок» файле.
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(FAILURE_GRACE);
        let reason = crate::commands::download_progress::take_stop_reason(&path);
        let (title, detail, error) = failure_texts(reason);
        let job = jobs().explain(job.id, error.clone()).unwrap_or(job);
        emit_jobs(&app);
        let current = current_request();
        if for_current_mod(page_of(job.request.as_ref()), page_of(current.as_ref())) {
            emit_state(&app, 2, title, detail, Some(error));
        }
    });
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
    let free = |path: &Path| !path.exists() && !busy.iter().any(|taken| taken == path);
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

    #[test]
    fn window_opens_one_request_at_a_time() {
        // Два «Обновить» подряд шли одновременно, и архив первого мода приписывался
        // второму. Очередь — первым делом, до записи контекста.
        let source = include_str!("download_window.rs");
        let body = &source[source.find("async fn open_request(").unwrap()..];
        let gate = body.find("OPEN_GATE.lock().await").unwrap();
        let context = body.find("*CONTEXT.lock()").unwrap();
        assert!(gate < context, "очередь до записи контекста");
        // Окно загрузки не восстанавливает прошлое место и видимость.
        let lib = include_str!("../lib.rs");
        let denylist = &lib[lib.find(".with_denylist(").unwrap()..];
        let denylist = &denylist[..denylist.find("])").unwrap()];
        assert!(denylist.contains("download_window::WINDOW_LABEL"));
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

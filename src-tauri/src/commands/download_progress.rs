//! Ход загрузки в окне скачивания: сколько скачано, из скольки, с какой скоростью.
//!
//! Tauri сообщает только начало и конец загрузки. Байты по ходу отдаёт сам WebView2
//! (`ICoreWebView2DownloadOperation`), поэтому на Windows подписываемся на него
//! напрямую. Расчёт скорости и частоты сообщений — общий и проверяется тестами.

// Счётчик нужен только на Windows: на других системах окна загрузки WebView2 нет.
#![cfg_attr(not(windows), allow(dead_code))]

use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

/// Не чаще четырёх сообщений в секунду: панели хватает, процессу не тяжело.
const EVERY: Duration = Duration::from_millis(250);
/// Скорость — по последним секундам, а не с начала: иначе после паузы
/// она месяцами «догоняет» реальную.
const WINDOW: Duration = Duration::from_secs(3);
/// Больше этого архив всё равно не установится (тот же предел у установки), а диск
/// закончился бы раньше, чем установка это заметит.
pub(crate) const MAX_DOWNLOAD_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// Столько без единого нового байта — загрузка повисла, держать её незачем.
pub(crate) const STALL_LIMIT: Duration = Duration::from_secs(5 * 60);

/// Почему загрузку оборвали сами: показать в панели вместо «сайт не отдал файл».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StopReason {
    TooBig,
    Stalled,
    /// Файл загрузки пропал или стал недоступен: его удалили, переименовали,
    /// перенесли или заняли другой программой, пока он качался.
    FileGone,
    NoSpace,
    NameTooLong,
    /// Антивирус или политика Windows не пропустили файл.
    Blocked,
    Network,
    Server,
    Crashed,
}

impl StopReason {
    pub fn texts(self) -> (&'static str, &'static str) {
        match self {
            StopReason::TooBig => (
                "Файл слишком большой",
                "Архив больше 8 ГБ — такой мод программа не установит, загрузку остановили.",
            ),
            StopReason::Stalled => (
                "Загрузка зависла",
                "Пять минут не пришло ни байта — загрузку остановили. Попробуйте ещё раз.",
            ),
            StopReason::FileGone => (
                "Файл загрузки пропал",
                "Пока архив качался, его файл удалили, переименовали, перенесли или открыли в другой программе. Скачайте ещё раз и не трогайте файл до конца загрузки.",
            ),
            StopReason::NoSpace => (
                "Не хватило места на диске",
                "Освободите место в папке загрузок и скачайте ещё раз.",
            ),
            StopReason::NameTooLong => (
                "Слишком длинный путь к файлу",
                "Выберите в настройках папку загрузок с путём покороче.",
            ),
            StopReason::Blocked => (
                "Windows не пропустила файл",
                "Антивирус или политика Windows заблокировали архив. Проверьте карантин антивируса.",
            ),
            StopReason::Network => ("Связь оборвалась", "Проверьте интернет и скачайте ещё раз."),
            StopReason::Server => (
                "Сайт оборвал загрузку",
                "Сайт перестал отдавать файл. Попробуйте ещё раз или откройте страницу в браузере.",
            ),
            StopReason::Crashed => (
                "Загрузчик упал",
                "Встроенный браузер оборвал загрузку. Попробуйте ещё раз.",
            ),
        }
    }

    /// Причина обрыва по коду WebView2 (`COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON`).
    /// `None` — оборвали сами (отмена, выход) или причины нет.
    pub fn from_interrupt(code: i32) -> Option<Self> {
        match code {
            1 | 2 | 7 | 10 | 11 => Some(StopReason::FileGone),
            3 | 5 => Some(StopReason::NoSpace),
            4 => Some(StopReason::NameTooLong),
            6 | 8 | 9 => Some(StopReason::Blocked),
            12..=16 => Some(StopReason::Network),
            17..=25 => Some(StopReason::Server),
            29 => Some(StopReason::Crashed),
            // 0 — нет причины; 26–28 — отмена, выход, пауза: это решил человек.
            _ => None,
        }
    }
}

type StopKey = (std::path::PathBuf, Option<u64>);

/// Почему программа сама оборвала загрузку — по файлу и попытке: загрузок может быть
/// несколько сразу, и причина одной не должна достаться другой.
static STOPPED: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<StopKey, StopReason>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Причина, по которой программа сама оборвала эту загрузку (одноразово).
pub(crate) fn take_stop_reason(path: &std::path::Path, job_id: Option<u64>) -> Option<StopReason> {
    STOPPED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&(path.to_path_buf(), job_id))
}

/// Причина от WebView2 — если программа сама уже не назвала свою (зависла, велик):
/// её отмена тоже приходит как обрыв.
#[cfg_attr(not(windows), allow(dead_code))]
fn note_interrupt(path: std::path::PathBuf, job_id: Option<u64>, reason: StopReason) {
    let mut stopped = STOPPED.lock().unwrap_or_else(|error| error.into_inner());
    if stopped.len() >= 64 {
        stopped.clear();
    }
    stopped.entry((path, job_id)).or_insert(reason);
}

#[cfg_attr(not(windows), allow(dead_code))]
fn note_stop(path: std::path::PathBuf, job_id: Option<u64>, reason: StopReason) {
    let mut stopped = STOPPED.lock().unwrap_or_else(|error| error.into_inner());
    if stopped.len() >= 64 {
        stopped.clear();
    }
    stopped.insert((path, job_id), reason);
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadProgress {
    /// Какой файл качается: загрузок бывает несколько сразу, и ход одной не должен
    /// попасть в полоску другой.
    pub path: String,
    /// Скачано байт.
    pub received: u64,
    /// Размер файла, если сайт его сообщил.
    pub total: Option<u64>,
    /// Байт в секунду за последние секунды; `None`, пока считать не из чего.
    pub speed: Option<u64>,
    pub job_id: Option<u64>,
}

/// Считает скорость и решает, пора ли сообщать панели.
pub(crate) struct ProgressMeter {
    samples: Vec<(Instant, u64)>,
    last_sent: Option<Instant>,
    /// Когда байты в последний раз прибавились (или загрузка началась).
    moved_at: Instant,
    most: u64,
}

impl ProgressMeter {
    pub fn new() -> Self {
        Self::started(Instant::now())
    }

    pub fn started(now: Instant) -> Self {
        Self {
            samples: Vec::new(),
            last_sent: None,
            moved_at: now,
            most: 0,
        }
    }

    /// Пора ли обрывать загрузку: файл больше предела (по счётчику или по
    /// объявленному размеру) — сразу, без единого лишнего гигабайта на диске.
    pub fn too_big(received: u64, total: Option<u64>) -> bool {
        received > MAX_DOWNLOAD_BYTES || total.is_some_and(|total| total > MAX_DOWNLOAD_BYTES)
    }

    /// Новых байт не было дольше [`STALL_LIMIT`].
    pub fn stalled(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.moved_at) >= STALL_LIMIT
    }

    /// Новое значение счётчика байт. Возвращает сообщение для панели, если пора;
    /// файл в нём заполняет вызывающий.
    pub fn update(
        &mut self,
        received: u64,
        total: Option<u64>,
        now: Instant,
    ) -> Option<DownloadProgress> {
        if received > self.most {
            self.most = received;
            self.moved_at = now;
        }
        self.samples.push((now, received));
        self.samples
            .retain(|(at, _)| now.saturating_duration_since(*at) <= WINDOW);
        let done = total.is_some_and(|total| total > 0 && received >= total);
        if !done
            && self
                .last_sent
                .is_some_and(|sent| now.saturating_duration_since(sent) < EVERY)
        {
            return None;
        }
        self.last_sent = Some(now);
        Some(DownloadProgress {
            path: String::new(),
            job_id: None,
            received,
            total: total.filter(|total| *total > 0),
            speed: self.speed(),
        })
    }

    fn speed(&self) -> Option<u64> {
        let (first_at, first) = *self.samples.first()?;
        let (last_at, last) = *self.samples.last()?;
        let elapsed = last_at.saturating_duration_since(first_at);
        if elapsed < Duration::from_millis(500) {
            return None;
        }
        let bytes = last.saturating_sub(first) as f64;
        Some((bytes / elapsed.as_secs_f64()).round() as u64)
    }
}

/// Подписка на байты загрузок сайта в окне скачивания: ход для панели, обрыв
/// слишком большого файла и зависшей загрузки.
#[cfg(windows)]
pub(crate) use platform::{attach, cancel_all};

#[cfg(windows)]
mod platform {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use tauri::Manager;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED, COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS,
        COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED, ICoreWebView2_4, ICoreWebView2DownloadOperation,
    };
    use webview2_com::{
        BytesReceivedChangedEventHandler, DownloadStartingEventHandler, StateChangedEventHandler,
    };
    use windows_core::Interface;

    use super::{ProgressMeter, StopReason, note_interrupt, note_stop};
    use crate::commands::download_window::{DownloadRegistration, take_download_registration};

    struct OperationContext {
        session_id: u64,
        nav_id: u64,
        job_id: Option<u64>,
        terminal_handler: bool,
        bytes_handler: bool,
    }

    struct ActiveOperation {
        operation: ICoreWebView2DownloadOperation,
        meter: Arc<Mutex<ProgressMeter>>,
        path: String,
        site_label: String,
        session_id: u64,
        nav_id: u64,
        job_id: Option<u64>,
        terminal_handler: bool,
        bytes_handler: bool,
        last_size: u64,
        moved_at: Instant,
        cancel_requested: bool,
        cancel_reason: Option<StopReason>,
    }

    impl ActiveOperation {
        fn new(
            operation: ICoreWebView2DownloadOperation,
            meter: Arc<Mutex<ProgressMeter>>,
            path: String,
            site_label: String,
            context: OperationContext,
        ) -> Self {
            let last_size = std::fs::metadata(&path).map(|meta| meta.len()).unwrap_or(0);
            Self {
                operation,
                meter,
                path,
                site_label,
                session_id: context.session_id,
                nav_id: context.nav_id,
                job_id: context.job_id,
                terminal_handler: context.terminal_handler,
                bytes_handler: context.bytes_handler,
                last_size,
                moved_at: Instant::now(),
                cancel_requested: false,
                cancel_reason: None,
            }
        }

        fn request_cancel(&mut self, reason: Option<StopReason>) {
            self.cancel_requested = true;
            self.cancel_reason = reason;
        }
    }

    type Active = Vec<ActiveOperation>;
    thread_local! {
        // COM-объекты загрузок живут на потоке webview: сторож и отмена заходят туда же.
        static ACTIVE: RefCell<Active> = const { RefCell::new(Vec::new()) };
    }

    fn read(operation: &ICoreWebView2DownloadOperation) -> Option<(u64, Option<u64>)> {
        let mut received = 0i64;
        let mut total = 0i64;
        // SAFETY: оба вызова пишут одно i64 по переданному указателю.
        unsafe {
            operation.BytesReceived(&mut received).ok()?;
            operation.TotalBytesToReceive(&mut total).ok()?;
        }
        Some((
            u64::try_from(received).unwrap_or(0),
            u64::try_from(total).ok().filter(|total| *total > 0),
        ))
    }

    /// Куда пишется файл: по нему панель отличает одну загрузку от другой.
    fn file_of(operation: &ICoreWebView2DownloadOperation) -> String {
        let mut raw = windows_core::PWSTR::null();
        // SAFETY: строку выделяет WebView2, `take_pwstr` её освобождает.
        unsafe {
            if operation.ResultFilePath(&mut raw).is_err() {
                return String::new();
            }
        }
        webview2_com::take_pwstr(raw)
    }

    fn in_progress(operation: &ICoreWebView2DownloadOperation) -> bool {
        let mut state = Default::default();
        // SAFETY: State пишет одно значение по переданному указателю.
        unsafe {
            operation.State(&mut state).is_ok() && state == COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS
        }
    }

    fn stop(
        operation: &ICoreWebView2DownloadOperation,
        path: &str,
        job_id: Option<u64>,
        reason: Option<StopReason>,
    ) -> bool {
        if let Some(reason) = reason {
            note_stop(PathBuf::from(path), job_id, reason);
        }
        // SAFETY: обычный вызов COM на потоке webview.
        unsafe { operation.Cancel() }.is_ok()
    }

    fn request_cancel(
        path: &str,
        session_id: u64,
        nav_id: u64,
        job_id: Option<u64>,
        reason: Option<StopReason>,
    ) {
        ACTIVE.with(|active| {
            for entry in active.borrow_mut().iter_mut() {
                if entry.path == path
                    && entry.session_id == session_id
                    && entry.nav_id == nav_id
                    && entry.job_id == job_id
                {
                    entry.request_cancel(reason);
                }
            }
        });
    }

    fn clear_site_operations(app: &tauri::AppHandle, label: &str) {
        let (sender, receiver) = mpsc::channel();
        let label = label.to_string();
        let queued = app.run_on_main_thread(move || {
            ACTIVE.with(|active| {
                active
                    .borrow_mut()
                    .retain(|entry| entry.site_label != label);
            });
            let _ = sender.send(());
        });
        if queued.is_ok() {
            let _ = receiver.recv_timeout(Duration::from_secs(1));
        }
    }

    /// Оборвать все идущие загрузки окна: человек сам решил закрыть его или программу.
    pub(crate) fn cancel_all(site: &tauri::Webview) {
        let label = site.label().to_string();
        let _ = site.with_webview(move |_| {
            ACTIVE.with(|active| {
                let mut active = active.borrow_mut();
                let operations = std::mem::take(&mut *active);
                for mut entry in operations {
                    if entry.site_label != label {
                        active.push(entry);
                        continue;
                    }
                    if !in_progress(&entry.operation) {
                        continue;
                    }
                    entry.request_cancel(None);
                    if !stop(&entry.operation, &entry.path, entry.job_id, None) {
                        active.push(entry);
                    }
                }
            });
        });
    }

    pub(crate) fn attach(
        site: &tauri::Webview,
        app: tauri::AppHandle,
        session_id: u64,
        nav_id: u64,
    ) -> bool {
        let attached = Arc::new(AtomicBool::new(false));
        let attached_result = Arc::clone(&attached);
        let watchdog_app = app.clone();
        let site_label = site.label().to_string();
        let watchdog_label = site_label.clone();
        let dispatched = site.with_webview(move |platform| {
            // SAFETY: вызовы COM на потоке webview, куда нас и вызывает Tauri.
            attached_result.store(
                (|| -> windows_core::Result<()> {
                    let webview = unsafe { platform.controller().CoreWebView2()? };
                    let webview: ICoreWebView2_4 = webview.cast()?;
                    let mut token = Default::default();
                    unsafe {
                        webview.add_DownloadStarting(
                            &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                                let Some(args) = args else { return Ok(()) };
                                let operation = args.DownloadOperation()?;
                                let (result_path, job_id) = match take_download_registration(nav_id)
                                {
                                    Some(DownloadRegistration::Accepted(path, job_id)) => {
                                        (path.to_string_lossy().into_owned(), Some(job_id))
                                    }
                                    Some(DownloadRegistration::Rejected) => return Ok(()),
                                    None => (file_of(&operation), None),
                                };
                                let meter = Arc::new(Mutex::new(ProgressMeter::new()));
                                let terminal_path = result_path.clone();
                                let state_app = app.clone();
                                let mut state_token = Default::default();
                                if operation.add_StateChanged(
                                    &StateChangedEventHandler::create(Box::new(
                                        move |operation, _| {
                                            let Some(operation) = operation else {
                                                return Ok(());
                                            };
                                            let mut state = Default::default();
                                            // SAFETY: State и InterruptReason пишут значения по указателям.
                                            if operation.State(&mut state).is_err() {
                                                crate::commands::download_window::finished_failed(
                                                    &state_app,
                                                    PathBuf::from(&terminal_path),
                                                    session_id,
                                                    nav_id,
                                                    job_id,
                                                );
                                                return Ok(());
                                            }
                                            if state == COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED {
                                                crate::commands::download_window::finished_ok(
                                                    &state_app,
                                                    Some(Path::new(&terminal_path)),
                                                    session_id,
                                                    nav_id,
                                                    job_id,
                                                );
                                                return Ok(());
                                            }
                                            if state == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
                                                let path = PathBuf::from(&terminal_path);
                                                let mut reason = Default::default();
                                                let has_reason =
                                                    operation.InterruptReason(&mut reason).is_ok();
                                                if has_reason
                                                    && let Some(reason) =
                                                        StopReason::from_interrupt(reason.0)
                                                {
                                                    note_interrupt(path.clone(), job_id, reason);
                                                }
                                                crate::commands::download_window::finished_failed(
                                                    &state_app, path, session_id, nav_id, job_id,
                                                );
                                            }
                                            Ok(())
                                        },
                                    )),
                                    &mut state_token,
                                )
                                .is_err()
                                {
                                    if let Some(job_id) = job_id {
                                        crate::commands::download_window::disable_native_terminal(
                                            Path::new(&result_path),
                                            session_id,
                                            nav_id,
                                            job_id,
                                        );
                                    }
                                    let cancelled = stop(&operation, &result_path, job_id, None);
                                    if cancelled {
                                        crate::commands::download_window::finished_failed(
                                            &app,
                                            PathBuf::from(&result_path),
                                            session_id,
                                            nav_id,
                                            job_id,
                                        );
                                    } else {
                                        let mut entry = ActiveOperation::new(
                                            operation,
                                            meter,
                                            result_path,
                                            site_label.clone(),
                                            OperationContext {
                                                session_id,
                                                nav_id,
                                                job_id,
                                                terminal_handler: false,
                                                bytes_handler: false,
                                            },
                                        );
                                        entry.request_cancel(None);
                                        ACTIVE.with(|active| active.borrow_mut().push(entry));
                                    }
                                    return Ok(());
                                }
                                if let Some((received, total)) = read(&operation)
                                    && ProgressMeter::too_big(received, total)
                                {
                                    if !stop(
                                        &operation,
                                        &result_path,
                                        job_id,
                                        Some(StopReason::TooBig),
                                    ) {
                                        let mut entry = ActiveOperation::new(
                                            operation.clone(),
                                            Arc::clone(&meter),
                                            result_path.clone(),
                                            site_label.clone(),
                                            OperationContext {
                                                session_id,
                                                nav_id,
                                                job_id,
                                                terminal_handler: true,
                                                bytes_handler: false,
                                            },
                                        );
                                        entry.request_cancel(Some(StopReason::TooBig));
                                        ACTIVE.with(|active| active.borrow_mut().push(entry));
                                    }
                                    return Ok(());
                                }
                                let progress_path = result_path.clone();
                                let bytes_meter = Arc::clone(&meter);
                                let app = app.clone();
                                let mut token = Default::default();
                                let bytes_handler = operation
                                    .add_BytesReceivedChanged(
                                        &BytesReceivedChangedEventHandler::create(Box::new(
                                            move |operation, _| {
                                                let Some(operation) = operation else {
                                                    return Ok(());
                                                };
                                                let Some((received, total)) = read(&operation)
                                                else {
                                                    return Ok(());
                                                };
                                                let path = progress_path.clone();
                                                if ProgressMeter::too_big(received, total) {
                                                    request_cancel(
                                                        &path,
                                                        session_id,
                                                        nav_id,
                                                        job_id,
                                                        Some(StopReason::TooBig),
                                                    );
                                                    let _ = stop(
                                                        &operation,
                                                        &path,
                                                        job_id,
                                                        Some(StopReason::TooBig),
                                                    );
                                                    return Ok(());
                                                }
                                                let message = bytes_meter
                                                    .lock()
                                                    .unwrap_or_else(|error| error.into_inner())
                                                    .update(received, total, Instant::now());
                                                if let Some(message) = message {
                                                    let message = super::DownloadProgress {
                                                        path: progress_path.clone(),
                                                        job_id,
                                                        ..message
                                                    };
                                                    crate::commands::download_window::emit_progress(
                                                        &app, message, session_id, nav_id, job_id,
                                                    );
                                                }
                                                Ok(())
                                            },
                                        )),
                                        &mut token,
                                    )
                                    .is_ok();
                                ACTIVE.with(|active| {
                                    active.borrow_mut().push(ActiveOperation::new(
                                        operation.clone(),
                                        Arc::clone(&meter),
                                        result_path,
                                        site_label.clone(),
                                        OperationContext {
                                            session_id,
                                            nav_id,
                                            job_id,
                                            terminal_handler: true,
                                            bytes_handler,
                                        },
                                    ));
                                });
                                Ok(())
                            })),
                            &mut token,
                        )?;
                    }
                    Ok(())
                })()
                .is_ok(),
                Ordering::Relaxed,
            );
        });
        if dispatched.is_err() || !attached.load(Ordering::Relaxed) {
            return false;
        }

        // Сторож зависших загрузок: раз в полминуты заходит на поток webview и обрывает
        // загрузки, по которым давно не было байт. Окно закрыли — сторож уходит.
        let site = site.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(30));
                if watchdog_app.get_webview(&watchdog_label).is_none() {
                    clear_site_operations(&watchdog_app, &watchdog_label);
                    break;
                }
                let checked_label = watchdog_label.clone();
                let checked_app = watchdog_app.clone();
                let checked = site.with_webview(move |_| {
                    ACTIVE.with(|active| {
                        let now = Instant::now();
                        active.borrow_mut().retain_mut(|entry| {
                            if entry.site_label != checked_label {
                                return true;
                            }
                            if !in_progress(&entry.operation) {
                                if !entry.terminal_handler {
                                    crate::commands::download_window::finished_failed(
                                        &checked_app,
                                        PathBuf::from(&entry.path),
                                        entry.session_id,
                                        entry.nav_id,
                                        entry.job_id,
                                    );
                                }
                                return false;
                            }
                            if !entry.bytes_handler {
                                let size = std::fs::metadata(&entry.path)
                                    .map(|meta| meta.len())
                                    .unwrap_or(entry.last_size);
                                if size != entry.last_size {
                                    entry.last_size = size;
                                    entry.moved_at = now;
                                }
                                if size > super::MAX_DOWNLOAD_BYTES {
                                    entry.request_cancel(Some(StopReason::TooBig));
                                } else if now.saturating_duration_since(entry.moved_at)
                                    >= super::STALL_LIMIT
                                {
                                    entry.request_cancel(Some(StopReason::Stalled));
                                }
                                crate::commands::download_window::emit_progress(
                                    &checked_app,
                                    super::DownloadProgress {
                                        path: entry.path.clone(),
                                        job_id: entry.job_id,
                                        received: size,
                                        total: None,
                                        speed: None,
                                    },
                                    entry.session_id,
                                    entry.nav_id,
                                    entry.job_id,
                                );
                            }
                            if entry.cancel_requested {
                                if stop(
                                    &entry.operation,
                                    &entry.path,
                                    entry.job_id,
                                    entry.cancel_reason,
                                ) {
                                    if !entry.terminal_handler {
                                        crate::commands::download_window::finished_failed(
                                            &checked_app,
                                            PathBuf::from(&entry.path),
                                            entry.session_id,
                                            entry.nav_id,
                                            entry.job_id,
                                        );
                                    }
                                    crate::commands::download_window::cleanup_failed_download(
                                        Path::new(&entry.path),
                                    );
                                    return false;
                                }
                                return true;
                            }
                            let stalled = entry.bytes_handler
                                && entry
                                    .meter
                                    .lock()
                                    .unwrap_or_else(|error| error.into_inner())
                                    .stalled(now);
                            if stalled {
                                entry.request_cancel(Some(StopReason::Stalled));
                                if stop(
                                    &entry.operation,
                                    &entry.path,
                                    entry.job_id,
                                    Some(StopReason::Stalled),
                                ) {
                                    crate::commands::download_window::cleanup_failed_download(
                                        Path::new(&entry.path),
                                    );
                                    return false;
                                }
                            }
                            true
                        });
                    });
                });
                if checked.is_err() {
                    clear_site_operations(&watchdog_app, &watchdog_label);
                    break;
                }
            }
        });
        true
    }
}

#[cfg(not(windows))]
pub(crate) fn attach(
    _site: &tauri::Webview,
    _app: tauri::AppHandle,
    _session_id: u64,
    _nav_id: u64,
) -> bool {
    true
}

#[cfg(not(windows))]
pub(crate) fn cancel_all(_site: &tauri::Webview) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn completed_state_uses_the_captured_path() {
        let source = include_str!("download_progress.rs");
        let attach = source.split("pub(crate) fn attach").nth(1).unwrap();
        let attach = &attach[..attach.find("// Сторож зависших").unwrap()];
        assert!(attach.contains("COREWEBVIEW2_DOWNLOAD_STATE_COMPLETED"));
        assert!(attach.contains("download_window::finished_ok"));
    }

    #[cfg(windows)]
    #[test]
    fn progress_uses_the_original_job_path() {
        let source = include_str!("download_progress.rs");
        let attach = source.split("pub(crate) fn attach").nth(1).unwrap();
        let attach = &attach[..attach.find("// Сторож зависших").unwrap()];
        assert!(attach.contains("path: progress_path.clone()"));
    }

    #[cfg(windows)]
    #[test]
    fn watchdog_cleanup_runs_on_the_main_thread() {
        let source = include_str!("download_progress.rs");
        let cleanup = source.split("fn clear_site_operations").nth(1).unwrap();
        let cleanup = &cleanup[..cleanup.find("/// Оборвать все").unwrap()];
        assert!(cleanup.contains("run_on_main_thread"));
        assert!(cleanup.contains("ACTIVE.with"));
    }

    #[cfg(windows)]
    #[test]
    fn stop_reason_uses_the_saved_operation_path() {
        let source = include_str!("download_progress.rs");
        let stop = source.split("fn stop(").nth(1).unwrap();
        let stop = &stop[..stop.find("fn request_cancel").unwrap()];
        assert!(stop.contains("PathBuf::from(path)"));
        assert!(!stop.contains("file_of(operation)"));
    }

    #[cfg(windows)]
    #[test]
    fn terminal_state_read_failure_finishes_the_captured_path() {
        let source = include_str!("download_progress.rs");
        let attach = source.split("pub(crate) fn attach").nth(1).unwrap();
        let attach = &attach[..attach.find("// Сторож зависших").unwrap()];
        assert!(attach.contains("take_download_registration(nav_id)"));
        assert!(attach.contains("DownloadRegistration::Accepted"));
        assert!(attach.contains("operation.State(&mut state).is_err()"));
        assert!(attach.contains("terminal_path"));
    }

    #[cfg(windows)]
    #[test]
    fn watchdog_stops_when_the_site_is_gone() {
        let source = include_str!("download_progress.rs");
        let watchdog = source.split("// Сторож зависших").nth(1).unwrap();
        assert!(watchdog.contains("get_webview"));
        assert!(watchdog.contains("clear_site_operations"));
        assert!(watchdog.contains("break"));
    }

    #[cfg(windows)]
    #[test]
    fn terminal_handler_failure_finishes_the_exact_operation() {
        let source = include_str!("download_progress.rs");
        let attach = source.split("pub(crate) fn attach").nth(1).unwrap();
        let attach = &attach[..attach.find("// Сторож зависших").unwrap()];
        assert!(attach.contains("operation.add_StateChanged"));
        assert!(attach.contains(".is_err()"));
        assert!(attach.contains("let cancelled = stop(&operation, &result_path, job_id, None);"));
        assert!(attach.contains("finished_failed"));
    }

    #[cfg(windows)]
    #[test]
    fn failed_cancel_is_retried_by_the_watchdog() {
        let source = include_str!("download_progress.rs");
        let stop = source.split("fn stop(").nth(1).unwrap();
        let stop = &stop[..stop.find("/// Оборвать все").unwrap()];
        assert!(stop.contains("-> bool"));
        assert!(stop.contains("Cancel() }.is_ok()"));
        let watchdog = source.split("// Сторож зависших").nth(1).unwrap();
        assert!(watchdog.contains("cancel_requested"));
        assert!(watchdog.contains("StopReason::Stalled"));
        assert!(source.contains("struct ActiveOperation"));
    }

    #[cfg(windows)]
    #[test]
    fn active_operation_is_added_after_the_bytes_handler() {
        let source = include_str!("download_progress.rs");
        let attach = source.split("pub(crate) fn attach").nth(1).unwrap();
        let attach = &attach[..attach.find("// Сторож зависших").unwrap()];
        let normal_push = attach.rfind("ActiveOperation::new(").unwrap();
        let bytes_handler = attach.find("add_BytesReceivedChanged").unwrap();
        assert!(bytes_handler < normal_push);
    }

    #[cfg(windows)]
    #[test]
    fn bytes_cancel_matches_the_operation_identity() {
        let source = include_str!("download_progress.rs");
        let production = source.split("#[cfg(test)]").next().unwrap();
        assert!(production.contains("entry.session_id == session_id"));
        assert!(production.contains("entry.nav_id == nav_id"));
        assert!(production.contains("entry.job_id == job_id"));
    }

    #[cfg(windows)]
    #[test]
    fn bytes_subscription_failure_still_gets_file_monitoring() {
        let source = include_str!("download_progress.rs");
        let attach = source.split("pub(crate) fn attach").nth(1).unwrap();
        assert!(attach.contains("let bytes_handler ="));
        assert!(attach.contains("add_BytesReceivedChanged"));
        let watchdog = source.split("// Сторож зависших").nth(1).unwrap();
        assert!(watchdog.contains("if !entry.bytes_handler"));
        assert!(watchdog.contains("MAX_DOWNLOAD_BYTES"));
        assert!(watchdog.contains("STALL_LIMIT"));
    }

    #[cfg(windows)]
    #[test]
    fn failed_terminal_subscription_keeps_ownership_until_cancel_succeeds() {
        let source = include_str!("download_progress.rs");
        let attach = source.split("pub(crate) fn attach").nth(1).unwrap();
        let attach = &attach[..attach.find("// Сторож зависших").unwrap()];
        let handler = attach.find("add_StateChanged").unwrap();
        let failed = handler + attach[handler..].find(".is_err()").unwrap();
        let end = failed
            + attach[failed..]
                .find("if let Some((received, total))")
                .unwrap();
        let fallback = &attach[failed..end];
        assert!(fallback.contains("if cancelled"));
        assert!(fallback.contains("false,"));
        assert!(fallback.contains("finished_failed"));
    }

    #[cfg(windows)]
    #[test]
    fn oversized_download_is_observed_until_it_is_interrupted() {
        let source = include_str!("download_progress.rs");
        let attach = source.split("pub(crate) fn attach").nth(1).unwrap();
        let attach = &attach[..attach.find("// Сторож зависших").unwrap()];
        let state_handler = attach.find("add_StateChanged").unwrap();
        let size_check = attach.find("ProgressMeter::too_big").unwrap();
        assert!(
            state_handler < size_check,
            "отмена слишком большого файла должна иметь terminal handler"
        );
    }

    fn at(start: Instant, ms: u64) -> Instant {
        start + Duration::from_millis(ms)
    }

    #[test]
    fn first_bytes_are_reported_at_once_then_not_more_than_four_times_a_second() {
        let start = Instant::now();
        let mut meter = ProgressMeter::new();
        let first = meter
            .update(1000, Some(10_000), start)
            .expect("первое сразу");
        assert_eq!(first.received, 1000);
        assert_eq!(first.total, Some(10_000));
        assert_eq!(first.speed, None, "скорость ещё не из чего считать");
        assert!(meter.update(2000, Some(10_000), at(start, 100)).is_none());
        assert!(meter.update(3000, Some(10_000), at(start, 260)).is_some());
    }

    #[test]
    fn speed_is_counted_over_the_last_seconds() {
        let start = Instant::now();
        let mut meter = ProgressMeter::new();
        meter.update(0, None, start);
        // 1 МБ в секунду две секунды подряд.
        let message = meter
            .update(2 * 1024 * 1024, None, at(start, 2000))
            .unwrap();
        assert_eq!(message.speed, Some(1024 * 1024));
        assert_eq!(message.total, None, "сайт не сообщил размер");
        // Потом загрузка стоит 4 секунды: старые замеры уходят, скорость падает до нуля.
        let stalled = meter
            .update(2 * 1024 * 1024, None, at(start, 6000))
            .unwrap();
        assert_eq!(stalled.speed, None, "один свежий замер — скорости нет");
        let still = meter
            .update(2 * 1024 * 1024, None, at(start, 7000))
            .unwrap();
        assert_eq!(still.speed, Some(0));
    }

    #[test]
    fn finished_download_is_reported_even_inside_the_pause() {
        let start = Instant::now();
        let mut meter = ProgressMeter::new();
        meter.update(9000, Some(10_000), start);
        let done = meter
            .update(10_000, Some(10_000), at(start, 50))
            .expect("последние байты не теряются");
        assert_eq!(done.received, 10_000);
    }

    #[test]
    fn zero_total_means_unknown_size() {
        let mut meter = ProgressMeter::new();
        let message = meter.update(10, Some(0), Instant::now()).unwrap();
        assert_eq!(message.total, None);
    }

    #[test]
    fn huge_file_is_stopped_by_counter_or_by_announced_size() {
        assert!(!ProgressMeter::too_big(1024, Some(2048)));
        assert!(ProgressMeter::too_big(MAX_DOWNLOAD_BYTES + 1, None));
        assert!(
            ProgressMeter::too_big(0, Some(MAX_DOWNLOAD_BYTES + 1)),
            "объявленный размер — сразу, без скачанных гигабайт"
        );
        assert!(!ProgressMeter::too_big(
            MAX_DOWNLOAD_BYTES,
            Some(MAX_DOWNLOAD_BYTES)
        ));
    }

    #[test]
    fn download_without_new_bytes_for_five_minutes_is_stalled() {
        let start = Instant::now();
        let mut meter = ProgressMeter::started(start);
        assert!(!meter.stalled(at(start, 60_000)));
        meter.update(100, None, at(start, 60_000));
        // Те же 100 байт снова — это не движение.
        meter.update(100, None, at(start, 200_000));
        assert!(!meter.stalled(at(start, 359_000)));
        assert!(meter.stalled(at(start, 360_000)));
        meter.update(200, None, at(start, 360_000));
        assert!(
            !meter.stalled(at(start, 361_000)),
            "байты пошли — снова живая"
        );
        // Вообще ни одного байта с начала — тоже зависла.
        assert!(ProgressMeter::started(start).stalled(at(start, 300_000)));
    }

    #[test]
    fn webview_interrupt_codes_get_honest_reasons() {
        // Было: что бы ни случилось, панель писала «Сайт не отдал файл» — даже когда
        // файл удалили из «Загрузок» посреди загрузки.
        assert_eq!(StopReason::from_interrupt(1), Some(StopReason::FileGone));
        assert_eq!(StopReason::from_interrupt(2), Some(StopReason::FileGone));
        assert_eq!(StopReason::from_interrupt(3), Some(StopReason::NoSpace));
        assert_eq!(StopReason::from_interrupt(4), Some(StopReason::NameTooLong));
        assert_eq!(StopReason::from_interrupt(6), Some(StopReason::Blocked));
        assert_eq!(StopReason::from_interrupt(14), Some(StopReason::Network));
        assert_eq!(StopReason::from_interrupt(22), Some(StopReason::Server));
        assert_eq!(StopReason::from_interrupt(29), Some(StopReason::Crashed));
        for own in [0, 26, 27, 28, 30, -1] {
            assert_eq!(StopReason::from_interrupt(own), None, "{own}");
        }
        for code in 1..=29 {
            if let Some(reason) = StopReason::from_interrupt(code) {
                let (title, detail) = reason.texts();
                assert!(!title.is_empty() && !detail.is_empty(), "{code}");
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn interrupt_codes_match_webview2() {
        use webview2_com::Microsoft::Web::WebView2::Win32::*;
        assert_eq!(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_FAILED.0, 1);
        assert_eq!(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NO_SPACE.0, 3);
        assert_eq!(
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NAME_TOO_LONG.0,
            4
        );
        assert_eq!(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_MALICIOUS.0, 6);
        assert_eq!(
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_HASH_MISMATCH.0,
            11
        );
        assert_eq!(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_FAILED.0, 12);
        assert_eq!(
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_INVALID_REQUEST.0,
            16
        );
        assert_eq!(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FAILED.0, 17);
        assert_eq!(
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_CROSS_ORIGIN_REDIRECT.0,
            25
        );
        assert_eq!(COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED.0, 26);
        assert_eq!(
            COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_DOWNLOAD_PROCESS_CRASHED.0,
            29
        );
    }

    #[test]
    fn own_reason_is_not_overwritten_by_the_webview() {
        // Программа сама оборвала зависшую загрузку; WebView2 потом сообщает обрыв
        // по-своему — причина остаётся нашей.
        let path = std::path::PathBuf::from("own-reason.zip");
        note_stop(path.clone(), Some(1), StopReason::Stalled);
        note_interrupt(path.clone(), Some(1), StopReason::Network);
        assert_eq!(take_stop_reason(&path, Some(1)), Some(StopReason::Stalled));
        note_interrupt(path.clone(), Some(1), StopReason::FileGone);
        assert_eq!(take_stop_reason(&path, Some(1)), Some(StopReason::FileGone));
    }

    #[test]
    fn stop_reason_is_taken_once_and_only_for_its_file() {
        // Две загрузки сразу: причина обрыва одной не должна достаться другой.
        let first = std::path::PathBuf::from("first-stopped.zip");
        let second = std::path::PathBuf::from("second-running.zip");
        note_stop(first.clone(), Some(1), StopReason::Stalled);
        assert_eq!(take_stop_reason(&second, Some(2)), None);
        assert_eq!(take_stop_reason(&first, Some(1)), Some(StopReason::Stalled));
        assert_eq!(take_stop_reason(&first, Some(1)), None);
    }

    #[test]
    fn stop_reason_is_scoped_to_job_generation() {
        let path = std::path::PathBuf::from("same-path.zip");
        note_stop(path.clone(), Some(1), StopReason::Stalled);
        note_stop(path.clone(), Some(2), StopReason::TooBig);

        assert_eq!(take_stop_reason(&path, Some(1)), Some(StopReason::Stalled));
        assert_eq!(take_stop_reason(&path, Some(2)), Some(StopReason::TooBig));
        assert_eq!(take_stop_reason(&path, None), None);
    }
}

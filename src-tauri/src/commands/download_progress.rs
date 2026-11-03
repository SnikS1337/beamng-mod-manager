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
        }
    }
}

/// Почему программа сама оборвала загрузку — по файлу: загрузок может быть несколько
/// сразу, и причина одной не должна достаться другой.
static STOPPED: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, StopReason>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

/// Причина, по которой программа сама оборвала загрузку этого файла (одноразово).
pub(crate) fn take_stop_reason(path: &std::path::Path) -> Option<StopReason> {
    STOPPED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(path)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn note_stop(path: std::path::PathBuf, reason: StopReason) {
    let mut stopped = STOPPED.lock().unwrap_or_else(|error| error.into_inner());
    if stopped.len() >= 64 {
        stopped.clear();
    }
    stopped.insert(path, reason);
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
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS, ICoreWebView2_4, ICoreWebView2DownloadOperation,
    };
    use webview2_com::{BytesReceivedChangedEventHandler, DownloadStartingEventHandler};
    use windows_core::Interface;

    use super::{ProgressMeter, StopReason, note_stop};

    type Active = Vec<(ICoreWebView2DownloadOperation, Arc<Mutex<ProgressMeter>>)>;
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

    fn stop(operation: &ICoreWebView2DownloadOperation, reason: Option<StopReason>) {
        if let Some(reason) = reason {
            note_stop(PathBuf::from(file_of(operation)), reason);
        }
        // SAFETY: обычный вызов COM на потоке webview.
        let _ = unsafe { operation.Cancel() };
    }

    /// Оборвать все идущие загрузки окна: человек сам решил закрыть его или программу.
    pub(crate) fn cancel_all(site: &tauri::Webview) {
        let _ = site.with_webview(|_| {
            ACTIVE.with(|active| {
                for (operation, _) in active.borrow_mut().drain(..) {
                    if in_progress(&operation) {
                        stop(&operation, None);
                    }
                }
            });
        });
    }

    pub(crate) fn attach(site: &tauri::Webview, app: tauri::AppHandle) {
        let _ = site.with_webview(move |platform| {
            // SAFETY: вызовы COM на потоке webview, куда нас и вызывает Tauri.
            let attached = (|| -> windows_core::Result<()> {
                let webview = unsafe { platform.controller().CoreWebView2()? };
                let webview: ICoreWebView2_4 = webview.cast()?;
                let mut token = Default::default();
                unsafe {
                    webview.add_DownloadStarting(
                        &DownloadStartingEventHandler::create(Box::new(move |_, args| {
                            let Some(args) = args else { return Ok(()) };
                            let operation = args.DownloadOperation()?;
                            let meter = Arc::new(Mutex::new(ProgressMeter::new()));
                            if let Some((received, total)) = read(&operation)
                                && ProgressMeter::too_big(received, total)
                            {
                                stop(&operation, Some(StopReason::TooBig));
                                return Ok(());
                            }
                            ACTIVE.with(|active| {
                                active
                                    .borrow_mut()
                                    .push((operation.clone(), Arc::clone(&meter)))
                            });
                            let app = app.clone();
                            let mut token = Default::default();
                            operation.add_BytesReceivedChanged(
                                &BytesReceivedChangedEventHandler::create(Box::new(
                                    move |operation, _| {
                                        let Some(operation) = operation else {
                                            return Ok(());
                                        };
                                        let Some((received, total)) = read(&operation) else {
                                            return Ok(());
                                        };
                                        if ProgressMeter::too_big(received, total) {
                                            stop(&operation, Some(StopReason::TooBig));
                                            return Ok(());
                                        }
                                        let message = meter
                                            .lock()
                                            .unwrap_or_else(|error| error.into_inner())
                                            .update(received, total, Instant::now());
                                        if let Some(message) = message {
                                            let message = super::DownloadProgress {
                                                path: file_of(&operation),
                                                ..message
                                            };
                                            crate::commands::download_window::emit_progress(
                                                &app, message,
                                            );
                                        }
                                        Ok(())
                                    },
                                )),
                                &mut token,
                            )?;
                            Ok(())
                        })),
                        &mut token,
                    )?;
                }
                Ok(())
            })();
            // Без хода загрузки окно всё равно работает: панель покажет «Файл скачивается».
            let _ = attached;
        });

        // Сторож зависших загрузок: раз в полминуты заходит на поток webview и обрывает
        // загрузки, по которым давно не было байт. Окно закрыли — сторож уходит.
        let site = site.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(30));
                let checked = site.with_webview(|_| {
                    ACTIVE.with(|active| {
                        let now = Instant::now();
                        active.borrow_mut().retain(|(operation, meter)| {
                            if !in_progress(operation) {
                                return false;
                            }
                            let stalled = meter
                                .lock()
                                .unwrap_or_else(|error| error.into_inner())
                                .stalled(now);
                            if stalled {
                                stop(operation, Some(StopReason::Stalled));
                                return false;
                            }
                            true
                        });
                    });
                });
                if checked.is_err() {
                    break;
                }
            }
        });
    }
}

#[cfg(not(windows))]
pub(crate) fn attach(_site: &tauri::Webview, _app: tauri::AppHandle) {}

#[cfg(not(windows))]
pub(crate) fn cancel_all(_site: &tauri::Webview) {}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn stop_reason_is_taken_once_and_only_for_its_file() {
        // Две загрузки сразу: причина обрыва одной не должна достаться другой.
        let first = std::path::PathBuf::from("first-stopped.zip");
        let second = std::path::PathBuf::from("second-running.zip");
        note_stop(first.clone(), StopReason::Stalled);
        assert_eq!(take_stop_reason(&second), None);
        assert_eq!(take_stop_reason(&first), Some(StopReason::Stalled));
        assert_eq!(take_stop_reason(&first), None);
    }
}

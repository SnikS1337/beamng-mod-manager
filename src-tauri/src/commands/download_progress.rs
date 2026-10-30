//! Ход загрузки в окне скачивания: сколько скачано, из скольки, с какой скоростью.
//!
//! Tauri сообщает только начало и конец загрузки. Байты по ходу отдаёт сам WebView2
//! (`ICoreWebView2DownloadOperation`), поэтому на Windows подписываемся на него
//! напрямую. Расчёт скорости и частоты сообщений — общий и проверяется тестами.

use std::time::{Duration, Instant};

use serde::Serialize;
use ts_rs::TS;

/// Не чаще четырёх сообщений в секунду: панели хватает, процессу не тяжело.
const EVERY: Duration = Duration::from_millis(250);
/// Скорость — по последним секундам, а не с начала: иначе после паузы
/// она месяцами «догоняет» реальную.
const WINDOW: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadProgress {
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
}

impl ProgressMeter {
    pub fn new() -> Self {
        Self {
            samples: Vec::new(),
            last_sent: None,
        }
    }

    /// Новое значение счётчика байт. Возвращает сообщение для панели, если пора.
    pub fn update(
        &mut self,
        received: u64,
        total: Option<u64>,
        now: Instant,
    ) -> Option<DownloadProgress> {
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

/// Подписка на байты загрузок сайта в окне скачивания.
#[cfg(windows)]
pub(crate) fn attach(site: &tauri::Webview, app: tauri::AppHandle) {
    use std::sync::{Arc, Mutex};

    use tauri::Emitter;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2_4, ICoreWebView2DownloadOperation,
    };
    use webview2_com::{BytesReceivedChangedEventHandler, DownloadStartingEventHandler};
    use windows_core::Interface;

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
                                    let message = meter
                                        .lock()
                                        .unwrap_or_else(|error| error.into_inner())
                                        .update(received, total, Instant::now());
                                    if let Some(message) = message {
                                        let _ = app.emit_to(
                                            super::download_window::PANEL_LABEL,
                                            "download://progress",
                                            message,
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
}

#[cfg(not(windows))]
pub(crate) fn attach(_site: &tauri::Webview, _app: tauri::AppHandle) {}

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
}

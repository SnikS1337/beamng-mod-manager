//! Проверка обновлений при запуске и по расписанию.

use super::*;

const AUTO_REQUEST_ID: u64 = 0;

pub(crate) fn done_event(
    request_id: u64,
    snapshot: UpdatesSnapshotDto,
    report: &UpdateRunReport,
    names: &HashMap<String, String>,
    phase: Option<&str>,
    error: Option<String>,
) -> UpdatesEventDto {
    UpdatesEventDto {
        followed: 0,
        request_id,
        phase: phase.unwrap_or("done").to_string(),
        progress: None,
        findings: snapshot.findings,
        hidden: snapshot.hidden,
        suggestions: snapshot.suggestions,
        sources: report
            .sources
            .iter()
            .map(|source| source_dto(source, names))
            .collect(),
        new_count: report.new_findings.len(),
        up_to_date: report.up_to_date,
        requests: report.requests,
        cancelled: report.cancelled,
        stopped_by_time: report.stopped_by_time,
        error,
    }
}

pub(crate) fn source_dto(
    report: &SourceRunReport,
    names: &HashMap<String, String>,
) -> UpdateSourceDto {
    UpdateSourceDto {
        id: report.source.clone(),
        display_name: names
            .get(&report.source)
            .cloned()
            .unwrap_or_else(|| report.source.clone()),
        status: match report.status {
            SourceStatus::Ok => "ok",
            SourceStatus::NeedsHumanCheck => "needsHumanCheck",
            SourceStatus::Unavailable => "unavailable",
        }
        .to_string(),
        cards_fetched: report.cards_fetched,
        feed_pages_fetched: report.feed_pages_fetched,
        postponed: report.postponed,
        missing: report.missing_mods.len(),
        unreadable: report.unreadable_mods.len(),
        layout_changed: report.layout_changed,
        note: report.note.clone(),
    }
}

/// Выполняет шаг проверки так, что паника внутри не роняет поток, который его вызвал.
pub(crate) fn contained<T>(step: impl FnOnce() -> T) -> Option<T> {
    std::panic::catch_unwind(AssertUnwindSafe(step)).ok()
}

/// Проверка при запуске и по расписанию. Итог уходит в окно событием `updates://event`
/// с фазой `auto`: значок на «Обновлениях» и тост появляются без открытия раздела.
pub fn spawn_auto_check(app: AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(8));
        let mut started = true;
        let mut last = std::time::Instant::now();
        loop {
            let runtime = state.update.clone();
            let settings = state.settings.load().settings;
            let due = if started {
                settings.auto_check_on_start
            } else if settings.auto_check_every_hours > 0 {
                last.elapsed()
                    >= std::time::Duration::from_secs(settings.auto_check_every_hours as u64 * 3600)
            } else {
                false
            };
            // Проверку самой программы делаем на старте и по тому же расписанию,
            // но не чаще: иначе GitHub дёргался бы каждую минуту.
            if settings.check_app_updates && (started || due) {
                crate::commands::app_update::auto_check(&app, &state);
            }
            started = false;
            if due && let Some(runtime) = runtime {
                {
                    let mut enabled = runtime
                        .enabled
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner());
                    enabled.clear();
                    for source in runtime.sources.iter() {
                        if !settings
                            .disabled_sources
                            .iter()
                            .any(|disabled| disabled.eq_ignore_ascii_case(source.id()))
                        {
                            enabled.insert(source.id().to_string());
                        }
                    }
                }
                // Сбой одной проверки не должен останавливать проверки по расписанию до
                // перезапуска программы: поток живёт, пока открыто окно.
                // Ход проверки при запуске виден в «Обновлениях»: иначе минуты поиска
                // страниц для свежей библиотеки выглядят как «ещё не проверяли».
                let app_for_progress = app.clone();
                let Some(mut report) = contained(|| {
                    let _batch = state.library.batch();
                    runtime.coordinator.check(
                        false,
                        Some(&mut |progress: &UpdateProgress| {
                            let _ = app_for_progress
                                .emit("updates://event", progress_event(AUTO_REQUEST_ID, progress));
                        }),
                    )
                }) else {
                    last = std::time::Instant::now();
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    continue;
                };
                if !report.cancelled
                    && let Some(_guard) = PageBusyGuard::acquire(runtime.page_busy.clone())
                {
                    runtime.page_cancel.store(false, Ordering::SeqCst);
                    let pages = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        run_page_search(&app, &state, AUTO_REQUEST_ID, &runtime.page_cancel, false)
                    }))
                    .unwrap_or_default();
                    if pages.followed > 0
                        && let Some(again) = contained(|| {
                            let _batch = state.library.batch();
                            runtime.coordinator.check(false, None)
                        })
                    {
                        report = merge_reports(report, again);
                    }
                }
                let names: HashMap<String, String> = runtime
                    .sources
                    .iter()
                    .map(|source| (source.id().to_string(), source.display_name().to_string()))
                    .collect();
                // Паника при сборке итога не должна навсегда останавливать расписание.
                if contained(|| {
                    let event = done_event(
                        AUTO_REQUEST_ID,
                        build_snapshot(&state),
                        &report,
                        &names,
                        Some("auto"),
                        None,
                    );
                    let _ = app.emit("updates://event", event);
                })
                .is_none()
                {
                    let _ = contained(|| {
                        let _ = app.emit(
                            "updates://event",
                            UpdatesEventDto {
                                followed: 0,
                                request_id: AUTO_REQUEST_ID,
                                phase: "auto".to_string(),
                                progress: None,
                                findings: Vec::new(),
                                hidden: Vec::new(),
                                suggestions: Vec::new(),
                                sources: Vec::new(),
                                new_count: 0,
                                up_to_date: 0,
                                requests: 0,
                                cancelled: false,
                                stopped_by_time: false,
                                error: Some(
                                    "Не удалось собрать итог фоновой проверки.".to_string(),
                                ),
                            },
                        );
                    });
                }
                last = std::time::Instant::now();
            }
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

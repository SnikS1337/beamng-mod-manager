//! Проверка обновлений, скрытие версий и страницы модов.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use bmm_checks::runner::SourceRunReport;
use bmm_checks::{UpdateCheckService, UpdateFinding, UpdateProgress, UpdateRunReport};
use bmm_domain::library::{LibraryStore, ModBinding};
use bmm_domain::versioning::UpdateStatus;
use bmm_sources::sources::SourceStatus;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFindingDto {
    pub mod_id: String,
    pub mod_name: String,
    pub source: String,
    pub status: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub url: String,
    pub dismissed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSuggestionDto {
    pub mod_id: String,
    pub mod_name: String,
    pub source: String,
    pub source_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSourceDto {
    pub id: String,
    pub display_name: String,
    pub status: String,
    pub cards_fetched: u32,
    pub feed_pages_fetched: u32,
    pub postponed: usize,
    pub missing: usize,
    pub unreadable: usize,
    pub layout_changed: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgressDto {
    pub source: String,
    pub stage: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesSnapshotDto {
    pub findings: Vec<UpdateFindingDto>,
    pub hidden: Vec<UpdateFindingDto>,
    pub suggestions: Vec<PageSuggestionDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesEventDto {
    pub request_id: u64,
    pub phase: String,
    pub progress: Option<UpdateProgressDto>,
    pub findings: Vec<UpdateFindingDto>,
    pub hidden: Vec<UpdateFindingDto>,
    pub suggestions: Vec<PageSuggestionDto>,
    pub sources: Vec<UpdateSourceDto>,
    pub new_count: usize,
    pub up_to_date: usize,
    pub requests: u32,
    pub cancelled: bool,
    pub stopped_by_time: bool,
    pub error: Option<String>,
}

#[tauri::command]
pub async fn updates_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<UpdatesSnapshotDto, AppError> {
    Ok(build_snapshot(&state))
}

fn progress_event(request_id: u64, progress: &UpdateProgress) -> UpdatesEventDto {
    UpdatesEventDto {
        request_id,
        phase: "progress".to_string(),
        progress: Some(progress_dto(progress)),
        findings: Vec::new(),
        hidden: Vec::new(),
        suggestions: Vec::new(),
        sources: Vec::new(),
        new_count: 0,
        up_to_date: 0,
        requests: 0,
        cancelled: false,
        stopped_by_time: false,
        error: None,
    }
}

fn page_search_service(state: &AppState) -> Option<Arc<bmm_sources::search::SearchService>> {
    let runtime = state.search.clone()?;
    let settings = state.settings.load().settings;
    let sources: Vec<Arc<dyn bmm_sources::sources::ModSource>> = runtime
        .service
        .sources()
        .iter()
        .filter(|source| {
            !settings
                .disabled_sources
                .iter()
                .any(|disabled| disabled.eq_ignore_ascii_case(source.id()))
        })
        .cloned()
        .collect();
    let fetcher = runtime.fetcher.clone();
    let options = runtime.service.options().clone();
    let decisions: Arc<dyn bmm_domain::matches::MatchDecisions + Send + Sync> =
        runtime.matches.clone();
    Some(Arc::new(bmm_sources::search::SearchService::new(
        sources,
        Arc::new(move |source| fetcher.status_of(source)),
        options,
        Some(decisions),
    )))
}

struct PageBusyGuard(Arc<AtomicBool>);

impl PageBusyGuard {
    fn acquire(busy: Arc<AtomicBool>) -> Option<Self> {
        if busy.swap(true, Ordering::SeqCst) {
            None
        } else {
            Some(Self(busy))
        }
    }
}

impl Drop for PageBusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Чем искать мод на сайтах: название из его архива (`info.json`), иначе из библиотеки.
fn page_title(state: &AppState, mod_id: &str) -> Option<String> {
    crate::commands::library::archive_title_of(state, mod_id)
        .or_else(|| state.library.get_mod(mod_id).map(|item| item.name))
}

fn run_page_search(
    app: &AppHandle,
    state: &AppState,
    request_id: u64,
    cancel: &AtomicBool,
) -> bmm_checks::PageSearchReport {
    let Some(service) = page_search_service(state) else {
        return bmm_checks::PageSearchReport::default();
    };
    let store = state.library.clone();
    let app_for_progress = app.clone();
    let mut progress = |progress: &UpdateProgress| {
        let _ = app_for_progress.emit("updates://event", progress_event(request_id, progress));
    };
    bmm_checks::find_pages(
        store.as_ref(),
        service.as_ref(),
        Utc::now().fixed_offset(),
        cancel,
        &|mod_id| page_title(state, mod_id),
        &|mod_id| crate::commands::library::archive_author_of(state, mod_id),
        Some(&mut progress),
    )
}

fn run_page_search_quiet(state: &AppState, cancel: &AtomicBool) -> bmm_checks::PageSearchReport {
    let Some(service) = page_search_service(state) else {
        return bmm_checks::PageSearchReport::default();
    };
    let store = state.library.clone();
    bmm_checks::find_pages(
        store.as_ref(),
        service.as_ref(),
        Utc::now().fixed_offset(),
        cancel,
        &|mod_id| page_title(state, mod_id),
        &|mod_id| crate::commands::library::archive_author_of(state, mod_id),
        None,
    )
}

#[tauri::command]
pub async fn updates_start(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    force: bool,
    request_id: u64,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    let runtime = state
        .update
        .clone()
        .ok_or_else(|| AppError::new("updates", "Проверка обновлений недоступна."))?;
    runtime.page_cancel.store(false, Ordering::SeqCst);
    let page_cancel = runtime.page_cancel.clone();
    let page_busy = runtime.page_busy.clone();
    let settings = state.settings.load().settings;
    {
        let mut enabled = runtime.enabled.lock().unwrap();
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
    let coordinator = runtime.coordinator.clone();
    let names: HashMap<String, String> = runtime
        .sources
        .iter()
        .map(|source| (source.id().to_string(), source.display_name().to_string()))
        .collect();
    let state_for_task = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app_for_progress = app.clone();
        // Сотни записей о проверке — одним пакетом: файл библиотеки пишется раз в 20 с и в конце.
        let batch = state_for_task.library.batch();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            coordinator.check(
                force,
                Some(&mut |progress: &UpdateProgress| {
                    let _ = app_for_progress
                        .emit("updates://event", progress_event(request_id, progress));
                }),
            )
        }));
        drop(batch);
        match result {
            Ok(mut report) => {
                // Моды без страницы ищем тем же ходом, как legacy, — но не после отмены.
                if !report.cancelled
                    && let Some(_guard) = PageBusyGuard::acquire(page_busy)
                {
                    // Подбор — подсказка: его сбой не должен съедать итог проверки.
                    let pages = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        run_page_search(&app, &state_for_task, request_id, &page_cancel)
                    }))
                    .unwrap_or_default();
                    // Взятые под наблюдение сами — проверяем сразу, а не при следующем запуске.
                    if pages.followed > 0 {
                        let again = {
                            let _batch = state_for_task.library.batch();
                            coordinator.check(false, None)
                        };
                        report = merge_reports(report, again);
                    }
                }
                let snapshot = build_snapshot(&state_for_task);
                let event = done_event(request_id, snapshot, &report, &names, Some("done"), None);
                let _ = app.emit("updates://event", event);
            }
            Err(_) => {
                let _ = app.emit(
                    "updates://event",
                    UpdatesEventDto {
                        request_id,
                        phase: "error".to_string(),
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
                        error: Some("Проверка не удалась: внутренняя ошибка.".to_string()),
                    },
                );
            }
        }
    });
    Ok(())
}

#[tauri::command]
pub async fn updates_cancel(state: tauri::State<'_, Arc<AppState>>) -> Result<(), AppError> {
    if let Some(runtime) = &state.update {
        runtime.coordinator.cancel();
        runtime.page_cancel.store(true, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
pub async fn updates_dismiss(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    let store = state.library.as_ref();
    let finding = UpdateCheckService::finding_for(store, &mod_id)
        .ok_or_else(|| AppError::new("updates", "Обновление не найдено."))?;
    UpdateCheckService::dismiss(store, &finding);
    Ok(build_snapshot(&state))
}

#[tauri::command]
pub async fn updates_restore(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    UpdateCheckService::restore(state.library.as_ref(), &mod_id);
    Ok(build_snapshot(&state))
}

#[tauri::command]
pub async fn updates_find_pages(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    request_id: u64,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    let runtime = state
        .update
        .clone()
        .ok_or_else(|| AppError::new("pages", "Поиск страниц недоступен."))?;
    let Some(guard) = PageBusyGuard::acquire(runtime.page_busy.clone()) else {
        return Err(AppError::new("pages", "Поиск страниц уже идёт."));
    };
    runtime.page_cancel.store(false, Ordering::SeqCst);
    let cancel = runtime.page_cancel.clone();
    let state_for_task = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let report = std::panic::catch_unwind(AssertUnwindSafe(|| {
            run_page_search(&app, &state_for_task, request_id, &cancel)
        }))
        .unwrap_or_default();
        if report.followed > 0 {
            let _batch = state_for_task.library.batch();
            let _ = runtime.coordinator.check(false, None);
        }
        let snapshot = build_snapshot(&state_for_task);
        let event = UpdatesEventDto {
            request_id,
            phase: "pages".to_string(),
            progress: None,
            findings: snapshot.findings,
            hidden: snapshot.hidden,
            suggestions: snapshot.suggestions,
            sources: Vec::new(),
            new_count: 0,
            up_to_date: 0,
            requests: report.searched as u32,
            cancelled: report.cancelled,
            stopped_by_time: false,
            error: None,
        };
        let _ = app.emit("updates://event", event);
    });
    Ok(())
}

#[tauri::command]
pub async fn updates_confirm_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    bmm_checks::confirm_page(state.library.as_ref(), &mod_id)
        .ok_or_else(|| AppError::new("pages", "Предложение страницы не найдено."))?;
    Ok(build_snapshot(&state))
}

#[tauri::command]
pub async fn updates_reject_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    bmm_checks::reject_page(state.library.as_ref(), &mod_id, Utc::now().fixed_offset())
        .ok_or_else(|| AppError::new("pages", "Предложение страницы не найдено."))?;
    Ok(build_snapshot(&state))
}

#[tauri::command]
pub async fn updates_undo_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
    source: String,
    source_id: String,
    url: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    let url = url
        .parse()
        .map_err(|_| AppError::new("url", "Некорректный адрес страницы."))?;
    let binding = ModBinding {
        mod_id: mod_id.clone(),
        source,
        source_id,
        url,
        confirmed: false,
    };
    bmm_checks::undo_page(state.library.as_ref(), &binding, Utc::now().fixed_offset());
    Ok(build_snapshot(&state))
}

const MODLAND_CHECK_LABEL: &str = "modland-check";
const MODLAND_CHECK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const MODLAND_CHECK_POLL: Duration = Duration::from_millis(400);
const MODLAND_CHECK_CLOSED_MISSES: u32 = 4;
static MODLAND_CHECK_BUSY: AtomicBool = AtomicBool::new(false);

/// Окно проверки одно на приложение: в нём человек проходит Cloudflare, а программа
/// сама замечает, что сайт пустил, и продолжает работу.
pub(crate) async fn open_modland_check(
    app: AppHandle,
    state: Arc<AppState>,
    url: url::Url,
) -> Result<bool, AppError> {
    if MODLAND_CHECK_BUSY.swap(true, Ordering::SeqCst) {
        return Err(AppError::new("check", "Проверка Modland уже идёт."));
    }
    let _busy = BusyGuard;

    let window = modland_window(&app, url.clone())?;
    let token = format!(
        "bmm-check-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|time| time.as_nanos())
            .unwrap_or_default()
    );
    let _ = window.eval(format!(
        "document.documentElement && document.documentElement.setAttribute('data-bmm-check','{token}')"
    ));
    window
        .navigate(url)
        .map_err(|error| AppError::new("window", error.to_string()))?;
    let _ = window.set_focus();

    let worker_app = app.clone();
    let worker_window = window.clone();
    let passed = tauri::async_runtime::spawn_blocking(move || {
        wait_for_check(&worker_app, &worker_window, &token)
    })
    .await
    .map_err(|error| AppError::new("check", error.to_string()))?;

    // Окно могли закрыть сразу после проверки. Тогда true от DOM уже не придёт,
    // поэтому доступ подтверждаем тем же скрытым браузером, которым работает поиск.
    let closed = app.get_webview_window(MODLAND_CHECK_LABEL).is_none();
    let confirmed = if passed || closed {
        modland_access_confirmed(&state).await
    } else {
        false
    };
    if confirmed {
        reset_modland_check(&state);
        if !closed {
            let _ = window.close();
        }
    }
    Ok(confirmed)
}

/// Проверка настоящим запросом программы: видимая страница могла пустить,
/// а скрытый браузер всё ещё видеть заглушку. Тогда поиск запускать рано.
async fn modland_access_confirmed(state: &Arc<AppState>) -> bool {
    let Some(search) = state.search.clone() else {
        return false;
    };
    let Ok(home) = "https://www.modland.net/".parse::<url::Url>() else {
        return false;
    };
    tauri::async_runtime::spawn_blocking(move || {
        search.fetcher.reset_source("modland", &home);
        search.fetcher.get("modland", &home).status == SourceStatus::Ok
    })
    .await
    .unwrap_or(false)
}

struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        MODLAND_CHECK_BUSY.store(false, Ordering::SeqCst);
    }
}

fn modland_window(app: &AppHandle, url: url::Url) -> Result<WebviewWindow, AppError> {
    if let Some(window) = app.get_webview_window(MODLAND_CHECK_LABEL) {
        return Ok(window);
    }
    WebviewWindowBuilder::new(app, MODLAND_CHECK_LABEL, WebviewUrl::External(url))
        .title("Проверка Modland")
        .inner_size(980.0, 720.0)
        .min_inner_size(520.0, 420.0)
        .center()
        .build()
        .map_err(|error| AppError::new("window", error.to_string()))
}

/// Пока окно открыто, смотрим на страницу: обычная страница Modland без маркера
/// старого документа и без cloudflare-заглушки означает, что сайт пустил.
fn wait_for_check(app: &AppHandle, window: &WebviewWindow, token: &str) -> bool {
    let deadline = Instant::now() + MODLAND_CHECK_TIMEOUT;
    let mut misses = 0;
    loop {
        if app.get_webview_window(MODLAND_CHECK_LABEL).is_none() || window.is_visible().is_err() {
            return false;
        }
        if Instant::now() >= deadline {
            return false;
        }
        match crate::browser_pages::document_html(window) {
            Ok(html) => {
                misses = 0;
                if let Ok(url) = window.url()
                    && page_passed(&html, token, &url)
                {
                    return true;
                }
            }
            Err(_) => {
                // Окно закрыли, а Tauri ещё держит его объект: не ждём полного таймаута.
                misses += 1;
                if misses >= MODLAND_CHECK_CLOSED_MISSES {
                    return false;
                }
            }
        }
        std::thread::sleep(MODLAND_CHECK_POLL);
    }
}

fn page_passed(html: &str, token: &str, url: &url::Url) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let modland = host.eq_ignore_ascii_case("modland.net")
        || host.to_ascii_lowercase().ends_with(".modland.net");
    modland
        && html.len() > 200
        && !html.contains(token)
        && !html.contains("id=\"main-frame-error\"")
        && !bmm_sources::net::is_challenge_page(html)
}

fn reset_modland_check(state: &AppState) {
    if let Some(runtime) = &state.update {
        runtime.coordinator.resolve_human_check("modland");
        return;
    }
    if let Some(search) = &state.search
        && let Ok(home) = "https://www.modland.net/".parse()
    {
        search.fetcher.reset_source("modland", &home);
    }
}

#[tauri::command]
pub async fn updates_open_modland(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, AppError> {
    let url: url::Url = "https://www.modland.net/"
        .parse()
        .map_err(|_| AppError::new("url", "Некорректный адрес Modland."))?;
    let state = state.inner().clone();
    open_modland_check(app, state, url).await
}

fn build_snapshot(state: &AppState) -> UpdatesSnapshotDto {
    let store = state.library.as_ref();
    // Как на карточках модов: название из архива, если оно уже прочитано.
    let titles = state.previews.titles();
    let keys: HashMap<String, String> = state
        .previews_by_id
        .lock()
        .unwrap()
        .iter()
        .map(|(id, target)| (id.clone(), target.key.clone()))
        .collect();
    let name_of = |mod_id: &str| -> String {
        keys.get(mod_id)
            .and_then(|key| titles.get(key))
            .filter(|title| !title.is_empty())
            .cloned()
            .or_else(|| store.get_mod(mod_id).map(|item| item.name))
            .unwrap_or_else(|| mod_id.to_string())
    };
    UpdatesSnapshotDto {
        findings: UpdateCheckService::pending(store, false)
            .into_iter()
            .map(|finding| finding_dto(&name_of, finding))
            .collect(),
        hidden: UpdateCheckService::pending(store, true)
            .into_iter()
            .filter(|finding| finding.is_dismissed)
            .map(|finding| finding_dto(&name_of, finding))
            .collect(),
        suggestions: bmm_checks::suggestions(store)
            .into_iter()
            .map(|binding| suggestion_dto(&name_of, binding))
            .collect(),
    }
}

fn finding_dto(name_of: &dyn Fn(&str) -> String, finding: UpdateFinding) -> UpdateFindingDto {
    let name = name_of(&finding.mod_id);
    UpdateFindingDto {
        mod_id: finding.mod_id,
        mod_name: name,
        source: finding.source,
        status: status_text(finding.status).to_string(),
        installed_version: finding.installed_version,
        latest_version: finding.latest_version,
        url: finding.url.to_string(),
        dismissed: finding.is_dismissed,
    }
}

fn suggestion_dto(name_of: &dyn Fn(&str) -> String, binding: ModBinding) -> PageSuggestionDto {
    let name = name_of(&binding.mod_id);
    PageSuggestionDto {
        mod_id: binding.mod_id,
        mod_name: name,
        source: binding.source,
        source_id: binding.source_id,
        url: binding.url.to_string(),
    }
}

fn status_text(status: UpdateStatus) -> &'static str {
    match status {
        UpdateStatus::None => "none",
        UpdateStatus::Likely => "likely",
        UpdateStatus::Possible => "possible",
    }
}

fn progress_dto(progress: &UpdateProgress) -> UpdateProgressDto {
    UpdateProgressDto {
        source: progress.source.clone(),
        stage: progress.stage.clone(),
        done: progress.done,
        total: progress.total,
    }
}

/// Итог проверки и дополнительной проверки модов, которые программа только что
/// взяла под наблюдение сама: новые находки обеих, состояние сайтов — свежее.
fn merge_reports(first: UpdateRunReport, again: UpdateRunReport) -> UpdateRunReport {
    let mut new_findings = first.new_findings;
    for finding in again.new_findings {
        if !new_findings
            .iter()
            .any(|known| known.mod_id == finding.mod_id)
        {
            new_findings.push(finding);
        }
    }
    let mut sources = first.sources;
    for source in again.sources {
        match sources
            .iter_mut()
            .find(|known| known.source == source.source)
        {
            Some(known) => *known = source,
            None => sources.push(source),
        }
    }
    UpdateRunReport {
        findings: again.findings,
        sources,
        requests: first.requests + again.requests,
        up_to_date: first.up_to_date + again.up_to_date,
        new_findings,
        fresh_skipped: first.fresh_skipped,
        cancelled: first.cancelled || again.cancelled,
        stopped_by_time: first.stopped_by_time || again.stopped_by_time,
        reused: false,
    }
}

/// Фоновая проверка не связана ни с одним нажатием в окне.
const AUTO_REQUEST_ID: u64 = 0;

fn done_event(
    request_id: u64,
    snapshot: UpdatesSnapshotDto,
    report: &UpdateRunReport,
    names: &HashMap<String, String>,
    phase: Option<&str>,
    error: Option<String>,
) -> UpdatesEventDto {
    UpdatesEventDto {
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

fn source_dto(report: &SourceRunReport, names: &HashMap<String, String>) -> UpdateSourceDto {
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
            if started && settings.check_app_updates {
                crate::commands::app_update::auto_check(&state);
            }
            let due = if started {
                settings.auto_check_on_start
            } else if settings.auto_check_every_hours > 0 {
                last.elapsed()
                    >= std::time::Duration::from_secs(settings.auto_check_every_hours as u64 * 3600)
            } else {
                false
            };
            started = false;
            if due && let Some(runtime) = runtime {
                {
                    let mut enabled = runtime.enabled.lock().unwrap();
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
                let mut report = {
                    let _batch = state.library.batch();
                    runtime.coordinator.check(false, None)
                };
                if !report.cancelled
                    && let Some(_guard) = PageBusyGuard::acquire(runtime.page_busy.clone())
                {
                    runtime.page_cancel.store(false, Ordering::SeqCst);
                    let pages = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        run_page_search_quiet(&state, &runtime.page_cancel)
                    }))
                    .unwrap_or_default();
                    if pages.followed > 0 {
                        let again = {
                            let _batch = state.library.batch();
                            runtime.coordinator.check(false, None)
                        };
                        report = merge_reports(report, again);
                    }
                }
                let names: HashMap<String, String> = runtime
                    .sources
                    .iter()
                    .map(|source| (source.id().to_string(), source.display_name().to_string()))
                    .collect();
                let event = done_event(
                    AUTO_REQUEST_ID,
                    build_snapshot(&state),
                    &report,
                    &names,
                    Some("auto"),
                    None,
                );
                let _ = app.emit("updates://event", event);
                last = std::time::Instant::now();
            }
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use chrono::TimeZone;

    use bmm_domain::clock::system_clock;
    use bmm_domain::library::{CheckState, InstalledMod, JsonLibraryStore};
    use bmm_domain::paths::AppPaths;
    use bmm_domain::settings::AppSettings;
    use bmm_domain::settings_store::SettingsStore;
    use bmm_packs::cache::PreviewCache;

    fn state() -> (tempfile::TempDir, AppState) {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let paths = AppPaths::at(&data);
        paths.ensure_dirs().unwrap();
        let settings = SettingsStore::with_system_clock(paths.settings());
        settings.save(&AppSettings::default()).unwrap();
        let library = Arc::new(JsonLibraryStore::new(
            data.join("library.json"),
            system_clock(),
        ));
        let state = AppState {
            paths,
            settings,
            library,
            previews: PreviewCache::new(data.join("previews")),
            load_lock: Mutex::new(()),
            previews_by_id: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
            deleted: Mutex::new(HashMap::new()),
            content_cache: Mutex::new(HashMap::new()),
            search: None,
            update: None,
            app_update: None,
            search_cancel: Mutex::new(None),
            last_search: Mutex::new(None),
        };
        (temp, state)
    }

    fn instant() -> chrono::DateTime<chrono::FixedOffset> {
        chrono::Utc
            .timestamp_opt(1_760_000_000, 0)
            .single()
            .unwrap()
            .fixed_offset()
    }

    #[test]
    fn snapshot_finds_dismisses_and_suggests() {
        let (_temp, state) = state();
        let store = state.library.as_ref();
        store.upsert_mod(InstalledMod {
            id: "m1".to_string(),
            name: "BMW M2 G87".to_string(),
            file_name: "bmw.zip".to_string(),
            version: Some("1.0".to_string()),
            size_bytes: Some(100),
            installed_at_utc: instant(),
            enabled: true,
        });
        store
            .upsert_binding(ModBinding {
                mod_id: "m1".to_string(),
                source: "stmods".to_string(),
                source_id: "bmw".to_string(),
                url: "https://stmods.org/beamng_drive/mods/bmw/".parse().unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            mod_id: "m1".to_string(),
            source: "stmods".to_string(),
            last_checked_utc: instant(),
            last_changed_utc: Some(instant()),
            last_version: Some("1.5".to_string()),
            last_size: Some(120),
            miss_count: 0,
            last_game_version: Some("0.39".to_string()),
            last_title: None,
        });
        store.upsert_mod(InstalledMod {
            id: "m2".to_string(),
            name: "Audi RS6".to_string(),
            file_name: "audi.zip".to_string(),
            version: None,
            size_bytes: None,
            installed_at_utc: instant(),
            enabled: true,
        });
        store
            .upsert_binding(ModBinding {
                mod_id: "m2".to_string(),
                source: "stmods".to_string(),
                source_id: "audi".to_string(),
                url: "https://stmods.org/beamng_drive/mods/audi/"
                    .parse()
                    .unwrap(),
                confirmed: false,
            })
            .unwrap();

        let snapshot = build_snapshot(&state);
        assert_eq!(snapshot.findings.len(), 1);
        assert_eq!(snapshot.findings[0].mod_name, "BMW M2 G87");
        assert_eq!(snapshot.findings[0].latest_version.as_deref(), Some("1.5"));
        assert_eq!(snapshot.findings[0].status, "likely");
        assert_eq!(snapshot.suggestions.len(), 1);
        assert_eq!(snapshot.suggestions[0].mod_name, "Audi RS6");

        let finding = UpdateCheckService::finding_for(store, "m1").unwrap();
        UpdateCheckService::dismiss(store, &finding);
        let snapshot = build_snapshot(&state);
        assert!(snapshot.findings.is_empty());
        assert_eq!(snapshot.hidden.len(), 1);
        assert!(snapshot.hidden[0].dismissed);

        UpdateCheckService::restore(store, "m1");
        assert_eq!(build_snapshot(&state).findings.len(), 1);
    }

    #[test]
    fn check_window_detects_a_page_the_site_let_through() {
        let page = format!("<html><body>{}</body></html>", "Toyota Sienna ".repeat(30));
        let url = url::Url::parse("https://www.modland.net/search/sienna").unwrap();
        assert!(page_passed(&page, "bmm-check-2", &url));
        let old = format!("{page} data-bmm-check=\"bmm-check-1\"");
        assert!(!page_passed(&old, "bmm-check-1", &url));
        assert!(!page_passed(
            "<html><head><title>Один момент…</title></head></html>",
            "bmm-check-1",
            &url
        ));
        assert!(!page_passed(
            "<html><body><div id=\"main-frame-error\">ERR_TIMED_OUT</div></body></html>",
            "bmm-check-1",
            &url
        ));
        let other = url::Url::parse("https://stmods.org/beamng_drive/").unwrap();
        assert!(!page_passed(&page, "bmm-check-1", &other));
    }
}

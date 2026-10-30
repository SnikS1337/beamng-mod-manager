//! Проверка обновлений, скрытие версий и страницы модов.

use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use bmm_checks::runner::SourceRunReport;
use bmm_checks::{
    BindingActions, UpdateCheckService, UpdateFinding, UpdateProgress, UpdateRunReport,
};
use bmm_domain::library::{LibraryStore, ModBinding};
use bmm_domain::versioning::UpdateStatus;
use bmm_sources::sources::SourceStatus;

use crate::error::AppError;
use crate::state::AppState;

mod types;
pub use types::*;

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
    retry_recent: bool,
) -> bmm_checks::PageSearchReport {
    let Some(service) = page_search_service(state) else {
        return bmm_checks::PageSearchReport::default();
    };
    // Десятки привязок и отметок поиска — одной записью файла библиотеки.
    let _batch = state.library.batch();
    let store = state.library.clone();
    let app_for_progress = app.clone();
    let mut progress = |progress: &UpdateProgress| {
        let _ = app_for_progress.emit("updates://event", progress_event(request_id, progress));
    };
    let include_disabled = state.settings.load().settings.check_disabled_mods;
    bmm_checks::find_pages_with(
        store.as_ref(),
        service.as_ref(),
        Utc::now().fixed_offset(),
        cancel,
        include_disabled,
        retry_recent,
        &|mod_id| page_title(state, mod_id),
        &|mod_id| crate::commands::library::archive_author_of(state, mod_id),
        Some(&mut progress),
    )
}

#[tauri::command]
pub async fn updates_start(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    force: bool,
    request_id: u64,
) -> Result<(), AppError> {
    start_updates(app, state.inner().clone(), force, request_id).await
}

/// Повтор после сбоя одного сайта: снимаем паузу и запускаем обычную проверку.
/// Свежие моды при этом не трогаются — пойдут только те, кому уже пора.
#[tauri::command]
pub async fn updates_retry_source(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    source: String,
    request_id: u64,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    if let Some(runtime) = &state.update {
        runtime.coordinator.resolve_human_check(&source);
    }
    start_updates(app, state, false, request_id).await
}

async fn start_updates(
    app: AppHandle,
    state: Arc<AppState>,
    force: bool,
    request_id: u64,
) -> Result<(), AppError> {
    let runtime = state
        .update
        .clone()
        .ok_or_else(|| AppError::new("updates", "Проверка обновлений недоступна."))?;
    let page_cancel = runtime.page_cancel.clone();
    let page_busy = runtime.page_busy.clone();
    let settings = state.settings.load().settings;
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
                    // Сбрасываем отмену только когда поиск страниц действительно наш:
                    // иначе «Проверка» снимет отмену чужого поиска.
                    page_cancel.store(false, Ordering::SeqCst);
                    // Подбор — подсказка: его сбой не должен съедать итог проверки.
                    let pages = std::panic::catch_unwind(AssertUnwindSafe(|| {
                        run_page_search(&app, &state_for_task, request_id, &page_cancel, false)
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

/// Проверка одного мода по уже привязанной странице. Не трогает TTL и другие моды:
/// человек нажал «Проверить обновления» именно здесь.
#[tauri::command]
pub async fn updates_check_mod(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || check_mod_once(&state, &mod_id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

fn check_mod_once(state: &AppState, mod_id: &str) -> Result<UpdatesSnapshotDto, AppError> {
    let store = state.library.as_ref();
    let binding = store
        .bindings()
        .into_iter()
        .find(|binding| binding.mod_id == mod_id && binding.confirmed)
        .ok_or_else(|| AppError::new("updates", "У мода нет страницы для проверки обновлений."))?;
    let settings = state.settings.load().settings;
    if settings
        .disabled_sources
        .iter()
        .any(|source| source.eq_ignore_ascii_case(&binding.source))
    {
        return Err(AppError::new(
            "updates",
            "Сайт этого мода выключен в настройках.",
        ));
    }
    let runtime = state
        .update
        .as_ref()
        .ok_or_else(|| AppError::new("updates", "Проверка обновлений недоступна."))?;
    if runtime.coordinator.is_running() {
        return Err(AppError::new(
            "updates",
            "Проверка обновлений уже идёт. Дождитесь её окончания.",
        ));
    }
    let source = runtime
        .sources
        .iter()
        .find(|source| source.id().eq_ignore_ascii_case(&binding.source))
        .ok_or_else(|| AppError::new("updates", "Сайт мода не найден."))?;
    let tracked = UpdateCheckService::tracked(store)
        .into_iter()
        .find(|tracked| tracked.mod_id == mod_id && tracked.source == binding.source)
        .ok_or_else(|| AppError::new("updates", "Мод не готов к проверке."))?;
    let listing = source
        .listing(&binding.url, &AtomicBool::new(false))
        .map_err(|error| AppError::new("updates", error.message))?
        .ok_or_else(|| {
            AppError::new(
                "updates",
                "Страница мода не открылась: возможно, её перенесли или удалили.",
            )
        })?;
    let _batch = state.library.batch();
    UpdateCheckService::apply_card(store, &tracked, &listing, Utc::now().fixed_offset());
    drop(_batch);
    Ok(build_snapshot(state))
}

/// «Не проверять обновления» для одного мода. Возвращает снимок, чтобы окно
/// сразу показало новое состояние; «Отменить» — отдельной командой.
#[tauri::command]
pub async fn updates_decline_mod(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    let state = state.inner().clone();
    if state
        .update
        .as_ref()
        .is_some_and(|runtime| runtime.coordinator.is_running())
    {
        return Err(AppError::new(
            "updates",
            "Проверка обновлений уже идёт. Дождитесь её окончания.",
        ));
    }
    let undo = BindingActions::decline(state.library.as_ref(), &mod_id)
        .ok_or_else(|| AppError::new("updates", "Мод не найден."))?;
    let mut list = declined();
    list.retain(|item| item.mod_id != mod_id);
    list.push(undo);
    let excess = list.len().saturating_sub(16);
    list.drain(..excess);
    Ok(build_snapshot(&state))
}

/// «Отменить» после «Не проверять обновления».
#[tauri::command]
pub async fn updates_undo_decline(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    let state = state.inner().clone();
    if state
        .update
        .as_ref()
        .is_some_and(|runtime| runtime.coordinator.is_running())
    {
        return Err(AppError::new(
            "updates",
            "Проверка обновлений уже идёт. Дождитесь её окончания.",
        ));
    }
    let undo = {
        let mut list = declined();
        let index = list
            .iter()
            .position(|item| item.mod_id == mod_id)
            .ok_or_else(|| AppError::new("updates", "Вернуть уже нечего."))?;
        list.remove(index)
    };
    if !BindingActions::undo(state.library.as_ref(), &undo) {
        return Err(AppError::new(
            "updates",
            "Не удалось вернуть проверку: мод изменился или удалён.",
        ));
    }
    Ok(build_snapshot(&state))
}

/// Снять отказ «Не проверять» (например, после перезапуска, когда отменять
/// конкретную привязку уже нечего).
#[tauri::command]
pub async fn updates_resume_mod(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    let state = state.inner().clone();
    if state
        .update
        .as_ref()
        .is_some_and(|runtime| runtime.coordinator.is_running())
    {
        return Err(AppError::new(
            "updates",
            "Проверка обновлений уже идёт. Дождитесь её окончания.",
        ));
    }
    {
        let mut list = declined();
        if let Some(index) = list.iter().position(|item| item.mod_id == mod_id) {
            let undo = list.remove(index);
            if BindingActions::undo(state.library.as_ref(), &undo) {
                return Ok(build_snapshot(&state));
            }
        }
    }
    BindingActions::resume_checking(state.library.as_ref(), &mod_id);
    Ok(build_snapshot(&state))
}

static DECLINED: std::sync::Mutex<Vec<bmm_checks::DeclineUndo>> = std::sync::Mutex::new(Vec::new());

fn declined() -> std::sync::MutexGuard<'static, Vec<bmm_checks::DeclineUndo>> {
    DECLINED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
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

mod pages;
pub use pages::*;

const MODLAND_CHECK_LABEL: &str = "modland-check";
const MODLAND_CHECK_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const MODLAND_CHECK_POLL: Duration = Duration::from_millis(400);
/// Сколько подряд не читать DOM, прежде чем считать, что окно закрыли.
/// Перезагрузка и Cloudflare на секунды делают документ недоступным — это не закрытие.
const MODLAND_CHECK_CLOSED_MISSES: u32 = 25;
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
    let reload = same_page(window.url().ok().as_ref(), &url);
    let _ = window.eval(format!(
        "document.documentElement && document.documentElement.setAttribute('data-bmm-check','{token}')"
    ));
    // Окно создаётся сразу на нужном адресе. Navigate на тот же адрес не перезагружает
    // документ, и метка старого документа осталась бы навсегда — проверка не увидит успех.
    if reload {
        let _ = window.eval("location.reload()");
    } else {
        window
            .navigate(url)
            .map_err(|error| AppError::new("window", error.to_string()))?;
    }
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
        .on_navigation(|url| url.scheme() == "about" || bmm_sources::net::redirect_allowed(url))
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

fn same_page(current: Option<&url::Url>, target: &url::Url) -> bool {
    current.is_some_and(|current| current == target)
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

mod snapshot;
pub(crate) use snapshot::*;

mod auto;
pub(crate) use auto::done_event;
pub use auto::spawn_auto_check;

#[cfg(test)]
mod tests {
    use super::auto::{contained, source_dto};
    use super::pages::{forget_lost, refollow, unfollow};
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
            browse: Mutex::new(None),
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
    fn bind_error_codes_match_window_union() {
        use crate::test_support::ts_union;
        let codes = ts_union("unions.ts", "BindErrorCode");
        for code in [
            bmm_checks::pages::BindErrorCode::InvalidUrl,
            bmm_checks::pages::BindErrorCode::UnknownSite,
            bmm_checks::pages::BindErrorCode::NotModPage,
            bmm_checks::pages::BindErrorCode::ModMissing,
            bmm_checks::pages::BindErrorCode::PageMissing,
            bmm_checks::pages::BindErrorCode::HumanCheck,
            bmm_checks::pages::BindErrorCode::PageOpen,
        ] {
            assert!(codes.contains(code.as_str()), "окно не знает код {code:?}");
        }
    }

    #[test]
    fn updates_window_and_rust_agree_on_values() {
        use crate::test_support::ts_union;
        let statuses = ts_union("unions.ts", "UpdateStatusText");
        for status in [
            UpdateStatus::None,
            UpdateStatus::Likely,
            UpdateStatus::Possible,
        ] {
            assert!(statuses.contains(status_text(status)));
        }
        let phases = ts_union("unions.ts", "UpdatesPhase");
        for phase in ["progress", "done", "pages", "error", "auto"] {
            assert!(phases.contains(phase), "окно не знает этап {phase}");
        }
        let sites = ts_union("unions.ts", "UpdateSiteStatus");
        for status in ["ok", "human_check", "unavailable", "off"] {
            assert!(
                sites.contains(status),
                "окно не знает состояние сайта {status}"
            );
        }
        let sources = ts_union("unions.ts", "UpdateSourceStatus");
        let names = HashMap::from([("stmods".to_string(), "STMods".to_string())]);
        for status in [
            SourceStatus::Ok,
            SourceStatus::NeedsHumanCheck,
            SourceStatus::Unavailable,
        ] {
            let report = SourceRunReport {
                source: "stmods".to_string(),
                status,
                feed_pages_fetched: 1,
                cards_fetched: 2,
                postponed: 0,
                missing_mods: Vec::new(),
                unreadable_mods: Vec::new(),
                layout_changed: false,
                note: None,
            };
            assert!(sources.contains(&source_dto(&report, &names).status));
        }
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

    #[test]
    fn same_page_is_reloaded_instead_of_navigated() {
        let target = url::Url::parse("https://www.modland.net/").unwrap();
        assert!(same_page(Some(&target), &target));
        assert!(!same_page(None, &target));
        assert!(!same_page(
            Some(&url::Url::parse("https://www.modland.net/search").unwrap()),
            &target
        ));
    }

    #[test]
    fn contained_step_survives_panic() {
        assert_eq!(contained(|| 5), Some(5));
        let failed: Option<i32> = contained(|| panic!("сбой проверки"));
        assert_eq!(failed, None);
        // Поток продолжает работать после сбоя.
        assert_eq!(contained(|| "дальше"), Some("дальше"));
    }

    #[test]
    fn unfollow_and_undo_from_the_window() {
        let (_temp, state) = state();
        let mod_id = "unfollow-test-mod";
        state.library.upsert_mod(InstalledMod {
            id: mod_id.to_string(),
            name: "Mercedes-Benz ML".to_string(),
            file_name: "mb.zip".to_string(),
            version: None,
            size_bytes: Some(455_163_953),
            installed_at_utc: Utc::now().fixed_offset(),
            enabled: true,
        });
        state
            .library
            .upsert_binding(ModBinding {
                mod_id: mod_id.to_string(),
                source: "modsgaming".to_string(),
                source_id: "132573".to_string(),
                url: "https://modsgaming.ru/load/x/38-1-0-132573"
                    .parse()
                    .unwrap(),
                confirmed: true,
            })
            .unwrap();
        state.library.save_check(CheckState {
            mod_id: mod_id.to_string(),
            source: "modsgaming".to_string(),
            last_checked_utc: Utc::now().fixed_offset(),
            last_changed_utc: None,
            last_version: None,
            last_size: Some(347_110_113),
            miss_count: 0,
            last_game_version: None,
            last_title: None,
        });
        assert_eq!(build_snapshot(&state).findings.len(), 1);

        unfollow(&state, mod_id, "modsgaming").unwrap();
        assert!(build_snapshot(&state).findings.is_empty());
        assert!(
            unfollow(&state, mod_id, "modsgaming").is_err(),
            "уже не следим"
        );

        refollow(&state, mod_id, "modsgaming").unwrap();
        assert_eq!(build_snapshot(&state).findings.len(), 1);
        assert!(
            refollow(&state, mod_id, "modsgaming").is_err(),
            "вернуть второй раз нечего"
        );
    }

    fn site_report(source: &str, cards: u32, missing: &[&str]) -> SourceRunReport {
        SourceRunReport {
            source: source.to_string(),
            status: bmm_sources::sources::SourceStatus::Ok,
            feed_pages_fetched: 1,
            cards_fetched: cards,
            postponed: 0,
            missing_mods: missing.iter().map(|id| id.to_string()).collect(),
            unreadable_mods: Vec::new(),
            layout_changed: false,
            note: None,
        }
    }

    fn run_report(sources: Vec<SourceRunReport>, requests: u32) -> UpdateRunReport {
        UpdateRunReport {
            findings: Vec::new(),
            sources,
            requests,
            up_to_date: 2,
            new_findings: Vec::new(),
            fresh_skipped: 5,
            cancelled: false,
            stopped_by_time: false,
            reused: false,
        }
    }

    #[test]
    fn second_pass_adds_to_the_site_report_instead_of_replacing_it() {
        let mut broken = site_report("stmods", 0, &["laurel"]);
        broken.layout_changed = true;
        let first = run_report(vec![broken, site_report("modsgaming", 3, &[])], 10);
        let mut again_stmods = site_report("stmods", 2, &["mb"]);
        again_stmods.status = bmm_sources::sources::SourceStatus::Unavailable;
        again_stmods.note = Some("Сайт не ответил.".to_string());
        let again = run_report(vec![again_stmods, site_report("modland", 1, &[])], 4);

        let merged = merge_reports(first, again);
        assert_eq!(merged.requests, 14);
        assert_eq!(merged.up_to_date, 4);
        assert_eq!(merged.fresh_skipped, 5);
        let stmods = merged
            .sources
            .iter()
            .find(|item| item.source == "stmods")
            .unwrap();
        assert_eq!(stmods.cards_fetched, 2);
        assert_eq!(stmods.feed_pages_fetched, 2);
        assert_eq!(stmods.missing_mods, vec!["laurel", "mb"]);
        assert!(stmods.layout_changed, "сбой первого хода не потерялся");
        assert_eq!(
            stmods.status,
            bmm_sources::sources::SourceStatus::Unavailable
        );
        assert_eq!(stmods.note.as_deref(), Some("Сайт не ответил."));
        assert_eq!(merged.sources.len(), 3);
        let modsgaming = merged
            .sources
            .iter()
            .find(|item| item.source == "modsgaming")
            .unwrap();
        assert_eq!(modsgaming.cards_fetched, 3);
    }

    #[test]
    fn lost_page_is_listed_and_can_be_searched_again() {
        let (_temp, state) = state();
        let store = state.library.as_ref();
        store.upsert_mod(InstalledMod {
            id: "m1".to_string(),
            name: "Old Car".to_string(),
            file_name: "old.zip".to_string(),
            version: Some("1.0".to_string()),
            size_bytes: Some(100),
            installed_at_utc: instant(),
            enabled: true,
        });
        store
            .upsert_binding(ModBinding {
                mod_id: "m1".to_string(),
                source: "stmods".to_string(),
                source_id: "old".to_string(),
                url: "https://stmods.org/beamng_drive/mods/old/".parse().unwrap(),
                confirmed: true,
            })
            .unwrap();
        let mut check = CheckState {
            mod_id: "m1".to_string(),
            source: "stmods".to_string(),
            last_checked_utc: instant(),
            last_changed_utc: None,
            last_version: Some("1.0".to_string()),
            last_size: None,
            miss_count: 1,
            last_game_version: None,
            last_title: None,
        };
        store.save_check(check.clone());
        assert!(
            build_snapshot(&state).lost.is_empty(),
            "один промах — ещё не пропажа"
        );

        check.miss_count = 2;
        store.save_check(check);
        let lost = build_snapshot(&state).lost;
        assert_eq!(lost.len(), 1);
        assert_eq!(lost[0].mod_name, "Old Car");
        assert_eq!(lost[0].url, "https://stmods.org/beamng_drive/mods/old/");

        forget_lost(&state, "m1", "stmods", true).unwrap();
        assert!(build_snapshot(&state).lost.is_empty());
        assert!(store.bindings().is_empty());
        assert!(
            store.get_page_search("m1").is_none(),
            "новую страницу можно искать сразу"
        );
        assert!(forget_lost(&state, "m1", "stmods", false).is_err());
    }

    #[test]
    fn addon_preview_is_hidden() {
        let preview = || Some("http://bmm.localhost/preview/cup.png".to_string());
        assert_eq!(preview_for_role(Some("addon"), preview()), None);
        assert_eq!(preview_for_role(Some("vehicle"), preview()), preview());
        assert_eq!(preview_for_role(Some("map"), preview()), preview());
        assert_eq!(
            preview_for_role(None, preview()),
            preview(),
            "пока роль неизвестна, картинку не выключаем"
        );
    }

    #[test]
    fn snapshot_reports_sites_and_last_check() {
        let (_temp, state) = state();
        let snapshot = build_snapshot(&state);
        assert!(snapshot.sites.is_empty(), "без сайтов в тестовом окружении");
        assert_eq!(snapshot.last_checked, None);

        // Все моды отложены — записей о проверке нет, но время нажатия видно.
        let at = chrono::DateTime::parse_from_rfc3339("2026-10-24T10:00:00+00:00").unwrap();
        state.library.set_last_run(at);
        assert_eq!(
            build_snapshot(&state).last_checked.as_deref(),
            Some("2026-10-24T10:00:00+00:00")
        );
    }
}

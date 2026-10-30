//! Поиск по сайтам: поток событий, отмена и ручные решения о дублях.

use std::collections::{BTreeMap, HashSet};
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;
use url::Url;

use bmm_domain::filename;
use bmm_domain::matches::{ManualMatch, MatchDecisions};
use bmm_domain::models::{Listing, Offer, PriceStatus};
use bmm_domain::text::{ModKind, TitleNormalizer, TitleParser};

use bmm_sources::search::{ModCard, PriceSummary, SearchResponse, SearchService, SourceReport};
use bmm_sources::sources::{ModSource, SourceStatus};

use crate::error::AppError;
use crate::state::{AppState, BrowseSession, SearchRuntime};

const BROWSE_PAGES: u32 = 2;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SearchOffer")]
pub struct OfferDto {
    #[ts(type = "SearchOfferKind")]
    pub kind: String,
    pub host: String,
    pub url: String,
    pub file_id: Option<String>,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SearchListing")]
pub struct SearchListingDto {
    pub source: String,
    pub source_id: String,
    pub url: String,
    pub title: String,
    /// Название без рубрики сайта и версии («автомобиль LADA 2106 v1.0» → «LADA 2106»).
    pub name: String,
    pub mod_version: Option<String>,
    pub game_version: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub size_bytes: Option<i64>,
    pub preview_url: Option<String>,
    #[ts(type = "SearchPrice")]
    pub price: String,
    pub offers: Vec<OfferDto>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub external_id: Option<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SearchCard")]
pub struct SearchCardDto {
    pub id: String,
    pub title: String,
    #[ts(type = "SearchPriceSummary")]
    pub price: String,
    pub possible_same_as: Vec<String>,
    pub relevance: f64,
    pub installed: bool,
    /// «vehicle», «map», «addon» или нет, если сайт не сказал.
    #[ts(type = "ModRole | null")]
    pub kind: Option<String>,
    pub listings: Vec<SearchListingDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SearchSource")]
pub struct SearchSourceDto {
    pub id: String,
    pub display_name: String,
    #[ts(type = "SearchSourceStatus")]
    pub status: String,
    pub count: usize,
    pub note: Option<String>,
    pub pending: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "SearchEvent")]
pub struct SearchEventDto {
    pub request_id: u64,
    #[ts(type = "SearchPhase")]
    pub phase: String,
    pub cards: Vec<SearchCardDto>,
    pub sources: Vec<SearchSourceDto>,
    pub error: Option<String>,
}

struct InstalledIndex {
    keys: HashSet<String>,
    urls: HashSet<String>,
    names: HashSet<String>,
}

impl InstalledIndex {
    fn build(state: &AppState) -> Self {
        Self::from_store(state.library.as_ref())
    }

    /// «Установлен» в поиске: страница, за которой программа следит, или то же название,
    /// что у установленного мода без страницы. Предложенная, но не подтверждённая страница
    /// ещё не «его»; а у мода со своей страницей одноимённые чужие моды — не он.
    fn from_store(store: &dyn bmm_domain::library::LibraryStore) -> Self {
        let mut keys = HashSet::new();
        let mut urls = HashSet::new();
        let mut followed = HashSet::new();
        for binding in store
            .bindings()
            .into_iter()
            .filter(|binding| binding.confirmed)
        {
            keys.insert(format!(
                "{}:{}",
                binding.source.trim().to_lowercase(),
                binding.source_id.trim()
            ));
            urls.insert(binding.url.as_str().trim_end_matches('/').to_string());
            followed.insert(binding.mod_id);
        }
        let mut names = HashSet::new();
        for installed in store.mods() {
            if followed.contains(&installed.id) {
                continue;
            }
            for name in [
                installed.name.as_str(),
                filename::from_file_name(&installed.file_name).name.as_str(),
            ] {
                let normalized = TitleNormalizer::normalize(name);
                if !normalized.is_empty() {
                    names.insert(normalized);
                }
            }
        }
        Self { keys, urls, names }
    }

    fn contains(&self, listing: &Listing) -> bool {
        let key = format!(
            "{}:{}",
            listing.source.trim().to_lowercase(),
            listing.source_id.trim()
        );
        if self.keys.contains(&key)
            || self
                .urls
                .contains(listing.url.as_str().trim_end_matches('/'))
        {
            return true;
        }
        let title = TitleNormalizer::normalize(&TitleParser::parse(&listing.title).name);
        !title.is_empty() && self.names.contains(&title)
    }
}

fn searchable_sources(runtime: &SearchRuntime, disabled: &[String]) -> Vec<Arc<dyn ModSource>> {
    runtime
        .service
        .sources()
        .iter()
        .filter(|source| {
            source.supports_search()
                && !disabled
                    .iter()
                    .any(|item| item.eq_ignore_ascii_case(source.id()))
        })
        .cloned()
        .collect()
}

/// Пользователь начал новый поиск — даём сайтам с челленджем ещё один шанс,
/// не дожидаясь десятиминутной паузы.
fn reset_challenged(fetcher: &bmm_sources::net::PoliteFetcher, sources: &[Arc<dyn ModSource>]) {
    for source in sources {
        if fetcher.status_of(source.id()) == SourceStatus::NeedsHumanCheck {
            fetcher.reset_source(source.id(), &source.home());
        }
    }
}

/// Собрать общую выдачу раздела из накопленных страниц.
fn assemble_browse(service: &SearchService, session: &BrowseSession) -> SearchResponse {
    let mut listings = Vec::new();
    let mut reports = Vec::new();
    for id in &session.order {
        let items = session.per_source.get(id).cloned().unwrap_or_default();
        if let Some(base) = session.reports.get(id) {
            reports.push(SourceReport {
                count: items.len(),
                ..base.clone()
            });
        }
        listings.extend(items);
    }
    SearchResponse {
        cards: service.group_site_order(&listings),
        sources: reports,
        listings,
    }
}

#[tauri::command]
pub async fn search_start(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    query: String,
    request_id: u64,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    let runtime = state
        .search
        .clone()
        .ok_or_else(|| AppError::new("search", "Поиск недоступен."))?;
    let settings = state.settings.load().settings;
    let sources = searchable_sources(&runtime, &settings.disabled_sources);
    let fetcher = runtime.fetcher.clone();
    reset_challenged(&fetcher, &sources);
    let options = runtime.service.options().clone();
    let decisions: Arc<dyn MatchDecisions + Send + Sync> = runtime.matches.clone();
    let service = SearchService::new(
        sources,
        Arc::new(move |source| fetcher.status_of(source)),
        options,
        Some(decisions),
    );
    let installed = Arc::new(InstalledIndex::build(&state));
    // Новый текстовый поиск закрывает раздел: старые страницы больше не нужны.
    *state.browse.lock().unwrap() = None;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = state.search_cancel.lock().unwrap();
        if let Some((_, previous)) = slot.take() {
            previous.store(true, Ordering::Relaxed);
        }
        *slot = Some((request_id, cancel.clone()));
    }
    let state_for_task = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let app_for_progress = app.clone();
        let installed_for_progress = installed.clone();
        let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
            let mut emit = |response: &SearchResponse| {
                let event = to_event(
                    response,
                    request_id,
                    "partial",
                    &installed_for_progress,
                    None,
                );
                let _ = app_for_progress.emit("search://event", event);
            };
            service.search(&query, &cancel, Some(&mut emit))
        }));
        match result {
            Ok(response) => {
                if !cancel.load(Ordering::Relaxed) {
                    *state_for_task.last_search.lock().unwrap() = Some(response.clone());
                    let event = to_event(&response, request_id, "done", &installed, None);
                    let _ = app.emit("search://event", event);
                }
            }
            Err(_) => {
                let event = SearchEventDto {
                    request_id,
                    phase: "error".to_string(),
                    cards: Vec::new(),
                    sources: Vec::new(),
                    error: Some("Поиск не удался: внутренняя ошибка.".to_string()),
                };
                let _ = app.emit("search://event", event);
            }
        }
        let mut slot = state_for_task.search_cancel.lock().unwrap();
        if slot
            .as_ref()
            .is_some_and(|(current, _)| *current == request_id)
        {
            *slot = None;
        }
    });
    Ok(())
}

/// Открыть раздел сайтов: карты, машины или дополнения. `from_page` — с какой
/// страницы продолжать; первые две страницы каждой порции.
#[tauri::command]
pub async fn search_browse(
    state: tauri::State<'_, Arc<AppState>>,
    kind: String,
    from_page: u32,
    request_id: u64,
) -> Result<SearchEventDto, AppError> {
    let state = state.inner().clone();
    let kind = match kind.as_str() {
        "vehicle" => ModKind::Vehicle,
        "map" => ModKind::Map,
        "addon" => ModKind::Addon,
        _ => return Err(AppError::new("search", "Неизвестный раздел.")),
    };
    let runtime = state
        .search
        .clone()
        .ok_or_else(|| AppError::new("search", "Поиск недоступен."))?;
    let settings = state.settings.load().settings;
    let sources = searchable_sources(&runtime, &settings.disabled_sources);
    if sources.is_empty() {
        return Err(AppError::new("search", "Все сайты выключены."));
    }
    if from_page <= 1 {
        reset_challenged(&runtime.fetcher, &sources);
    }
    let status_source = runtime.fetcher.clone();
    let service = SearchService::new(
        sources,
        Arc::new(move |source| status_source.status_of(source)),
        runtime.service.options().clone(),
        Some(runtime.matches.clone()),
    );
    let installed = InstalledIndex::build(&state);
    let from_page = from_page.max(1);
    if from_page > 1 {
        let session = state.browse.lock().unwrap();
        if !session.as_ref().is_some_and(|session| session.kind == kind) {
            return Err(AppError::new(
                "search",
                "Раздел уже закрыт. Откройте его заново.",
            ));
        }
    }
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = state.search_cancel.lock().unwrap();
        if slot.is_some() {
            return Err(AppError::new(
                "search",
                "Поиск уже идёт. Дождитесь его окончания.",
            ));
        }
        *slot = Some((request_id, Arc::clone(&cancel)));
    }
    let state_for_task = state.clone();
    let run = tauri::async_runtime::spawn_blocking(move || {
        service.browse(kind, from_page, BROWSE_PAGES, &cancel, None)
    })
    .await;
    let superseded = {
        let mut slot = state.search_cancel.lock().unwrap();
        let ours = slot
            .as_ref()
            .is_some_and(|(current, _)| *current == request_id);
        if ours {
            *slot = None;
        }
        !ours
    };
    let response = run.map_err(|error| AppError::new("task", error.to_string()))?;
    if superseded {
        return Err(AppError::new(
            "search",
            "Раздел отменён: уже идёт другой поиск.",
        ));
    }

    let mut session_lock = state_for_task.browse.lock().unwrap();
    let mut session = if from_page <= 1 {
        BrowseSession {
            kind,
            pages: 0,
            order: Vec::new(),
            per_source: BTreeMap::new(),
            reports: BTreeMap::new(),
            response: SearchResponse {
                cards: Vec::new(),
                sources: Vec::new(),
                listings: Vec::new(),
            },
        }
    } else {
        session_lock
            .take()
            .ok_or_else(|| AppError::new("search", "Раздел уже закрыт. Откройте его заново."))?
    };
    session.order = response
        .sources
        .iter()
        .map(|report| report.source_id.clone())
        .collect();
    for report in response.sources {
        session.reports.insert(report.source_id.clone(), report);
    }
    for item in response.listings {
        let target = session.per_source.entry(item.source.clone()).or_default();
        if !target
            .iter()
            .any(|existing| existing.source_id == item.source_id)
        {
            target.push(item);
        }
    }
    let built = assemble_browse(&runtime.service, &session);
    session.pages = from_page.saturating_add(BROWSE_PAGES - 1);
    session.response = built.clone();
    *state_for_task.last_search.lock().unwrap() = Some(built.clone());
    *session_lock = Some(session);
    Ok(to_event(&built, request_id, "done", &installed, None))
}

/// Повторить только один сайт после сбоя, не трогая остальную выдачу.
#[tauri::command]
pub async fn search_retry_source(
    state: tauri::State<'_, Arc<AppState>>,
    query: String,
    source_id: String,
    request_id: u64,
) -> Result<SearchEventDto, AppError> {
    let state = state.inner().clone();
    let runtime = state
        .search
        .clone()
        .ok_or_else(|| AppError::new("search", "Поиск недоступен."))?;
    let settings = state.settings.load().settings;
    let source = runtime
        .service
        .sources()
        .iter()
        .find(|source| {
            source.id() == source_id
                && source.supports_search()
                && !settings
                    .disabled_sources
                    .iter()
                    .any(|disabled| disabled.eq_ignore_ascii_case(source.id()))
        })
        .cloned()
        .ok_or_else(|| AppError::new("search", "Сайт выключен или не поддерживает поиск."))?;
    runtime.fetcher.reset_source(source.id(), &source.home());
    let browse = state
        .browse
        .lock()
        .unwrap()
        .as_ref()
        .map(|session| (session.kind, session.pages));
    let previous = if browse.is_none() {
        Some(
            state
                .last_search
                .lock()
                .unwrap()
                .clone()
                .ok_or_else(|| AppError::new("search", "Сначала выполните поиск."))?,
        )
    } else {
        None
    };
    let status_source = runtime.fetcher.clone();
    let service = SearchService::new(
        vec![source],
        Arc::new(move |source| status_source.status_of(source)),
        runtime.service.options().clone(),
        Some(runtime.matches.clone()),
    );
    let installed = InstalledIndex::build(&state);
    let cancel = Arc::new(AtomicBool::new(false));
    {
        // Повтор занимает тот же слот, что и обычный поиск: новый поиск отменит
        // повтор, а не станет с ним наперегонки за `last_search`.
        let mut slot = state.search_cancel.lock().unwrap();
        if slot.is_some() {
            return Err(AppError::new(
                "search",
                "Поиск уже идёт. Дождитесь его окончания.",
            ));
        }
        *slot = Some((request_id, Arc::clone(&cancel)));
    }
    let state_for_task = state.clone();
    let source_id_for_task = source_id.clone();
    let cancel_for_task = Arc::clone(&cancel);
    let run = tauri::async_runtime::spawn_blocking(move || match browse {
        Some((kind, pages)) => Some(service.browse(kind, 1, pages.max(1), &cancel_for_task, None)),
        None => {
            let previous = previous?;
            service.retry_source(&previous, &source_id_for_task, &query, &cancel_for_task)
        }
    })
    .await;
    let superseded = {
        let mut slot = state.search_cancel.lock().unwrap();
        let ours = slot
            .as_ref()
            .is_some_and(|(current, _)| *current == request_id);
        if ours {
            *slot = None;
        }
        !ours
    };
    let response = run
        .map_err(|error| AppError::new("task", error.to_string()))?
        .ok_or_else(|| AppError::new("search", "Сайт не найден."))?;
    if superseded || cancel.load(Ordering::Relaxed) {
        return Err(AppError::new(
            "search",
            "Повтор отменён: уже идёт другой поиск.",
        ));
    }
    if let Some((kind, _)) = browse {
        let mut session_lock = state_for_task.browse.lock().unwrap();
        let Some(session) = session_lock.as_mut() else {
            return Err(AppError::new(
                "search",
                "Раздел уже закрыт. Откройте его заново.",
            ));
        };
        if session.kind != kind {
            return Err(AppError::new(
                "search",
                "Раздел уже закрыт. Откройте его заново.",
            ));
        }
        session
            .per_source
            .insert(source_id.clone(), response.listings);
        if let Some(report) = response.sources.into_iter().next() {
            session.reports.insert(report.source_id.clone(), report);
        }
        let built = assemble_browse(&runtime.service, session);
        session.response = built.clone();
        *state_for_task.last_search.lock().unwrap() = Some(built.clone());
        return Ok(to_event(&built, request_id, "done", &installed, None));
    }
    *state_for_task.last_search.lock().unwrap() = Some(response.clone());
    Ok(to_event(&response, request_id, "done", &installed, None))
}

#[tauri::command]
pub async fn search_cancel(
    state: tauri::State<'_, Arc<AppState>>,
    request_id: Option<u64>,
) -> Result<(), AppError> {
    let slot = state.search_cancel.lock().unwrap();
    if let Some((current, cancel)) = slot.as_ref()
        && request_id.is_none_or(|request| request == *current)
    {
        cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

/// «Пройти проверку» из поиска: окно открывает поиск на том же сайте, чтобы после
/// проверки человек сразу видел выдачу.
#[tauri::command]
pub async fn search_open_check(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    source: String,
    query: String,
) -> Result<bool, AppError> {
    let site = crate::commands::updates::check_site(&source)?;
    let url = check_search_url(site.id, &query)
        .ok_or_else(|| AppError::new("url", "Некорректный адрес сайта."))?;
    let state = state.inner().clone();
    crate::commands::updates::open_site_check(app, state, site, url).await
}

fn check_search_url(source: &str, query: &str) -> Option<Url> {
    let query = query.trim();
    match source {
        "modhub" => bmm_sources::sources::modhub::ModHubSource::search_url(query, 1)
            .or_else(|| Url::parse("https://www.modhub.us/category/beamng-drive-mods").ok()),
        _ => {
            let mut url = Url::parse("https://www.modland.net/search").ok()?;
            url.path_segments_mut().ok()?.push(query);
            Some(url)
        }
    }
}

const PREVIEW_HOSTS: &[&str] = &[
    "modland.net",
    "modhub.us",
    "stmods.org",
    "modsgaming.ru",
    "modsgaming.us",
    "beamng.com",
];

fn preview_url_allowed(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https") && bmm_domain::hosts::host_matches(url, PREVIEW_HOSTS)
}

#[tauri::command]
pub async fn search_page_preview(
    state: tauri::State<'_, Arc<AppState>>,
    source: String,
    page: String,
) -> Result<Option<String>, AppError> {
    let runtime = state
        .search
        .clone()
        .ok_or_else(|| AppError::new("search", "Поиск недоступен."))?;
    let parsed =
        Url::parse(&page).map_err(|_| AppError::new("url", "Некорректный адрес страницы."))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::new(
            "url",
            "Страница с этого сайта не загружается.",
        ));
    }
    let Some(mod_source) = runtime
        .sources
        .iter()
        .find(|item| item.id().eq_ignore_ascii_case(&source))
        .cloned()
    else {
        return Err(AppError::new("source", "Неизвестный сайт."));
    };
    if !mod_source.owns(&parsed) || !mod_source.is_mod_page(&parsed) {
        return Err(AppError::new("url", "Это не страница мода."));
    }
    let previews = runtime.previews.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let cancel = AtomicBool::new(false);
        let listing = mod_source.listing(&parsed, &cancel).ok().flatten()?;
        let preview = listing.preview_url.filter(preview_url_allowed)?;
        let image = previews.fetch(&preview).ok().flatten()?;
        Some(format!(
            "http://bmm.localhost/search-preview/{}.{}",
            image.key, image.extension
        ))
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))?;
    Ok(result)
}

/// Сколько картинок выдачи грузится одновременно.
const SEARCH_PREVIEW_LIMIT: usize = 6;
static SEARCH_PREVIEW_GATE: std::sync::LazyLock<tokio::sync::Semaphore> =
    std::sync::LazyLock::new(|| tokio::sync::Semaphore::new(SEARCH_PREVIEW_LIMIT));

#[tauri::command]
pub async fn search_preview(
    state: tauri::State<'_, Arc<AppState>>,
    url: String,
) -> Result<Option<String>, AppError> {
    let runtime = state
        .search
        .clone()
        .ok_or_else(|| AppError::new("search", "Поиск недоступен."))?;
    let parsed =
        Url::parse(&url).map_err(|_| AppError::new("url", "Некорректный адрес картинки."))?;
    if !preview_url_allowed(&parsed) {
        return Err(AppError::new(
            "url",
            "Картинка с этого сайта не загружается.",
        ));
    }
    // Выдача на 150 карточек раньше запускала 150 фоновых потоков сразу: каждый ждал
    // очереди сайта по 1,5–2,5 с и держал поток. Не больше нескольких одновременно.
    let _permit = SEARCH_PREVIEW_GATE
        .acquire()
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?;
    let cache = runtime.previews.clone();
    let result = tauri::async_runtime::spawn_blocking(move || cache.fetch(&parsed))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?;
    match result {
        Ok(Some(image)) => Ok(Some(format!(
            "http://bmm.localhost/search-preview/{}.{}",
            image.key, image.extension
        ))),
        Ok(None) | Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn search_decide(
    state: tauri::State<'_, Arc<AppState>>,
    query: String,
    a_source: String,
    a_id: String,
    b_source: String,
    b_id: String,
    same: bool,
) -> Result<SearchEventDto, AppError> {
    let runtime = state
        .search
        .clone()
        .ok_or_else(|| AppError::new("search", "Поиск недоступен."))?;
    let previous = state
        .last_search
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| AppError::new("search", "Сначала выполните поиск."))?;
    let a = find_listing(&previous, &a_source, &a_id)
        .ok_or_else(|| AppError::new("search", "Запись не найдена."))?;
    let b = find_listing(&previous, &b_source, &b_id)
        .ok_or_else(|| AppError::new("search", "Запись не найдена."))?;
    runtime
        .matches
        .set(
            a,
            b,
            if same {
                ManualMatch::Same
            } else {
                ManualMatch::Different
            },
        )
        .map_err(|error| AppError::new("matches", error.to_string()))?;
    let browsing = state.browse.lock().unwrap().is_some();
    let response = if browsing {
        runtime.service.regroup_site_order(&previous)
    } else {
        runtime.service.regroup(&previous, &query)
    };
    *state.last_search.lock().unwrap() = Some(response.clone());
    let installed = InstalledIndex::build(&state);
    Ok(to_event(&response, 0, "regroup", &installed, None))
}

fn find_listing<'a>(response: &'a SearchResponse, source: &str, id: &str) -> Option<&'a Listing> {
    response
        .listings
        .iter()
        .find(|listing| listing.source.eq_ignore_ascii_case(source) && listing.source_id == id)
}

fn to_event(
    response: &SearchResponse,
    request_id: u64,
    phase: &str,
    installed: &InstalledIndex,
    error: Option<String>,
) -> SearchEventDto {
    SearchEventDto {
        request_id,
        phase: phase.to_string(),
        cards: response
            .cards
            .iter()
            .map(|card| card_dto(card, installed))
            .collect(),
        sources: response.sources.iter().map(source_dto).collect(),
        error,
    }
}

fn card_dto(card: &ModCard, installed: &InstalledIndex) -> SearchCardDto {
    let listings: Vec<SearchListingDto> = card
        .listings
        .iter()
        .map(|listing| listing_dto(listing, installed))
        .collect();
    SearchCardDto {
        id: card.id.clone(),
        title: card.title.clone(),
        price: price_summary(card.price).to_string(),
        possible_same_as: card.possible_same_as.clone(),
        relevance: card.relevance,
        installed: listings.iter().any(|listing| listing.installed),
        kind: card.kind.map(|kind| kind.code().to_string()),
        listings,
    }
}

fn listing_dto(listing: &Listing, installed: &InstalledIndex) -> SearchListingDto {
    SearchListingDto {
        source: listing.source.clone(),
        source_id: listing.source_id.clone(),
        url: listing.url.to_string(),
        title: listing.title.clone(),
        name: {
            let name = bmm_domain::text::TitleParser::parse(&listing.title).name;
            if name.is_empty() {
                listing.title.clone()
            } else {
                name
            }
        },
        mod_version: listing.mod_version.clone(),
        game_version: listing.game_version.clone(),
        author: listing.author.clone(),
        category: listing.category.clone(),
        size_bytes: listing.size_bytes,
        preview_url: listing.preview_url.as_ref().map(ToString::to_string),
        price: price_status(listing.price).to_string(),
        offers: listing.offers.iter().map(offer_dto).collect(),
        published_at: listing.published_at.map(|date| date.to_rfc3339()),
        updated_at: listing.updated_at.map(|date| date.to_rfc3339()),
        external_id: listing.external_id.clone(),
        installed: installed.contains(listing),
    }
}

fn offer_dto(offer: &Offer) -> OfferDto {
    OfferDto {
        kind: match offer.kind {
            bmm_domain::models::OfferKind::Direct => "direct",
            bmm_domain::models::OfferKind::ExternalHost => "externalHost",
            bmm_domain::models::OfferKind::PaidLink => "paidLink",
        }
        .to_string(),
        host: offer.host.clone(),
        url: offer.url.to_string(),
        file_id: offer.host_file_id.clone(),
        size_bytes: offer.size_bytes,
    }
}

fn source_dto(report: &SourceReport) -> SearchSourceDto {
    SearchSourceDto {
        id: report.source_id.clone(),
        display_name: report.display_name.clone(),
        status: match report.status {
            SourceStatus::Ok => "ok",
            SourceStatus::NeedsHumanCheck => "needsHumanCheck",
            SourceStatus::Unavailable => "unavailable",
        }
        .to_string(),
        count: report.count,
        note: report.note.clone(),
        pending: report.pending,
    }
}

fn price_status(price: PriceStatus) -> &'static str {
    match price {
        PriceStatus::Unknown => "unknown",
        PriceStatus::Free => "free",
        PriceStatus::Paid => "paid",
    }
}

fn price_summary(price: PriceSummary) -> &'static str {
    match price {
        PriceSummary::Unknown => "unknown",
        PriceSummary::Free => "free",
        PriceSummary::Paid => "paid",
        PriceSummary::Mixed => "mixed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_previews_are_limited() {
        assert_eq!(
            SEARCH_PREVIEW_GATE.available_permits(),
            SEARCH_PREVIEW_LIMIT
        );
        let held: Vec<_> = (0..SEARCH_PREVIEW_LIMIT)
            .map(|_| SEARCH_PREVIEW_GATE.try_acquire().unwrap())
            .collect();
        assert!(
            SEARCH_PREVIEW_GATE.try_acquire().is_err(),
            "седьмая картинка ждёт, а не занимает ещё поток"
        );
        drop(held);
    }
    use bmm_domain::library::{InMemoryLibraryStore, InstalledMod, LibraryStore, ModBinding};
    use chrono::Utc;

    fn installed(id: &str, name: &str) -> InstalledMod {
        InstalledMod {
            id: id.to_string(),
            name: name.to_string(),
            file_name: format!("{id}.zip"),
            version: None,
            size_bytes: None,
            installed_at_utc: Utc::now().fixed_offset(),
            enabled: true,
        }
    }

    fn listing(source: &str, id: &str, title: &str) -> Listing {
        Listing::new(
            source,
            Url::parse(&format!("https://{source}.test/{id}/")).unwrap(),
            id,
            title,
        )
    }

    fn bind(store: &InMemoryLibraryStore, mod_id: &str, source: &str, id: &str, confirmed: bool) {
        store
            .upsert_binding(ModBinding {
                mod_id: mod_id.to_string(),
                source: source.to_string(),
                source_id: id.to_string(),
                url: Url::parse(&format!("https://{source}.test/{id}/")).unwrap(),
                confirmed,
            })
            .unwrap();
    }

    #[test]
    fn only_known_hosts_are_allowed_for_previews() {
        for raw in [
            "https://www.modland.net/i.webp",
            "http://modland.net/i.jpg",
            "https://stmods.org/i.jpg",
            "https://modsgaming.ru/i.png",
            "https://www.modsgaming.us/_ld/i.jpg",
            "https://cdn.beamng.com/i.jpg",
        ] {
            assert!(
                preview_url_allowed(&Url::parse(raw).unwrap()),
                "{raw} — сайт из списка"
            );
        }
        for raw in [
            "https://evil.example/i.jpg",
            "https://modland.net.evil.example/i.jpg",
            "http://127.0.0.1/i.jpg",
            "http://localhost/i.jpg",
            "file:///C:/x.png",
        ] {
            assert!(
                !preview_url_allowed(&Url::parse(raw).unwrap()),
                "{raw} — чужой адрес"
            );
        }
    }

    #[test]
    fn followed_page_is_installed_but_namesakes_are_not() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("rr", "Rolls-Royce Cullinan"));
        bind(&store, "rr", "stmods", "luxuryteam", true);
        let index = InstalledIndex::from_store(&store);
        assert!(index.contains(&listing("stmods", "luxuryteam", "Rolls-Royce Cullinan")));
        // Тот же адрес без косой черты в конце.
        let mut no_slash = listing("other", "x", "Что-то");
        no_slash.url = Url::parse("https://stmods.test/luxuryteam").unwrap();
        assert!(index.contains(&no_slash));
        // Одноимённый мод другого автора на другом сайте — не установлен.
        assert!(!index.contains(&listing("modsgaming", "7", "Rolls-Royce Cullinan")));
    }

    #[test]
    fn suggested_page_is_not_installed_yet() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("maserati", "Maserati GranTurismo"));
        bind(&store, "maserati", "stmods", "cheekqo", false);
        let index = InstalledIndex::from_store(&store);
        assert!(!index.contains(&listing("stmods", "cheekqo", "Lamborghini Urus")));
        // Пока страницы нет, узнаём мод по названию.
        assert!(index.contains(&listing("stmods", "cheekqo", "Maserati GranTurismo")));
    }

    #[test]
    fn listings_are_found_by_site_and_id() {
        let response = SearchResponse {
            cards: Vec::new(),
            sources: Vec::new(),
            listings: vec![listing("STMods", "a", "A"), listing("modland", "a", "B")],
        };
        assert_eq!(find_listing(&response, "stmods", "a").unwrap().title, "A");
        assert_eq!(find_listing(&response, "modland", "a").unwrap().title, "B");
        assert!(find_listing(&response, "stmods", "b").is_none());
    }

    #[test]
    fn search_event_marks_installed_pages_and_prices() {
        use bmm_sources::search::ModCard;
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("rr", "Rolls-Royce Cullinan"));
        bind(&store, "rr", "stmods", "luxuryteam", true);
        let index = InstalledIndex::from_store(&store);
        let mut paid = listing("modland", "9", "Rolls-Royce Phantom");
        paid.price = PriceStatus::Paid;
        let response = SearchResponse {
            cards: vec![
                ModCard {
                    id: "stmods:luxuryteam".to_string(),
                    title: "Rolls-Royce Cullinan".to_string(),
                    listings: vec![listing("stmods", "luxuryteam", "Rolls-Royce Cullinan")],
                    price: PriceSummary::Free,
                    possible_same_as: vec!["modland:9".to_string()],
                    relevance: 1.0,
                    kind: Some(bmm_domain::text::ModKind::Vehicle),
                },
                ModCard {
                    id: "modland:9".to_string(),
                    title: "Rolls-Royce Phantom".to_string(),
                    listings: vec![paid],
                    price: PriceSummary::Paid,
                    possible_same_as: Vec::new(),
                    relevance: 0.5,
                    kind: None,
                },
            ],
            sources: vec![SourceReport {
                source_id: "stmods".to_string(),
                display_name: "STMods".to_string(),
                status: SourceStatus::NeedsHumanCheck,
                count: 1,
                note: None,
                pending: false,
            }],
            listings: Vec::new(),
        };
        let event = to_event(&response, 7, "done", &index, None);
        assert_eq!(event.request_id, 7);
        assert_eq!(event.phase, "done");
        assert!(event.cards[0].installed);
        assert!(event.cards[0].listings[0].installed);
        assert_eq!(event.cards[0].listings[0].name, "Rolls-Royce Cullinan");
        assert_eq!(event.cards[0].price, "free");
        assert_eq!(event.cards[0].kind.as_deref(), Some("vehicle"));
        assert_eq!(event.cards[1].kind, None);
        assert_eq!(event.cards[0].possible_same_as, vec!["modland:9"]);
        assert!(!event.cards[1].installed);
        assert_eq!(event.cards[1].price, "paid");
        assert_eq!(event.cards[1].listings[0].price, "paid");
        assert_eq!(event.sources[0].status, "needsHumanCheck");
    }

    #[test]
    fn browse_assembly_keeps_pages_and_counts() {
        use crate::state::BrowseSession;
        use bmm_sources::search::SearchOptions;

        let service = SearchService::new(
            Vec::new(),
            Arc::new(|_| SourceStatus::Ok),
            SearchOptions::default(),
            None,
        );
        let mut session = BrowseSession {
            kind: ModKind::Map,
            pages: 2,
            order: vec!["one".to_string(), "two".to_string()],
            per_source: BTreeMap::new(),
            reports: BTreeMap::new(),
            response: SearchResponse {
                cards: Vec::new(),
                sources: Vec::new(),
                listings: Vec::new(),
            },
        };
        session.per_source.insert(
            "one".to_string(),
            vec![
                listing("one", "1", "Zulu Map"),
                listing("one", "2", "Yankee Map"),
            ],
        );
        session
            .per_source
            .insert("two".to_string(), vec![listing("two", "3", "Alpha Map")]);
        for (id, count) in [("one", 0), ("two", 0)] {
            session.reports.insert(
                id.to_string(),
                SourceReport {
                    source_id: id.to_string(),
                    display_name: id.to_string(),
                    status: SourceStatus::Ok,
                    count,
                    note: None,
                    pending: false,
                },
            );
        }
        let built = assemble_browse(&service, &session);
        assert_eq!(built.listings.len(), 3);
        assert_eq!(built.sources[0].count, 2);
        assert_eq!(built.sources[1].count, 1);
        assert_eq!(built.cards[0].title, "Zulu Map", "порядок сайтов сохранён");
    }

    #[test]
    fn search_window_and_rust_agree_on_values() {
        use crate::test_support::ts_union;
        use bmm_domain::models::{Offer, OfferKind};
        use bmm_sources::search::ModCard;

        let mut item = listing("stmods", "bmw", "BMW E34 версия 3.3");
        item.offers = vec![Offer {
            kind: OfferKind::ExternalHost,
            host: "modsfire".to_string(),
            url: "https://modsfire.com/x".parse().unwrap(),
            host_file_id: Some("x".to_string()),
            size_bytes: Some(1),
        }];
        let response = SearchResponse {
            cards: vec![ModCard {
                id: "stmods:bmw".to_string(),
                title: "BMW E34".to_string(),
                listings: vec![item],
                price: PriceSummary::Mixed,
                possible_same_as: Vec::new(),
                relevance: 1.0,
                kind: Some(bmm_domain::text::ModKind::Vehicle),
            }],
            sources: vec![SourceReport {
                source_id: "stmods".to_string(),
                display_name: "STMods".to_string(),
                status: SourceStatus::Ok,
                count: 1,
                note: None,
                pending: false,
            }],
            listings: Vec::new(),
        };
        // Каждое значение, которое может прийти из Rust, окно знает.
        let kinds = ts_union("unions.ts", "ModRole");
        for kind in [
            bmm_domain::text::ModKind::Vehicle,
            bmm_domain::text::ModKind::Map,
            bmm_domain::text::ModKind::Addon,
        ] {
            assert!(
                kinds.contains(kind.code()),
                "окно не знает тип {}",
                kind.code()
            );
        }
        let summaries = ts_union("unions.ts", "SearchPriceSummary");
        for price in [
            PriceSummary::Unknown,
            PriceSummary::Free,
            PriceSummary::Paid,
            PriceSummary::Mixed,
        ] {
            assert!(summaries.contains(price_summary(price)));
        }
        let prices = ts_union("unions.ts", "SearchPrice");
        for price in [PriceStatus::Unknown, PriceStatus::Free, PriceStatus::Paid] {
            assert!(prices.contains(price_status(price)));
        }
        let statuses = ts_union("unions.ts", "SearchSourceStatus");
        for status in [
            SourceStatus::Ok,
            SourceStatus::NeedsHumanCheck,
            SourceStatus::Unavailable,
        ] {
            let mut report = response.sources[0].clone();
            report.status = status;
            assert!(statuses.contains(&source_dto(&report).status));
        }
        let offer_kinds = ts_union("unions.ts", "SearchOfferKind");
        for kind in [
            OfferKind::Direct,
            OfferKind::ExternalHost,
            OfferKind::PaidLink,
        ] {
            let offer = Offer {
                kind,
                host: String::new(),
                url: "https://example.com".parse().unwrap(),
                host_file_id: None,
                size_bytes: None,
            };
            assert!(offer_kinds.contains(&offer_dto(&offer).kind));
        }
        let phases = ts_union("unions.ts", "SearchPhase");
        for phase in ["partial", "done", "error", "regroup"] {
            assert!(phases.contains(phase));
        }
    }
}

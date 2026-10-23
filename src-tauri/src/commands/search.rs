//! Поиск по сайтам: поток событий, отмена и ручные решения о дублях.

use std::collections::HashSet;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use url::Url;

use bmm_domain::filename;
use bmm_domain::matches::{ManualMatch, MatchDecisions};
use bmm_domain::models::{Listing, Offer, PriceStatus};
use bmm_domain::text::{TitleNormalizer, TitleParser};

use bmm_sources::search::{ModCard, PriceSummary, SearchResponse, SearchService, SourceReport};
use bmm_sources::sources::SourceStatus;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferDto {
    pub kind: String,
    pub host: String,
    pub url: String,
    pub file_id: Option<String>,
    pub size_bytes: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchListingDto {
    pub source: String,
    pub source_id: String,
    pub url: String,
    pub title: String,
    pub mod_version: Option<String>,
    pub game_version: Option<String>,
    pub author: Option<String>,
    pub category: Option<String>,
    pub size_bytes: Option<i64>,
    pub preview_url: Option<String>,
    pub price: String,
    pub offers: Vec<OfferDto>,
    pub published_at: Option<String>,
    pub updated_at: Option<String>,
    pub external_id: Option<String>,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchCardDto {
    pub id: String,
    pub title: String,
    pub price: String,
    pub possible_same_as: Vec<String>,
    pub relevance: f64,
    pub installed: bool,
    pub listings: Vec<SearchListingDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchSourceDto {
    pub id: String,
    pub display_name: String,
    pub status: String,
    pub count: usize,
    pub note: Option<String>,
    pub pending: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchEventDto {
    pub request_id: u64,
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
    let decisions: Arc<dyn MatchDecisions + Send + Sync> = runtime.matches.clone();
    let service = SearchService::new(
        sources,
        Arc::new(move |source| fetcher.status_of(source)),
        options,
        Some(decisions),
    );
    let installed = Arc::new(InstalledIndex::build(&state));
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

#[tauri::command]
pub async fn search_open_modland(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    query: String,
) -> Result<bool, AppError> {
    let mut url = Url::parse("https://www.modland.net/search")
        .map_err(|_| AppError::new("url", "Некорректный адрес Modland."))?;
    url.path_segments_mut()
        .map_err(|_| AppError::new("url", "Некорректный адрес Modland."))?
        .push(query.trim());
    let state = state.inner().clone();
    crate::commands::updates::open_modland_check(app, state, url).await
}

const PREVIEW_HOSTS: &[&str] = &["modland.net", "stmods.org", "modsgaming.ru", "beamng.com"];

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
        let preview = listing.preview_url?;
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
    if !matches!(parsed.scheme(), "http" | "https")
        || !bmm_domain::hosts::host_matches(&parsed, PREVIEW_HOSTS)
    {
        return Err(AppError::new(
            "url",
            "Картинка с этого сайта не загружается.",
        ));
    }
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
    let response = runtime.service.regroup(&previous, &query);
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
        listings,
    }
}

fn listing_dto(listing: &Listing, installed: &InstalledIndex) -> SearchListingDto {
    SearchListingDto {
        source: listing.source.clone(),
        source_id: listing.source_id.clone(),
        url: listing.url.to_string(),
        title: listing.title.clone(),
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
}

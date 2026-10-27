//! Подбор страниц модам без привязок и действия с предложенной страницей.
//!
//! «Нашли страницу» → привязка неподтверждённая; «Следить» подтверждает её,
//! «Это другой мод» отклоняет и запоминает отказ, «Отменить» возвращает
//! предложение. Всё это меняет несколько записей хранилища разом.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, FixedOffset};

use bmm_domain::library::{CheckState, InstalledMod, LibraryStore, ModBinding, PageSearch};
use bmm_domain::models::Listing;
use bmm_domain::text::TitleNormalizer;
use bmm_sources::search::SearchService;

use crate::bindings::gate;
use crate::page_guesser::{CERTAIN_SIZE_RATIO, MIN_NAME, PageGuesser};
use crate::runner::UpdateProgress;

/// Итог подбора страниц.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageSearchReport {
    /// Сколько модов получили предложенную страницу.
    pub found: usize,
    /// Из них сразу взяты под наблюдение (совпадение почти наверняка).
    pub followed: usize,
    /// Сколько модов реально искали (хотя бы один сайт ответил).
    pub searched: usize,
    pub cancelled: bool,
}

impl PageSearchReport {
    /// Есть ли что показать пользователю.
    pub fn has_suggestions(&self) -> bool {
        self.found > 0
    }
}

/// Похожая страница того же мода на другом сайте.
#[derive(Debug, Clone, PartialEq)]
pub struct PageMatch {
    pub source: String,
    pub title: String,
    pub url: url::Url,
    pub version: Option<String>,
    pub size: Option<i64>,
    pub author: Option<String>,
    pub similarity: f64,
    pub same_size: bool,
    pub same_version: bool,
    pub same_author: bool,
}

/// Результат ручной проверки страницы по другим источникам.
#[derive(Debug, Clone, PartialEq)]
pub struct PageVerification {
    pub page_source: String,
    pub page_title: String,
    pub page_url: url::Url,
    pub page_size: Option<i64>,
    pub matches: Vec<PageMatch>,
}

/// Открывает выбранную страницу и ищет её же на других включённых сайтах.
/// Ничего не привязывает: это доказательства для человека.
pub fn verify_page(
    store: &dyn LibraryStore,
    search: &SearchService,
    mod_id: &str,
    cancel: &AtomicBool,
    title_of: &dyn Fn(&str) -> Option<String>,
) -> Result<PageVerification, String> {
    let binding = store
        .bindings()
        .into_iter()
        .find(|binding| binding.mod_id == mod_id)
        .ok_or_else(|| "У мода нет страницы для проверки.".to_string())?;
    let page_source = search
        .sources()
        .iter()
        .find(|source| source.id() == binding.source)
        .ok_or_else(|| "Сайт этой страницы выключен в настройках.".to_string())?;
    let page = match page_source.listing(&binding.url, cancel) {
        Ok(Some(page)) => page,
        Ok(None) => {
            return Err("Страница мода не найдена: возможно, её удалили.".to_string());
        }
        Err(error) => return Err(format!("Страница не открылась: {}", error.message)),
    };
    let archive_title = title_of(mod_id).filter(|title| !title.trim().is_empty());
    let identity_title = archive_title.as_deref().unwrap_or(&page.title);

    let mut queries = vec![page.title.clone()];
    if let Some(title) = archive_title.as_deref()
        && !queries
            .iter()
            .any(|query| query.eq_ignore_ascii_case(title))
    {
        queries.push(title.to_string());
    }

    let mut candidates: Vec<Listing> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for query in &queries {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let response = search.search(query, cancel, None);
        let found = if response.listings.is_empty() {
            response
                .cards
                .into_iter()
                .flat_map(|card| card.listings)
                .collect()
        } else {
            response.listings
        };
        for listing in found {
            if listing.source == page.source {
                continue;
            }
            if seen.insert((listing.source.clone(), listing.source_id.clone())) {
                candidates.push(listing);
            }
        }
    }

    let mut matches: Vec<PageMatch> = candidates
        .into_iter()
        .filter_map(|listing| {
            let similarity = TitleNormalizer::similarity(identity_title, &listing.title);
            let same_size = match (page.size_bytes, listing.size_bytes) {
                (Some(mine), Some(theirs)) if mine > 0 && theirs > 0 => {
                    mine.min(theirs) as f64 / mine.max(theirs) as f64 >= CERTAIN_SIZE_RATIO
                }
                _ => false,
            };
            let same_version =
                page.mod_version.is_some() && page.mod_version == listing.mod_version;
            let same_author = crate::page_guesser::authors_match(
                page.author.as_deref(),
                listing.author.as_deref(),
            );
            // Одна версия — ещё не тот же мод: на сайтах полно «1.1» и «1.0».
            let title_ok = similarity >= MIN_NAME;
            let supporting = (same_size || same_author) && similarity >= 0.5;
            if !title_ok && !supporting {
                return None;
            }
            Some(PageMatch {
                source: listing.source,
                title: listing.title,
                url: listing.url,
                version: listing.mod_version,
                size: listing.size_bytes,
                author: listing.author,
                similarity,
                same_size,
                same_version,
                same_author,
            })
        })
        .collect();
    matches.sort_by(|a, b| {
        let score = |item: &PageMatch| {
            item.similarity
                + if item.same_size { 0.15 } else { 0.0 }
                + if item.same_version { 0.1 } else { 0.0 }
                + if item.same_author { 0.15 } else { 0.0 }
        };
        score(b)
            .partial_cmp(&score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    matches.truncate(5);

    Ok(PageVerification {
        page_source: page.source,
        page_title: page.title,
        page_url: page.url,
        page_size: page.size_bytes,
        matches,
    })
}

/// Подбирает страницы для модов без привязок.
///
/// Каждый мод ищется по названию (из окна, иначе из имени файла) на включённых
/// сайтах. Если ни один сайт не ответил, попытка не записывается: повторим
/// в следующий раз. Найденная страница ставится неподтверждённой.
pub fn find_pages(
    store: &dyn LibraryStore,
    search: &SearchService,
    now: DateTime<FixedOffset>,
    cancel: &AtomicBool,
    title_of: &dyn Fn(&str) -> Option<String>,
    author_of: &dyn Fn(&str) -> Option<String>,
    progress: Option<&mut dyn FnMut(&UpdateProgress)>,
) -> PageSearchReport {
    find_pages_with(
        store, search, now, cancel, true, title_of, author_of, progress,
    )
}

/// Как [`find_pages`], но можно пропустить выключенные моды.
#[allow(clippy::too_many_arguments)]
pub fn find_pages_with(
    store: &dyn LibraryStore,
    search: &SearchService,
    now: DateTime<FixedOffset>,
    cancel: &AtomicBool,
    include_disabled: bool,
    title_of: &dyn Fn(&str) -> Option<String>,
    author_of: &dyn Fn(&str) -> Option<String>,
    mut progress: Option<&mut dyn FnMut(&UpdateProgress)>,
) -> PageSearchReport {
    let due = PageGuesser::due_with(store, now, include_disabled);
    let mut result = PageSearchReport::default();
    if due.is_empty() {
        return result;
    }

    for (index, installed) in due.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        if let Some(callback) = progress.as_mut() {
            (*callback)(&UpdateProgress {
                source: "страницы".to_string(),
                stage: "страницы".to_string(),
                done: index,
                total: due.len(),
            });
        }

        let title = title_of(&installed.id);
        let primary = PageGuesser::query_for(installed, title.as_deref());
        let mut queries = vec![primary];
        // Имя из info.json может отличаться от имени файла/страницы: пробуем и его.
        if title
            .as_deref()
            .is_some_and(|title| !title.trim().is_empty())
        {
            let from_file = PageGuesser::query_for(installed, None);
            if !queries
                .iter()
                .any(|query| query.eq_ignore_ascii_case(&from_file))
            {
                queries.push(from_file);
            }
        }
        let mut searched_ok = false;
        let mut listings: Vec<Listing> = Vec::new();
        let mut seen_listings: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut stopped = false;
        for query in &queries {
            if cancel.load(Ordering::Relaxed) {
                result.cancelled = true;
                stopped = true;
                break;
            }
            let response = search.search(query, cancel, None);
            if response
                .sources
                .iter()
                .any(|source| source.status == bmm_sources::sources::SourceStatus::Ok)
            {
                searched_ok = true;
            }
            let found = if response.listings.is_empty() {
                response
                    .cards
                    .into_iter()
                    .flat_map(|card| card.listings)
                    .collect()
            } else {
                response.listings
            };
            for listing in found {
                if seen_listings.insert((listing.source.clone(), listing.source_id.clone())) {
                    listings.push(listing);
                }
            }
        }
        if stopped {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        // Ни один сайт не ответил: не считаем, что искали, — попробуем при следующей проверке.
        if !searched_ok {
            continue;
        }
        result.searched += 1;

        let _gate = gate();
        // За время поиска мод могли удалить, привязать вручную или отказаться от проверки.
        if store.get_mod(&installed.id).is_none()
            || store.is_checking_declined(&installed.id)
            || store
                .bindings()
                .iter()
                .any(|binding| binding.mod_id == installed.id)
        {
            continue;
        }
        let author = author_of(&installed.id);
        let mut guess = PageGuesser::pick_with_author(
            installed,
            title.as_deref(),
            author.as_deref(),
            &listings,
        );
        // Следить без вопроса можно, только если ничто не спорит. В выдаче поиска размера
        // обычно нет, а совпадение одного названия обманчиво («Mercedes-Benz ML» — десятки
        // разных модов): открываем страницу мода и решаем ещё раз уже с размером и автором.
        if let Some(found) = guess.as_ref().filter(|guess| {
            guess.certain && (guess.listing.size_bytes.is_none() || guess.listing.author.is_none())
        }) && let Some(detail) = page_details(search, &found.listing, cancel)
        {
            let enriched: Vec<Listing> = listings
                .iter()
                .map(|listing| {
                    if listing.source == detail.source && listing.source_id == detail.source_id {
                        detail.clone()
                    } else {
                        listing.clone()
                    }
                })
                .collect();
            guess = PageGuesser::pick_with_author(
                installed,
                title.as_deref(),
                author.as_deref(),
                &enriched,
            );
        }
        if let Some(guess) = guess {
            let binding = ModBinding {
                mod_id: installed.id.clone(),
                source: guess.listing.source,
                source_id: guess.listing.source_id,
                url: guess.listing.url,
                confirmed: guess.certain,
            };
            if store.upsert_binding(binding).is_ok() {
                result.found += 1;
                if guess.certain {
                    result.followed += 1;
                }
            }
        }
        store.set_page_search(
            &installed.id,
            Some(PageSearch {
                at: now,
                rejected: false,
            }),
        );
    }

    if let Some(callback) = progress.as_mut() {
        (*callback)(&UpdateProgress {
            source: "страницы".to_string(),
            stage: "страницы".to_string(),
            done: due.len(),
            total: due.len(),
        });
    }
    result
}

/// Запись со страницы мода поверх записи из поиска: размер, автор и версия, которых
/// в выдаче поиска обычно нет. `None` — страницу не открыть.
fn page_details(search: &SearchService, found: &Listing, cancel: &AtomicBool) -> Option<Listing> {
    let source = search
        .sources()
        .iter()
        .find(|source| source.id() == found.source)?;
    let detail = source.listing(&found.url, cancel).ok()??;
    let mut merged = found.clone();
    merged.size_bytes = detail.size_bytes.or(merged.size_bytes);
    merged.author = detail.author.or(merged.author);
    merged.mod_version = detail.mod_version.or(merged.mod_version);
    merged.game_version = detail.game_version.or(merged.game_version);
    Some(merged)
}

/// «Следить»: подтверждает предложенную страницу, снимает отказ «Не проверять»
/// и скрытие обновления. Возвращает прежнюю (неподтверждённую) привязку для «Отменить».
pub fn confirm_page(store: &dyn LibraryStore, mod_id: &str) -> Option<ModBinding> {
    let _gate = gate();
    let suggestion = store
        .bindings()
        .into_iter()
        .find(|binding| binding.mod_id == mod_id && !binding.confirmed)?;
    store.get_mod(mod_id)?;
    let confirmed = ModBinding {
        confirmed: true,
        ..suggestion.clone()
    };
    store.upsert_binding(confirmed).ok()?;
    store.set_checking_declined(mod_id, false);
    store.set_dismissed_update(mod_id, None);
    Some(suggestion)
}

/// «Это другой мод»: убирает предложение и запоминает отказ, чтобы не искать
/// этот мод снова 14 дней. Возвращает снятую привязку для «Отменить».
pub fn reject_page(
    store: &dyn LibraryStore,
    mod_id: &str,
    now: DateTime<FixedOffset>,
) -> Option<ModBinding> {
    let _gate = gate();
    let suggestion = store
        .bindings()
        .into_iter()
        .find(|binding| binding.mod_id == mod_id && !binding.confirmed)?;
    store.remove_binding(mod_id, &suggestion.source);
    store.set_page_search(
        mod_id,
        Some(PageSearch {
            at: now,
            rejected: true,
        }),
    );
    Some(suggestion)
}

/// «Отменить» после подтверждения или отказа: возвращает предложение и снимает
/// отметку поиска. Если за это время привязали другую страницу того же сайта,
/// её не трогаем.
pub fn undo_page(
    store: &dyn LibraryStore,
    suggestion: &ModBinding,
    now: DateTime<FixedOffset>,
) -> bool {
    let _gate = gate();
    if store.get_mod(&suggestion.mod_id).is_none() {
        return false;
    }
    let current = store
        .bindings()
        .into_iter()
        .find(|binding| binding.mod_id == suggestion.mod_id && binding.source == suggestion.source);
    if let Some(current) = &current
        && current.source_id != suggestion.source_id
    {
        return false;
    }
    if current.is_some() {
        // Вместе с проверками этой страницы.
        store.remove_binding(&suggestion.mod_id, &suggestion.source);
    }
    let restored = ModBinding {
        confirmed: false,
        ..suggestion.clone()
    };
    if let Err(error) = store.upsert_binding(restored) {
        debug_assert!(false, "мод пропал во время отмены: {error}");
        return false;
    }
    store.set_page_search(
        &suggestion.mod_id,
        Some(PageSearch {
            at: now,
            rejected: false,
        }),
    );
    true
}

/// Страница, за которой перестали следить, со всем, что о ней знали, — для «Отменить».
#[derive(Debug, Clone, PartialEq)]
pub struct Unfollowed {
    pub binding: ModBinding,
    pub check: Option<CheckState>,
    pub search: Option<PageSearch>,
    pub dismissed: Option<String>,
}

/// «Это другой мод» для страницы, за которой уже следим (взятой программой сама или
/// подтверждённой раньше). Привязка снимается вместе с проверками, найденное по ней
/// обновление пропадает, а мод больше не ищется сам, чтобы та же ошибка не повторилась:
/// страницу можно указать вручную.
pub fn unfollow_page(
    store: &dyn LibraryStore,
    mod_id: &str,
    source: &str,
    now: DateTime<FixedOffset>,
) -> Option<Unfollowed> {
    let _gate = gate();
    let binding = store.bindings().into_iter().find(|binding| {
        binding.mod_id == mod_id && binding.source == source && binding.confirmed
    })?;
    let saved = Unfollowed {
        check: store.get_check(mod_id, source),
        search: store.get_page_search(mod_id),
        dismissed: store.get_dismissed_update(mod_id),
        binding,
    };
    store.remove_binding(mod_id, source);
    store.set_dismissed_update(mod_id, None);
    store.set_page_search(
        mod_id,
        Some(PageSearch {
            at: now,
            rejected: true,
        }),
    );
    Some(saved)
}

/// «Отменить» после [`unfollow_page`]: возвращает страницу, проверки и пропуски как
/// были. Если за это время на том же сайте привязали другую страницу, её не трогаем.
pub fn refollow_page(store: &dyn LibraryStore, saved: &Unfollowed) -> bool {
    let _gate = gate();
    let binding = &saved.binding;
    if store.get_mod(&binding.mod_id).is_none() {
        return false;
    }
    if store.bindings().iter().any(|other| {
        other.mod_id == binding.mod_id
            && other.source == binding.source
            && other.source_id != binding.source_id
    }) {
        return false;
    }
    if store.upsert_binding(binding.clone()).is_err() {
        return false;
    }
    if let Some(check) = &saved.check {
        store.save_check(check.clone());
    }
    store.set_page_search(&binding.mod_id, saved.search.clone());
    store.set_dismissed_update(&binding.mod_id, saved.dismissed.as_deref());
    true
}

/// Удобно для интерфейса: мод, у которого есть неподтверждённое предложение.
pub fn suggestion_for(store: &dyn LibraryStore, mod_id: &str) -> Option<ModBinding> {
    store
        .bindings()
        .into_iter()
        .find(|binding| binding.mod_id == mod_id && !binding.confirmed)
}

/// Все моды с предложенной, но не подтверждённой страницей.
pub fn suggestions(store: &dyn LibraryStore) -> Vec<ModBinding> {
    let mut result: Vec<ModBinding> = store
        .bindings()
        .into_iter()
        .filter(|binding| !binding.confirmed)
        .collect();
    result.sort_by(|a, b| {
        a.mod_id
            .cmp(&b.mod_id)
            .then_with(|| a.source.cmp(&b.source))
            .then_with(|| a.source_id.cmp(&b.source_id))
    });
    result
}

/// «Следить» подтверждает уже предложенную страницу (`confirm_page`).
/// Для окна «Указать страницу»: человек сам выбрал запись на сайте.
///
/// `replace_source` — потерянная привязка, которую заменяем (её состояние
/// проверок уйдёт вместе с ней). На одном сайте за модом следим по одной
/// странице: прежняя привязка того же сайта тоже заменяется.
/// `verified` — страницу только что открыли целиком: её версия и размер уже
/// известны, поэтому обновление видно сразу, без следующей проверки.
pub fn bind_page(
    store: &dyn LibraryStore,
    mod_id: &str,
    listing: &Listing,
    replace_source: Option<&str>,
    verified: bool,
    now: DateTime<FixedOffset>,
) -> bool {
    let _gate = gate();
    if store.get_mod(mod_id).is_none() {
        return false;
    }
    if let Some(source) = replace_source {
        store.remove_binding(mod_id, source);
    }
    if store
        .bindings()
        .iter()
        .any(|binding| binding.mod_id == mod_id && binding.source == listing.source)
    {
        store.remove_binding(mod_id, &listing.source);
    }
    let binding = ModBinding {
        mod_id: mod_id.to_string(),
        source: listing.source.clone(),
        source_id: listing.source_id.clone(),
        url: listing.url.clone(),
        confirmed: true,
    };
    if store.upsert_binding(binding).is_err() {
        return false;
    }
    store.set_checking_declined(mod_id, false);
    store.set_dismissed_update(mod_id, None);
    if verified {
        let tracked =
            crate::planner::TrackedBinding::new(mod_id, &listing.source, &listing.source_id);
        crate::service::UpdateCheckService::apply_card(store, &tracked, listing, now);
    }
    true
}

/// «Указать страницу»: человек вставил ссылку на страницу мода. Страница открывается
/// сразу — так видно, что это действительно страница мода, и версия с размером
/// известны без следующей проверки. Возвращает прочитанную запись или понятную причину.
pub fn bind_url(
    store: &dyn LibraryStore,
    sources: &[std::sync::Arc<dyn bmm_sources::sources::ModSource>],
    mod_id: &str,
    url: &str,
    cancel: &AtomicBool,
    now: DateTime<FixedOffset>,
) -> Result<Listing, String> {
    let url = url::Url::parse(url.trim())
        .map_err(|_| "Это не ссылка. Скопируйте адрес страницы мода из браузера.".to_string())?;
    let source = sources
        .iter()
        .find(|source| source.owns(&url))
        .ok_or_else(|| {
            "Этот сайт программа не знает: подойдут STMods, ModsGaming, Modland и репозиторий BeamNG."
                .to_string()
        })?;
    if !source.is_mod_page(&url) {
        return Err(format!(
            "Это не страница мода на {}: откройте сам мод и скопируйте его адрес.",
            source.display_name()
        ));
    }
    if store.get_mod(mod_id).is_none() {
        return Err("Мод не найден. Обновите список.".to_string());
    }
    let listing = match source.listing(&url, cancel) {
        Ok(Some(listing)) => listing,
        Ok(None) => {
            return Err(format!(
                "На {} по этой ссылке нет мода: страница удалена или адрес неполный.",
                source.display_name()
            ));
        }
        Err(error) => {
            return Err(format!("Страница не открылась: {}", error.message));
        }
    };
    if !bind_page(store, mod_id, &listing, None, true, now) {
        return Err("Мод не найден. Обновите список.".to_string());
    }
    Ok(listing)
}

/// Мод без привязок, для которого поиск страницы уже не имеет смысла.
pub fn is_awaiting_manual(store: &dyn LibraryStore, installed: &InstalledMod) -> bool {
    store.is_checking_declined(&installed.id)
        || store
            .get_page_search(&installed.id)
            .is_some_and(|search| search.rejected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    use chrono::TimeDelta;
    use url::Url;

    use bmm_domain::library::{CheckState, InMemoryLibraryStore};
    use bmm_sources::search::{SearchOptions, SearchService};
    use bmm_sources::sources::{ModSource, SourceError, SourceStatus};

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-18T12:00:00+00:00").unwrap()
    }

    fn installed(id: &str, file: &str) -> InstalledMod {
        InstalledMod {
            id: id.to_string(),
            name: id.to_string(),
            file_name: file.to_string(),
            version: Some("1.0".to_string()),
            size_bytes: None,
            installed_at_utc: now() - TimeDelta::days(1),
            enabled: true,
        }
    }

    fn listing(id: &str, title: &str) -> Listing {
        Listing::new(
            "modsgaming",
            Url::parse(&format!("https://modsgaming.test/{id}")).unwrap(),
            id,
            title,
        )
    }

    type Answer = Box<dyn Fn(&str) -> Result<Vec<Listing>, SourceError> + Send + Sync>;

    struct Site {
        calls: AtomicUsize,
        answer: Answer,
    }

    impl Site {
        fn new(
            answer: impl Fn(&str) -> Result<Vec<Listing>, SourceError> + Send + Sync + 'static,
        ) -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicUsize::new(0),
                answer: Box::new(answer),
            })
        }
    }

    impl ModSource for Site {
        fn id(&self) -> &'static str {
            "modsgaming"
        }
        fn display_name(&self) -> &'static str {
            "ModsGaming"
        }
        fn supports_search(&self) -> bool {
            true
        }
        fn home(&self) -> Url {
            Url::parse("https://modsgaming.test/").unwrap()
        }
        fn owns(&self, url: &Url) -> bool {
            url.host_str() == Some("modsgaming.test")
        }
        fn is_mod_page(&self, url: &Url) -> bool {
            self.owns(url) && url.path() != "/"
        }
        fn feed_covers(&self, _url: &Url) -> bool {
            true
        }
        fn search(&self, query: &str, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            (self.answer)(query)
        }
        fn feed_page(&self, _page: u32, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }
        fn listing(
            &self,
            _url: &Url,
            _cancel: &AtomicBool,
        ) -> Result<Option<Listing>, SourceError> {
            Ok(None)
        }
    }

    fn search(site: Arc<Site>) -> SearchService {
        SearchService::new(
            vec![site],
            Arc::new(|_| SourceStatus::Ok),
            SearchOptions {
                per_source_timeout: std::time::Duration::from_secs(5),
                ..SearchOptions::default()
            },
            None,
        )
    }

    struct VerifySite {
        id: &'static str,
        page: Option<Listing>,
        results: Vec<Listing>,
    }

    impl VerifySite {
        fn new(id: &'static str, page: Option<Listing>, results: Vec<Listing>) -> Arc<Self> {
            Arc::new(Self { id, page, results })
        }
    }

    impl ModSource for VerifySite {
        fn id(&self) -> &'static str {
            self.id
        }
        fn display_name(&self) -> &'static str {
            self.id
        }
        fn supports_search(&self) -> bool {
            true
        }
        fn home(&self) -> Url {
            Url::parse(&format!("https://{}.test/", self.id)).unwrap()
        }
        fn owns(&self, url: &Url) -> bool {
            url.host_str()
                .is_some_and(|host| host == format!("{}.test", self.id))
        }
        fn is_mod_page(&self, _url: &Url) -> bool {
            true
        }
        fn feed_covers(&self, _url: &Url) -> bool {
            true
        }
        fn search(&self, _query: &str, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(self.results.clone())
        }
        fn feed_page(&self, _page: u32, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }
        fn listing(
            &self,
            _url: &Url,
            _cancel: &AtomicBool,
        ) -> Result<Option<Listing>, SourceError> {
            Ok(self.page.clone())
        }
    }

    #[test]
    fn searches_archive_title_and_file_name() {
        let store = InMemoryLibraryStore::new();
        let mut car = installed("a", "toyota_sienna_v1.0.zip");
        car.name = "Sienna Archive".to_string();
        store.upsert_mod(car);
        let site = Site::new(|query| {
            if query.to_lowercase().contains("toyota") {
                Ok(vec![listing("s", "Sienna Archive v2.0")])
            } else {
                Ok(Vec::new())
            }
        });
        let search = search(Arc::clone(&site));
        let cancel = AtomicBool::new(false);
        let report = find_pages(
            &store,
            &search,
            now(),
            &cancel,
            &|_| Some("Sienna Archive".to_string()),
            &|_| None,
            None,
        );
        assert_eq!(
            report.found, 1,
            "второй запрос по имени файла нашёл страницу"
        );
        assert!(site.calls.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn verifies_a_page_against_another_site() {
        let store = InMemoryLibraryStore::new();
        let mut maps = installed("maps", "MapsFromRCD.zip");
        maps.size_bytes = Some(68_513_955);
        store.upsert_mod(maps);
        store
            .upsert_binding(ModBinding {
                mod_id: "maps".to_string(),
                source: "modsgaming".to_string(),
                source_id: "123015".to_string(),
                url: Url::parse("https://modsgaming.test/rcd").unwrap(),
                confirmed: false,
            })
            .unwrap();

        let mut page = listing("123015", "RCD - Russian Car Drift v1.1 (0.40.x)");
        page.url = Url::parse("https://modsgaming.test/rcd").unwrap();
        page.size_bytes = Some(68_513_955);
        page.mod_version = Some("1.1".to_string());
        page.author = Some("TurboDiesel".to_string());
        let mut mirror = listing("mirror", "RCD Russian Car Drift 1.1");
        mirror.source = "stmods".to_string();
        mirror.url = Url::parse("https://stmods.test/rcd").unwrap();
        mirror.size_bytes = Some(68_513_955);
        mirror.mod_version = Some("1.1".to_string());
        mirror.author = Some("TurboDiesel".to_string());
        let mut noise = listing("noise", "Farming Simulator Claas Arion");
        noise.source = "stmods".to_string();
        noise.url = Url::parse("https://stmods.test/claas").unwrap();
        noise.mod_version = Some("1.1".to_string());

        let primary = VerifySite::new("modsgaming", Some(page.clone()), Vec::new());
        let other = VerifySite::new("stmods", None, vec![mirror, noise]);
        let service = SearchService::new(
            vec![primary, other],
            Arc::new(|_| SourceStatus::Ok),
            SearchOptions::default(),
            None,
        );
        let result = verify_page(&store, &service, "maps", &AtomicBool::new(false), &|_| None)
            .expect("страница проверяется");
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].source, "stmods");
        assert!(result.matches[0].same_size);
        assert!(result.matches[0].same_version);
        assert!(result.matches[0].same_author);
    }

    #[test]
    fn find_pages_suggests_once_then_confirm_reject_undo() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "toyota_sienna_v1.0.zip"));
        store.upsert_mod(installed("b", "lada_niva_4x4.zip"));
        let site = Site::new(|query| {
            if query.to_lowercase().contains("sienna") {
                Ok(vec![listing(
                    "s",
                    "МОД Toyota Sienna v2.0 (0.34.x) ДЛЯ BEAMNG.DRIVE",
                )])
            } else {
                Ok(Vec::new())
            }
        });
        let search = search(Arc::clone(&site));
        let cancel = AtomicBool::new(false);

        let mut steps = Vec::new();
        let report = find_pages(
            &store,
            &search,
            now(),
            &cancel,
            &|_| None,
            &|_| None,
            Some(&mut |progress: &UpdateProgress| {
                steps.push(format!("{}/{}", progress.done, progress.total));
            }),
        );
        assert_eq!(report.found, 1);
        assert_eq!(steps, ["0/2", "1/2", "2/2"]);

        let bindings = store.bindings();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].mod_id, "a");
        assert_eq!(bindings[0].source_id, "s");
        assert!(!bindings[0].confirmed, "предложена, следить не начали");
        assert!(
            store
                .get_page_search("b")
                .is_some_and(|search| !search.rejected)
        );
        assert_eq!(site.calls.load(Ordering::SeqCst), 2);

        // Повторный подбор ничего не ищет.
        let before = site.calls.load(Ordering::SeqCst);
        let report = find_pages(&store, &search, now(), &cancel, &|_| None, &|_| None, None);
        assert_eq!(report.found, 0);
        assert_eq!(site.calls.load(Ordering::SeqCst), before);

        // «Следить».
        let confirmed = confirm_page(&store, "a").expect("есть предложение");
        assert!(store.bindings()[0].confirmed);
        assert_eq!(store.bindings()[0].source_id, confirmed.source_id);

        // «Отменить» подтверждение — снова только предложение.
        assert!(undo_page(&store, &confirmed, now()));
        assert!(!store.bindings()[0].confirmed);

        // «Это другой мод».
        let rejected = reject_page(&store, "a", now()).expect("есть предложение");
        assert!(store.bindings().is_empty());
        assert!(
            store
                .get_page_search("a")
                .is_some_and(|search| search.rejected)
        );
        assert!(
            PageGuesser::due(&store, now() + TimeDelta::days(60))
                .iter()
                .all(|installed| installed.id != "a")
        );

        // «Отменить» отказ возвращает предложение и снимает rejected.
        assert!(undo_page(&store, &rejected, now()));
        let restored = store.bindings();
        assert_eq!(restored.len(), 1);
        assert!(!restored[0].confirmed);
        assert_eq!(restored[0].mod_id, "a");
        assert!(
            store
                .get_page_search("a")
                .is_some_and(|search| !search.rejected)
        );
    }

    #[test]
    fn no_site_answered_means_not_searched() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "toyota_sienna_2017.zip"));
        let site = Site::new(|_| Err(SourceError::unavailable("код 503")));
        let search = search(site);
        let cancel = AtomicBool::new(false);
        let report = find_pages(&store, &search, now(), &cancel, &|_| None, &|_| None, None);
        assert_eq!(report.found, 0);
        assert_eq!(report.searched, 0);
        assert!(
            store.get_page_search("a").is_none(),
            "попробуем в следующий раз"
        );
    }

    #[test]
    fn stop_interrupts_page_search() {
        let store = InMemoryLibraryStore::new();
        for index in 0..5 {
            store.upsert_mod(installed(
                &format!("m{index}"),
                &format!("car_{index}_mod.zip"),
            ));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&calls);
        let site = Arc::new(Site {
            calls: AtomicUsize::new(0),
            answer: Box::new(move |_| {
                if counter.fetch_add(1, Ordering::SeqCst) == 1 {
                    flag.store(true, Ordering::SeqCst);
                }
                Ok(Vec::new())
            }),
        });
        let search = search(site);
        let report = find_pages(&store, &search, now(), &cancel, &|_| None, &|_| None, None);
        assert!(report.cancelled);
        assert_eq!(
            calls.load(Ordering::SeqCst),
            2,
            "после «Остановить» дальше не ищем"
        );
    }

    #[test]
    fn confirm_requires_suggestion_and_clears_decline_and_dismiss() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "a.zip"));
        assert!(confirm_page(&store, "a").is_none());

        store
            .upsert_binding(ModBinding {
                mod_id: "a".to_string(),
                source: "modsgaming".to_string(),
                source_id: "1".to_string(),
                url: Url::parse("https://modsgaming.test/1").unwrap(),
                confirmed: false,
            })
            .unwrap();
        store.set_checking_declined("a", true);
        store.set_dismissed_update("a", Some("1.0"));
        assert!(confirm_page(&store, "a").is_some());
        assert!(!store.is_checking_declined("a"));
        assert!(store.get_dismissed_update("a").is_none());
        assert!(store.bindings()[0].confirmed);
    }

    #[test]
    fn bind_page_replaces_missing_binding_and_shows_update_immediately() {
        let store = InMemoryLibraryStore::new();
        let mut bmw = installed("bmw", "bmw_e34.zip");
        bmw.version = Some("3.0".to_string());
        store.upsert_mod(bmw);
        store
            .upsert_binding(ModBinding {
                mod_id: "bmw".to_string(),
                source: "modland".to_string(),
                source_id: "old".to_string(),
                url: Url::parse("https://www.modland.test/old.html").unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            mod_id: "bmw".to_string(),
            source: "modland".to_string(),
            last_checked_utc: now(),
            last_changed_utc: None,
            last_version: Some("1.0".to_string()),
            last_size: None,
            miss_count: 3,
            last_game_version: None,
            last_title: None,
        });
        let mut page = Listing::new(
            "stmods",
            Url::parse("https://stmods.test/new").unwrap(),
            "new",
            "BMW E34 версия 3.3",
        );
        page.mod_version = Some("3.3".to_string());

        assert!(bind_page(
            &store,
            "bmw",
            &page,
            Some("modland"),
            true,
            now()
        ));
        assert!(store.get_check("bmw", "modland").is_none(), "промахи ушли");
        let bindings = store.bindings();
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].source, "stmods");
        assert!(bindings[0].confirmed);

        let pending = crate::UpdateCheckService::pending(&store, false);
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].latest_version.as_deref(), Some("3.3"));
    }

    #[test]
    fn undo_does_not_remove_a_newer_binding_on_same_source() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "a.zip"));
        let old = ModBinding {
            mod_id: "a".to_string(),
            source: "modsgaming".to_string(),
            source_id: "old".to_string(),
            url: Url::parse("https://modsgaming.test/old").unwrap(),
            confirmed: false,
        };
        store.upsert_binding(old.clone()).unwrap();
        store
            .upsert_binding(ModBinding {
                source_id: "new".to_string(),
                url: Url::parse("https://modsgaming.test/new").unwrap(),
                confirmed: true,
                ..old.clone()
            })
            .unwrap();
        assert!(!undo_page(&store, &old, now()));
        assert_eq!(store.bindings()[0].source_id, "new");
    }

    #[test]
    fn page_search_does_not_touch_removed_mod() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "toyota_sienna.zip"));
        let site = Site::new(|_| Ok(vec![listing("s", "Toyota Sienna")]));
        let search = search(site);
        let cancel = AtomicBool::new(false);
        // Мод «удаляют» между `due` и записью: здесь это видно по пустому хранилищу.
        store.remove_mod("a");
        let report = find_pages(&store, &search, now(), &cancel, &|_| None, &|_| None, None);
        assert_eq!(report.found, 0);
        assert!(store.bindings().is_empty());
    }

    #[test]
    fn check_state_is_untouched_by_page_actions() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "a.zip"));
        store
            .upsert_binding(ModBinding {
                mod_id: "a".to_string(),
                source: "modsgaming".to_string(),
                source_id: "1".to_string(),
                url: Url::parse("https://modsgaming.test/1").unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            mod_id: "a".to_string(),
            source: "modsgaming".to_string(),
            last_checked_utc: now(),
            last_changed_utc: None,
            last_version: Some("1.0".to_string()),
            last_size: None,
            miss_count: 0,
            last_game_version: None,
            last_title: None,
        });
        let suggestion = suggestion_for(&store, "a");
        assert!(
            suggestion.is_none(),
            "подтверждённая не считается предложением"
        );
        assert_eq!(suggestions(&store).len(), 0);
        assert!(store.get_check("a", "modsgaming").is_some());
    }

    /// Сайт, у которого в поиске нет размера, а на странице мода он есть.
    struct DetailSite {
        found: Listing,
        detail: Listing,
        detail_calls: AtomicUsize,
    }

    impl ModSource for DetailSite {
        fn id(&self) -> &'static str {
            "modsgaming"
        }
        fn display_name(&self) -> &'static str {
            "ModsGaming"
        }
        fn supports_search(&self) -> bool {
            true
        }
        fn home(&self) -> Url {
            Url::parse("https://modsgaming.test/").unwrap()
        }
        fn owns(&self, url: &Url) -> bool {
            url.host_str() == Some("modsgaming.test")
        }
        fn is_mod_page(&self, url: &Url) -> bool {
            self.owns(url) && url.path() != "/"
        }
        fn feed_covers(&self, _url: &Url) -> bool {
            true
        }
        fn search(&self, _query: &str, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(vec![self.found.clone()])
        }
        fn feed_page(&self, _page: u32, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }
        fn listing(&self, url: &Url, _cancel: &AtomicBool) -> Result<Option<Listing>, SourceError> {
            self.detail_calls.fetch_add(1, Ordering::SeqCst);
            Ok((url == &self.detail.url).then(|| self.detail.clone()))
        }
    }

    fn mercedes_case(site_size: i64) -> (InMemoryLibraryStore, PageSearchReport, usize) {
        // Как на живых модах: «MB_w164_ken_3.0» и страница «Mercedes-Benz ML 3.6» —
        // название совпало целиком, а файл на странице на треть меньше: это другой мод.
        let store = InMemoryLibraryStore::new();
        let mut mine = installed("mb", "MB_w164_ken_3.0_f4rza.zip");
        mine.version = None;
        mine.size_bytes = Some(455_163_953);
        store.upsert_mod(mine);
        let found = listing("ml36", "МОД Mercedes-Benz ML 3.6 (0.30.X) ДЛЯ BEAMNG DRIVE");
        let mut detail = found.clone();
        detail.size_bytes = Some(site_size);
        let site = Arc::new(DetailSite {
            found,
            detail,
            detail_calls: AtomicUsize::new(0),
        });
        let service = SearchService::new(
            vec![site.clone()],
            Arc::new(|_| SourceStatus::Ok),
            SearchOptions::default(),
            None,
        );
        let report = find_pages(
            &store,
            &service,
            now(),
            &AtomicBool::new(false),
            &|_| Some("Mercedes-Benz ML".to_string()),
            &|_| None,
            None,
        );
        (store, report, site.detail_calls.load(Ordering::SeqCst))
    }

    #[test]
    fn certain_guess_is_checked_against_size_on_the_mod_page() {
        let (store, report, calls) = mercedes_case(347_110_113);
        assert_eq!(calls, 1, "страницу мода открыли, чтобы узнать размер");
        assert_eq!(report.followed, 0, "{report:?}");
        assert!(
            store.bindings().iter().all(|binding| !binding.confirmed),
            "{:?}",
            store.bindings()
        );
    }

    #[test]
    fn certain_guess_with_matching_size_is_followed() {
        let (store, report, _) = mercedes_case(455_000_000);
        assert_eq!(report.followed, 1, "{report:?}");
        assert!(store.bindings()[0].confirmed);
    }

    fn followed_with_update(store: &InMemoryLibraryStore) {
        let mut mine = installed("mb", "MB_w164_ken_3.0_f4rza.zip");
        mine.version = None;
        mine.size_bytes = Some(455_163_953);
        store.upsert_mod(mine);
        store
            .upsert_binding(ModBinding {
                mod_id: "mb".to_string(),
                source: "modsgaming".to_string(),
                source_id: "132573".to_string(),
                url: Url::parse("https://modsgaming.test/132573").unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            mod_id: "mb".to_string(),
            source: "modsgaming".to_string(),
            last_checked_utc: now(),
            last_changed_utc: Some(now()),
            last_version: None,
            last_size: Some(347_110_113),
            miss_count: 0,
            last_game_version: None,
            last_title: Some("Mercedes-Benz ML 3.6".to_string()),
        });
    }

    #[test]
    fn unfollow_removes_wrong_page_and_its_update_then_undo_brings_all_back() {
        let store = InMemoryLibraryStore::new();
        followed_with_update(&store);
        store.set_dismissed_update("mb", Some("size:1"));
        assert_eq!(crate::UpdateCheckService::pending(&store, true).len(), 1);

        let saved = unfollow_page(&store, "mb", "modsgaming", now()).expect("следили");
        assert!(store.bindings().is_empty());
        assert!(store.get_check("mb", "modsgaming").is_none());
        assert!(crate::UpdateCheckService::pending(&store, true).is_empty());
        assert!(store.get_page_search("mb").unwrap().rejected);
        // Сам этот мод больше не ищется — ошибка не повторится.
        assert!(PageGuesser::due(&store, now() + TimeDelta::days(30)).is_empty());

        assert!(refollow_page(&store, &saved));
        assert_eq!(store.bindings(), vec![saved.binding.clone()]);
        assert_eq!(store.get_check("mb", "modsgaming"), saved.check);
        assert_eq!(store.get_dismissed_update("mb").as_deref(), Some("size:1"));
        assert_eq!(store.get_page_search("mb"), saved.search);
    }

    #[test]
    fn unfollow_needs_a_followed_page() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "a.zip"));
        assert!(unfollow_page(&store, "a", "modsgaming", now()).is_none());
        store
            .upsert_binding(ModBinding {
                mod_id: "a".to_string(),
                source: "modsgaming".to_string(),
                source_id: "1".to_string(),
                url: Url::parse("https://modsgaming.test/1").unwrap(),
                confirmed: false,
            })
            .unwrap();
        // Предложение снимается через «Это другой мод» для предложений, не здесь.
        assert!(unfollow_page(&store, "a", "modsgaming", now()).is_none());
        assert_eq!(store.bindings().len(), 1);
    }

    #[test]
    fn refollow_keeps_a_newer_page_on_the_same_site() {
        let store = InMemoryLibraryStore::new();
        followed_with_update(&store);
        let saved = unfollow_page(&store, "mb", "modsgaming", now()).unwrap();
        store
            .upsert_binding(ModBinding {
                mod_id: "mb".to_string(),
                source: "modsgaming".to_string(),
                source_id: "999".to_string(),
                url: Url::parse("https://modsgaming.test/999").unwrap(),
                confirmed: true,
            })
            .unwrap();
        assert!(!refollow_page(&store, &saved));
        assert_eq!(store.bindings()[0].source_id, "999");
        store.remove_mod("mb");
        assert!(!refollow_page(&store, &saved));
    }

    fn detail_site(detail: Listing) -> Arc<DetailSite> {
        Arc::new(DetailSite {
            found: detail.clone(),
            detail,
            detail_calls: AtomicUsize::new(0),
        })
    }

    #[test]
    fn manual_page_is_opened_and_followed_with_its_version() {
        let store = InMemoryLibraryStore::new();
        let mut mine = installed("mb", "MB_w164_ken_3.0_f4rza.zip");
        mine.version = Some("3.0".to_string());
        store.upsert_mod(mine);
        // Отказ «Это другой мод» не мешает указать страницу вручную.
        store.set_page_search(
            "mb",
            Some(PageSearch {
                at: now(),
                rejected: true,
            }),
        );
        let mut page = listing("w164", "Mercedes-Benz M-Class W164 3.0");
        page.mod_version = Some("3.0".to_string());
        page.size_bytes = Some(455_000_000);
        let site = detail_site(page.clone());
        let sources: Vec<Arc<dyn ModSource>> = vec![site.clone()];

        let got = bind_url(
            &store,
            &sources,
            "mb",
            " https://modsgaming.test/w164 ",
            &AtomicBool::new(false),
            now(),
        )
        .unwrap();
        assert_eq!(got.source_id, "w164");
        let bindings = store.bindings();
        assert_eq!(bindings.len(), 1);
        assert!(bindings[0].confirmed);
        let check = store
            .get_check("mb", "modsgaming")
            .expect("проверено сразу");
        assert_eq!(check.last_version.as_deref(), Some("3.0"));
        assert!(crate::UpdateCheckService::pending(&store, true).is_empty());
    }

    #[test]
    fn manual_page_explains_what_is_wrong() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "a.zip"));
        let site = detail_site(listing("real", "Some Car"));
        let sources: Vec<Arc<dyn ModSource>> = vec![site];
        let bind = |url: &str| {
            bind_url(&store, &sources, "a", url, &AtomicBool::new(false), now()).unwrap_err()
        };
        assert!(bind("не ссылка").contains("не ссылка"));
        assert!(bind("https://unknown.example/mod").contains("не знает"));
        assert!(bind("https://modsgaming.test/").contains("не страница мода"));
        assert!(bind("https://modsgaming.test/missing").contains("нет мода"));
        assert!(
            bind_url(
                &store,
                &[] as &[Arc<dyn ModSource>],
                "a",
                "https://modsgaming.test/real",
                &AtomicBool::new(false),
                now()
            )
            .is_err()
        );
        assert!(store.bindings().is_empty());
    }
}

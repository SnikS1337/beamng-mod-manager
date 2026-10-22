//! Подбор страниц модам без привязок и действия с предложенной страницей.
//!
//! «Нашли страницу» → привязка неподтверждённая; «Следить» подтверждает её,
//! «Это другой мод» отклоняет и запоминает отказ, «Отменить» возвращает
//! предложение. Всё это меняет несколько записей хранилища разом.

use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, FixedOffset};

use bmm_domain::library::{InstalledMod, LibraryStore, ModBinding, PageSearch};
use bmm_domain::models::Listing;
use bmm_sources::search::SearchService;

use crate::bindings::gate;
use crate::page_guesser::PageGuesser;
use crate::runner::UpdateProgress;

/// Итог подбора страниц.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PageSearchReport {
    /// Сколько модов получили предложенную страницу.
    pub found: usize,
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
    mut progress: Option<&mut dyn FnMut(&UpdateProgress)>,
) -> PageSearchReport {
    let due = PageGuesser::due(store, now);
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
        let query = PageGuesser::query_for(installed, title.as_deref());
        let response = search.search(&query, cancel, None);
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        // Ни один сайт не ответил: не считаем, что искали, — попробуем при следующей проверке.
        if !response
            .sources
            .iter()
            .any(|source| source.status == bmm_sources::sources::SourceStatus::Ok)
        {
            continue;
        }

        let listings: Vec<Listing> = if response.listings.is_empty() {
            response
                .cards
                .into_iter()
                .flat_map(|card| card.listings)
                .collect()
        } else {
            response.listings
        };
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
        if let Some(guess) = PageGuesser::pick(installed, title.as_deref(), &listings) {
            let binding = ModBinding {
                mod_id: installed.id.clone(),
                source: guess.listing.source,
                source_id: guess.listing.source_id,
                url: guess.listing.url,
                confirmed: false,
            };
            if store.upsert_binding(binding).is_ok() {
                result.found += 1;
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

    #[test]
    fn find_pages_suggests_once_then_confirm_reject_undo() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "toyota_sienna_2017_v1.0.zip"));
        store.upsert_mod(installed("b", "lada_niva_4x4.zip"));
        let site = Site::new(|query| {
            if query.to_lowercase().contains("sienna") {
                Ok(vec![listing(
                    "s",
                    "МОД Toyota Sienna 2017 v2.0 (0.34.x) ДЛЯ BEAMNG.DRIVE",
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
        let report = find_pages(&store, &search, now(), &cancel, &|_| None, None);
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
        let report = find_pages(&store, &search, now(), &cancel, &|_| None, None);
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
        let report = find_pages(&store, &search, now(), &cancel, &|_| None, None);
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
        let report = find_pages(&store, &search, now(), &cancel, &|_| None, None);
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
}

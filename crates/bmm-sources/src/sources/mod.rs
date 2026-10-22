//! Источники модов: общий контракт и загрузка страниц.

pub mod beamng;
pub mod common;
pub mod modland;
pub mod modsgaming;
pub mod stmods;

use std::sync::atomic::AtomicBool;

use url::Url;

use bmm_domain::models::{Listing, Offer};

pub use crate::net::{FetchResult, PageFetcher, PoliteFetcher, SourceStatus};

#[derive(Debug, Clone, thiserror::Error)]
#[error("{message}")]
pub struct SourceError {
    pub status: SourceStatus,
    pub message: String,
}

impl SourceError {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            status: SourceStatus::Unavailable,
            message: message.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self {
            status: SourceStatus::Unavailable,
            message: "поиск отменён".to_string(),
        }
    }
}

pub trait ModSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn supports_search(&self) -> bool;
    fn home(&self) -> Url;
    fn owns(&self, url: &Url) -> bool;
    fn is_mod_page(&self, url: &Url) -> bool;
    fn feed_covers(&self, url: &Url) -> bool;
    fn search(&self, query: &str, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError>;
    fn feed_page(&self, page: u32, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError>;
    fn listing(&self, url: &Url, cancel: &AtomicBool) -> Result<Option<Listing>, SourceError>;

    fn offers(&self, listing: &Listing, cancel: &AtomicBool) -> Result<Vec<Offer>, SourceError> {
        if !listing.offers.is_empty() {
            return Ok(listing.offers.clone());
        }
        Ok(self
            .listing(&listing.url, cancel)?
            .map(|item| item.offers)
            .unwrap_or_default())
    }
}

pub fn all_sources(fetcher: std::sync::Arc<PoliteFetcher>) -> Vec<std::sync::Arc<dyn ModSource>> {
    vec![
        std::sync::Arc::new(modland::ModlandSource::new(fetcher.clone())),
        std::sync::Arc::new(stmods::StModsSource::new(fetcher.clone())),
        std::sync::Arc::new(modsgaming::ModsGamingSource::new(fetcher.clone())),
        std::sync::Arc::new(beamng::BeamNgRepoSource::new(fetcher)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use url::Url;

    use crate::net::{PoliteOptions, RawResponse};

    struct FakeFetcher {
        calls: Mutex<Vec<String>>,
    }

    impl FakeFetcher {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
            })
        }
    }

    fn fixture(parts: &[&str]) -> String {
        let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("../../fixtures");
        for part in parts {
            path.push(part);
        }
        std::fs::read_to_string(path).unwrap()
    }

    impl PageFetcher for FakeFetcher {
        fn get(&self, url: &Url) -> Result<RawResponse, crate::net::FetchError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("GET {}", url.path()));
            let host = url.host_str().unwrap_or("");
            let text = if host.ends_with("modland.net") {
                if url.path().starts_with("/search/") {
                    fixture(&["modland", "search-real.html"])
                } else {
                    fixture(&["modland", "list-real.html"])
                }
            } else if host.ends_with("modsgaming.ru") && url.path().starts_with("/search/") {
                fixture(&["modsgaming", "search-real.html"])
            } else if host.ends_with("stmods.org") && url.path() == "/search.php" {
                fixture(&["stmods", "search-beamng-2026-10-18.html"])
            } else if url.path().starts_with("/beamng_drive/") && url.path().contains("mods/") {
                fixture(&["stmods", "mod-real.html"])
            } else if url.path().starts_with("/beamng_drive") {
                fixture(&["stmods", "list-real.html"])
            } else if url.path().starts_with("/load/0-0-1")
                || url.path() == "/load/beamng_drive/mashiny/38-1-0-160435"
            {
                fixture(&["modsgaming", "mod-page.html"])
            } else if url.path().starts_with("/load/beamng_drive") {
                fixture(&["modsgaming", "list-page.html"])
            } else if url.path().starts_with("/resources") {
                fixture(&["beamng", "list-real.html"])
            } else {
                fixture(&["modland", "list-real.html"])
            };
            Ok(RawResponse {
                status: 200,
                text,
                final_url: url.clone(),
                retry_after: None,
            })
        }

        fn post_form(
            &self,
            url: &Url,
            form: &[(&str, &str)],
        ) -> Result<RawResponse, crate::net::FetchError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("POST {} {:?}", url.path(), form));
            let text = if url.path() == "/search.php" {
                fixture(&["stmods", "search-beamng-2026-10-18.html"])
            } else {
                fixture(&["modsgaming", "list-page.html"])
            };
            Ok(RawResponse {
                status: 200,
                text,
                final_url: url.clone(),
                retry_after: None,
            })
        }
    }

    fn polite(fake: Arc<dyn PageFetcher>) -> Arc<PoliteFetcher> {
        Arc::new(PoliteFetcher::new(
            fake,
            PoliteOptions {
                pause: std::time::Duration::ZERO,
                jitter: std::time::Duration::ZERO,
                retries: 0,
                backoff: std::time::Duration::ZERO,
                max_backoff: std::time::Duration::ZERO,
                cache_ttl: std::time::Duration::from_secs(60),
                cache_capacity: 16,
                failure_limit: 3,
                cooldown: std::time::Duration::from_secs(60),
            },
        ))
    }

    /// Отдаёт одну и ту же страницу на любой адрес: удобно для заглушек и «редизайна».
    struct FixedFetcher {
        text: String,
    }

    impl PageFetcher for FixedFetcher {
        fn get(&self, url: &Url) -> Result<RawResponse, crate::net::FetchError> {
            Ok(RawResponse {
                status: 200,
                text: self.text.clone(),
                final_url: url.clone(),
                retry_after: None,
            })
        }
    }

    /// Отвечает по адресу: `None` — 404. Считает запросы.
    type Route = Box<dyn Fn(&Url) -> Option<String> + Send + Sync>;

    struct RouteFetcher {
        route: Route,
        calls: Mutex<Vec<String>>,
    }

    impl RouteFetcher {
        fn new<F>(route: F) -> Arc<Self>
        where
            F: Fn(&Url) -> Option<String> + Send + Sync + 'static,
        {
            Arc::new(Self {
                route: Box::new(route),
                calls: Mutex::new(Vec::new()),
            })
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl PageFetcher for RouteFetcher {
        fn get(&self, url: &Url) -> Result<RawResponse, crate::net::FetchError> {
            self.calls.lock().unwrap().push(url.to_string());
            match (self.route)(url) {
                Some(text) => Ok(RawResponse {
                    status: 200,
                    text,
                    final_url: url.clone(),
                    retry_after: None,
                }),
                None => Ok(RawResponse {
                    status: 404,
                    text: String::new(),
                    final_url: url.clone(),
                    retry_after: None,
                }),
            }
        }
    }

    fn cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn modland_search_and_feed() {
        let fake = FakeFetcher::new();
        let source = modland::ModlandSource::new(polite(fake.clone()));
        let found = source.search("sienna", &cancel()).unwrap();
        assert_eq!(found.len(), 5);
        assert!(found.iter().all(|item| source.is_mod_page(&item.url)));
        let feed = source.feed_page(1, &cancel()).unwrap();
        assert_eq!(feed.len(), 7);
        let calls = fake.calls.lock().unwrap();
        assert!(calls[0].starts_with("GET /search/sienna"), "{calls:?}");
    }

    #[test]
    fn stmods_search_posts_form() {
        let fake = FakeFetcher::new();
        let source = stmods::StModsSource::new(polite(fake.clone()));
        let found = source.search("bmw", &cancel()).unwrap();
        assert_eq!(found.len(), 2);
        let calls = fake.calls.lock().unwrap();
        assert!(calls[0].starts_with("POST /search.php"));
        assert!(calls[0].contains("per_page_search"));
        assert!(calls[0].contains("date_public"));
    }

    #[test]
    fn beamng_feed_and_mod_page() {
        let fake = FakeFetcher::new();
        let source = beamng::BeamNgRepoSource::new(polite(fake));
        let feed = source.feed_page(1, &cancel()).unwrap();
        assert_eq!(feed.len(), 4);
        let url = Url::parse("https://beamng.com/resources/acng-tire-heat.39760/").unwrap();
        assert!(source.is_mod_page(&url));
        assert!(
            !source
                .is_mod_page(&Url::parse("https://beamng.com/resources/categories/x.1/").unwrap())
        );
    }

    #[test]
    fn modsgaming_listing_and_feed_filter() {
        let fake = FakeFetcher::new();
        let source = modsgaming::ModsGamingSource::new(polite(fake));
        let feed = source.feed_page(1, &cancel()).unwrap();
        assert_eq!(feed.len(), 2);
        let url =
            Url::parse("https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-160435").unwrap();
        let detail = source.listing(&url, &cancel()).unwrap().unwrap();
        assert_eq!(detail.author.as_deref(), Some("Infiniti"));
    }

    #[test]
    fn modsgaming_search_combines_two_queries() {
        let fake = FakeFetcher::new();
        let source = modsgaming::ModsGamingSource::new(polite(fake.clone()));
        let found = source.search("sienna", &cancel()).unwrap();
        assert_eq!(found.len(), 9);
        assert!(found.iter().all(|item| source.is_mod_page(&item.url)));
        assert!(fake.calls.lock().unwrap().len() >= 2);
    }

    fn all_sources(fetcher: Arc<PoliteFetcher>) -> Vec<Arc<dyn ModSource>> {
        vec![
            Arc::new(modland::ModlandSource::new(fetcher.clone())),
            Arc::new(stmods::StModsSource::new(fetcher.clone())),
            Arc::new(modsgaming::ModsGamingSource::new(fetcher.clone())),
            Arc::new(beamng::BeamNgRepoSource::new(fetcher)),
        ]
    }

    #[test]
    fn is_mod_page_matrix_all_four_sites() {
        let fake = FakeFetcher::new();
        let sources = all_sources(polite(fake));
        let mods = [
            "https://www.modland.net/beamng.drive-mods/cars/toyota-sienna-4.html",
            "https://modland.net/beamng.drive-mods/maps/paid-duskport-realistic-city-map.html",
            "https://www.modland.net/beamng.drive-mods/2011-2017-toyota-sienna.html",
            "http://www.modland.net/BeamNG.drive-mods/cars/x.html",
            "https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/",
            "https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng",
            "https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-160435",
            "https://modsgaming.ru/load/beamng_drive/mashiny/mod_vaz_2106_v6_0_0_38_x_dlja_beamng_drive",
            "https://modsgaming.ru/load/beamng_drive/karty/mod_karta_domik_v_derevne_v1_0_0_38_x_dlja_beamng_drive",
            "https://beamng.com/resources/acng-tire-heat-pressure-wear.34567/",
            "https://www.beamng.com/resources/acng.34567/updates",
        ];
        for raw in mods {
            let url = Url::parse(raw).unwrap();
            let owners: Vec<&str> = sources
                .iter()
                .filter(|source| source.is_mod_page(&url))
                .map(|source| source.id())
                .collect();
            assert_eq!(owners.len(), 1, "{raw}: {owners:?}");
        }

        let sections = [
            "https://www.modland.net/",
            "https://www.modland.net/beamng.drive-mods",
            "https://www.modland.net/beamng.drive-mods/cars",
            "https://www.modland.net/beamng.drive-mods/page/2",
            "https://www.modland.net/beamng.drive-mods/version/039x",
            "https://www.modland.net/search/sienna",
            "https://www.modland.net/user/rudd",
            "https://www.modland.net/news",
            "https://www.modland.net/gta-5-mods/vehicles/cars/2017-toyota-sienna-xle-premium-10-replaceadd-on.html",
            "https://www.modland.net/beamng.drive-mods/.html",
            "https://stmods.org/",
            "https://stmods.org/beamng_drive/",
            "https://stmods.org/beamng_drive/mods/",
            "https://stmods.org/search.php?search_request=x",
            "https://stmods.org/x/",
            "https://modsgaming.ru/",
            "https://modsgaming.ru/load/beamng_drive/mashiny/38",
            "https://modsgaming.ru/load/beamng_drive/mashiny/38-2",
            "https://modsgaming.ru/load/beamng_drive/mashiny",
            "https://modsgaming.ru/search/?q=sienna&t=0",
            "https://modsgaming.ru/load/farming_simulator/traktory/mod_x",
            "https://beamng.com/",
            "https://beamng.com/resources/",
            "https://beamng.com/resources/categories/vehicles.2/",
            "https://beamng.com/forums/x.12/",
            "https://evil.example/beamng_drive/mods/x/",
            "ftp://stmods.org/beamng_drive/mods/x/",
        ];
        for raw in sections {
            let url = Url::parse(raw).unwrap();
            assert!(
                sources.iter().all(|source| !source.is_mod_page(&url)),
                "{raw}"
            );
        }
    }

    #[test]
    fn read_listing_rejects_feeds_stubs_and_non_mod_urls() {
        let st_url =
            Url::parse("https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/")
                .unwrap();
        let st_feed = Url::parse("https://stmods.org/beamng_drive/").unwrap();
        let mg_url =
            Url::parse("https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-160435").unwrap();
        let ng_url = Url::parse("https://beamng.com/resources/acng.34567/").unwrap();

        let st = stmods::StModsSource::new(polite(FakeFetcher::new()));
        let page = st.listing(&st_url, &cancel()).unwrap().unwrap();
        assert_eq!(page.mod_version.as_deref(), Some("3.3"));
        assert!(
            st.listing(&st_feed, &cancel()).unwrap().is_none(),
            "ссылка на раздел не открывается как карточка"
        );

        let stubs = [
            String::new(),
            "<html><body></body></html>".to_string(),
            "<html><head><title>Сайт на обслуживании</title></head><body><h1>Скоро вернёмся</h1></body></html>".to_string(),
            "<h1>404 — страница не найдена</h1>".to_string(),
            fixture(&["stmods", "list-real.html"]),
            fixture(&["modsgaming", "list-real.html"]),
            fixture(&["beamng", "list-real.html"]),
        ];
        for stub in stubs {
            let page = Arc::new(FixedFetcher { text: stub.clone() });
            let st = stmods::StModsSource::new(polite(page.clone()));
            assert!(
                st.listing(&st_url, &cancel()).unwrap().is_none(),
                "STMods: заглушка не читается ({} символов)",
                stub.len()
            );
            let mg = modsgaming::ModsGamingSource::new(polite(page.clone()));
            assert!(
                mg.listing(&mg_url, &cancel()).unwrap().is_none(),
                "ModsGaming: заглушка не читается ({} символов)",
                stub.len()
            );
            let ng = beamng::BeamNgRepoSource::new(polite(page));
            assert!(
                ng.listing(&ng_url, &cancel()).unwrap().is_none(),
                "BeamNG: заглушка не читается ({} символов)",
                stub.len()
            );
        }
    }

    #[test]
    fn read_listing_does_not_fetch_sections() {
        let fake = FakeFetcher::new();
        let st = stmods::StModsSource::new(polite(fake.clone()));
        let feed = Url::parse("https://stmods.org/beamng_drive/").unwrap();
        assert!(st.listing(&feed, &cancel()).unwrap().is_none());
        let search = Url::parse("https://stmods.org/search.php?search_request=x").unwrap();
        assert!(st.listing(&search, &cancel()).unwrap().is_none());
        let foreign = Url::parse("https://evil.example/beamng_drive/mods/x/").unwrap();
        assert!(st.listing(&foreign, &cancel()).unwrap().is_none());
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn stmods_redesign_degrades_to_not_readable() {
        let html = fixture(&["stmods", "mod-real.html"]);
        let url =
            Url::parse("https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/")
                .unwrap();
        let card = |html: String| {
            let source = stmods::StModsSource::new(polite(Arc::new(FixedFetcher { text: html })));
            source.listing(&url, &cancel()).unwrap()
        };

        let no_version = card(html.replace(
            "Мод BMW E34 версия 3.3 для BeamNG.drive (v0.39.x)",
            "BMW E34",
        ))
        .expect("размер из кнопки остаётся");
        assert!(no_version.mod_version.is_none());
        assert_eq!(no_version.size_bytes, Some(223_053_086));

        let no_button = card(
            html.replace("Скачать мод (", "Download (")
                .replace("download_file.php", "get.php"),
        )
        .expect("версия остаётся");
        assert_eq!(no_button.mod_version.as_deref(), Some("3.3"));
        assert!(no_button.offers.is_empty());

        let both = html
            .replace(
                "Мод BMW E34 версия 3.3 для BeamNG.drive (v0.39.x)",
                "BMW E34",
            )
            .replace("Скачать мод (", "Download (")
            .replace("download_file.php", "get.php");
        assert_eq!(common::page_title(&both), "BMW E34");
        assert!(
            card(both).is_none(),
            "без версии и файла страница не читается"
        );
    }

    #[test]
    fn modsgaming_real_search_keeps_everything_and_drops_typos() {
        let html = fixture(&["modsgaming", "search-real.html"]);
        let all = crate::parsers::modsgaming::parse_search(&html).len();
        assert!(all > 0);

        let full = html.clone();
        let every = modsgaming::ModsGamingSource::new(polite(RouteFetcher::new(move |_| {
            Some(full.clone())
        })));
        assert_eq!(every.search("sienna", &cancel()).unwrap().len(), all);

        // Опечатка: как есть сайт ничего не нашёл, с «beamng.drive» — всё
        // подряд по «beamng»; из широкой выдачи не берём ничего.
        let wide_html = html.clone();
        let routes = RouteFetcher::new(move |url| {
            let wide = url
                .query_pairs()
                .any(|(key, value)| key == "q" && value.contains("beamng.drive"));
            Some(if wide {
                wide_html.clone()
            } else {
                String::new()
            })
        });
        let exact_empty =
            modsgaming::ModsGamingSource::new(polite(Arc::clone(&routes) as Arc<dyn PageFetcher>));
        assert_eq!(exact_empty.search("bmv", &cancel()).unwrap().len(), 0);
        let calls = routes.calls();
        assert!(calls.len() >= 2, "точный и широкий запросы: {calls:?}");
        assert!(calls[0].contains("q=bmv&t=0"), "{calls:?}");
        assert!(
            calls.iter().any(|call| call.contains("beamng.drive")),
            "{calls:?}"
        );

        // Слово только в описании (TAA): запрос как есть находит, широкая пуста.
        let exact_html = html.clone();
        let wide_empty = modsgaming::ModsGamingSource::new(polite(RouteFetcher::new(move |url| {
            let wide = url
                .query_pairs()
                .any(|(key, value)| key == "q" && value.contains("beamng.drive"));
            Some(if wide {
                String::new()
            } else {
                exact_html.clone()
            })
        })));
        assert_eq!(wide_empty.search("TAA", &cancel()).unwrap().len(), all);
    }
}

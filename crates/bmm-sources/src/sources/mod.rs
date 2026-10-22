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
            } else if url.path().starts_with("/load/0-0-1") {
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

    fn polite(fake: Arc<FakeFetcher>) -> Arc<PoliteFetcher> {
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
        let url = Url::parse("https://modsgaming.ru/load/0-0-1-160435-20").unwrap();
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
}

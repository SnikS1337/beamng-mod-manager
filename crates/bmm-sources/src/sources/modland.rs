//! Адаптер Modland.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock};

use url::Url;

use bmm_domain::models::Listing;
use bmm_domain::text::ModKind;

use crate::net::PoliteFetcher;
use crate::parsers::modland as parser;

use super::common;
use super::{ModSource, SourceError};

static ROOT: LazyLock<Url> = LazyLock::new(|| Url::parse(parser::BASE).unwrap());

pub struct ModlandSource {
    fetcher: Arc<PoliteFetcher>,
}

impl ModlandSource {
    pub fn new(fetcher: Arc<PoliteFetcher>) -> Self {
        Self { fetcher }
    }

    fn search_url(query: &str) -> Option<Url> {
        if query.trim_matches(['.', ' ']).is_empty() {
            return None;
        }
        let mut url = ROOT.join("search").ok()?;
        url.path_segments_mut().ok()?.push(query);
        Some(url)
    }

    fn feed_url(page: u32) -> Url {
        if page <= 1 {
            ROOT.join("beamng.drive-mods").unwrap()
        } else {
            ROOT.join(&format!("beamng.drive-mods/page/{page}"))
                .unwrap()
        }
    }

    fn browse_url(kind: ModKind, page: u32) -> Url {
        let section = match kind {
            ModKind::Vehicle => "cars",
            ModKind::Map => "maps",
            ModKind::Addon => "other",
        };
        if page <= 1 {
            ROOT.join(&format!("beamng.drive-mods/{section}")).unwrap()
        } else {
            ROOT.join(&format!("beamng.drive-mods/{section}/page/{page}"))
                .unwrap()
        }
    }
}

impl ModSource for ModlandSource {
    fn id(&self) -> &'static str {
        parser::SOURCE_ID
    }

    fn display_name(&self) -> &'static str {
        "Modland"
    }

    fn supports_search(&self) -> bool {
        true
    }

    fn home(&self) -> Url {
        ROOT.clone()
    }

    fn owns(&self, url: &Url) -> bool {
        common::belongs_to(&ROOT, url)
    }

    fn is_mod_page(&self, url: &Url) -> bool {
        if !self.owns(url) {
            return false;
        }
        let parts = common::segments(url);
        parts.len() >= 2
            && parts[0].eq_ignore_ascii_case("beamng.drive-mods")
            && parts.last().is_some_and(|last| {
                last.to_lowercase().ends_with(".html") && last.len() > ".html".len()
            })
    }

    fn feed_covers(&self, _url: &Url) -> bool {
        true
    }

    fn search(&self, query: &str, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
        let Some(url) = Self::search_url(query) else {
            return Ok(Vec::new());
        };
        common::read_search(
            &self.fetcher,
            self.id(),
            &url,
            |_page| None,
            1,
            None,
            parser::parse_list,
            &|url| self.is_mod_page(url),
            cancel,
        )
    }

    fn feed_page(&self, page: u32, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
        let url = Self::feed_url(page.max(1));
        let html = common::load(&self.fetcher, self.id(), &url, None, cancel)?;
        Ok(html
            .map(|html| parser::parse_list(&html))
            .unwrap_or_default())
    }

    fn supports_browse(&self, _kind: ModKind) -> bool {
        true
    }

    fn browse(
        &self,
        kind: ModKind,
        page: u32,
        cancel: &AtomicBool,
    ) -> Result<Vec<Listing>, SourceError> {
        let url = Self::browse_url(kind, page.max(1));
        let html = common::load(&self.fetcher, self.id(), &url, None, cancel)?;
        let (wanted, category) = match kind {
            ModKind::Vehicle => ("cars", "cars"),
            ModKind::Map => ("maps", "maps"),
            ModKind::Addon => ("other", "addons"),
        };
        Ok(html
            .map(|html| {
                let mut found: Vec<Listing> = parser::parse_list(&html)
                    .into_iter()
                    // На странице раздела бывают карточки из других разделов:
                    // у каждой ссылка ведёт в свой раздел, ей и верим.
                    .filter(|item| {
                        parser::category_slug(&item.url).is_none_or(|section| section == wanted)
                    })
                    .collect();
                for item in &mut found {
                    item.category = Some(category.to_string());
                }
                found
            })
            .unwrap_or_default())
    }

    fn listing(&self, url: &Url, cancel: &AtomicBool) -> Result<Option<Listing>, SourceError> {
        common::read_listing(
            &self.fetcher,
            self.id(),
            url,
            &|url| self.owns(url),
            &|url| self.is_mod_page(url),
            &parser::slug_of,
            &parser::parse_detail,
            cancel,
        )
    }
}

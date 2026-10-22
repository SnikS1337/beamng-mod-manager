//! Адаптер ModsGaming: две выдачи поиска и лента раздела «Машины».

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock};

use url::Url;

use bmm_domain::models::Listing;

use crate::net::PoliteFetcher;
use crate::parsers::modsgaming as parser;
use crate::search::relevance;

use super::common;
use super::{ModSource, SourceError};

static ROOT: LazyLock<Url> = LazyLock::new(|| Url::parse(parser::BASE).unwrap());

pub struct ModsGamingSource {
    fetcher: Arc<PoliteFetcher>,
}

impl ModsGamingSource {
    pub fn new(fetcher: Arc<PoliteFetcher>) -> Self {
        Self { fetcher }
    }

    fn feed_url(page: u32) -> Url {
        if page <= 1 {
            ROOT.join("load/beamng_drive/mashiny/38").unwrap()
        } else {
            ROOT.join(&format!("load/beamng_drive/mashiny/38-{page}"))
                .unwrap()
        }
    }

    fn page_url(query: &str, page: u32) -> Option<Url> {
        let mut url = ROOT.join("search/").ok()?;
        {
            let mut pairs = url.query_pairs_mut();
            pairs.append_pair("q", query).append_pair("t", "0");
            if page > 1 {
                pairs.append_pair("p", &page.to_string());
            }
        }
        Some(url)
    }

    fn for_beamng(query: &str) -> String {
        if query.to_lowercase().contains("beamng") {
            query.to_string()
        } else {
            format!("{} beamng.drive", query.trim())
        }
    }

    fn read(
        &self,
        query: &str,
        page: u32,
        pages: u32,
        cancel: &AtomicBool,
    ) -> Result<Vec<Listing>, SourceError> {
        let Some(first) = Self::page_url(query, 1) else {
            return Ok(Vec::new());
        };
        common::read_search(
            &self.fetcher,
            self.id(),
            &first,
            |page| Self::page_url(query, page),
            pages.max(page),
            None,
            parser::parse_search,
            &|url| self.is_mod_page(url),
            cancel,
        )
    }
}

impl ModSource for ModsGamingSource {
    fn id(&self) -> &'static str {
        parser::SOURCE_ID
    }

    fn display_name(&self) -> &'static str {
        "ModsGaming"
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
        if parts.len() < 4
            || !parts[0].eq_ignore_ascii_case("load")
            || !parts[1].eq_ignore_ascii_case("beamng_drive")
        {
            return false;
        }
        let last = parts.last().copied().unwrap_or("");
        !is_page_number(last)
    }

    fn feed_covers(&self, url: &Url) -> bool {
        url.path()
            .to_lowercase()
            .starts_with("/load/beamng_drive/mashiny/")
    }

    fn search(&self, query: &str, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        if query.to_lowercase().contains("beamng") {
            return self.read(query, 1, 2, cancel);
        }
        let exact = self.read(query, 1, 1, cancel)?;
        let mut wide = match self.read(&Self::for_beamng(query), 1, 2, cancel) {
            Ok(found) => relevance::filter(query, found, &["beamng", "drive", "beamng.drive"]),
            Err(error) if exact.is_empty() => return Err(error),
            Err(_) => Vec::new(),
        };
        for item in exact {
            if wide
                .iter()
                .all(|existing| existing.source_id != item.source_id)
            {
                wide.push(item);
            }
        }
        Ok(wide)
    }

    fn feed_page(&self, page: u32, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
        let url = Self::feed_url(page.max(1));
        let html = common::load(&self.fetcher, self.id(), &url, None, cancel)?;
        Ok(html
            .map(|html| parser::parse_list(&html))
            .unwrap_or_default())
    }

    fn listing(&self, url: &Url, cancel: &AtomicBool) -> Result<Option<Listing>, SourceError> {
        common::read_listing(
            &self.fetcher,
            self.id(),
            url,
            &|url| self.owns(url),
            &|url| self.is_mod_page(url),
            &slug_of,
            &parser::parse_detail,
            cancel,
        )
    }
}

fn is_page_number(last: &str) -> bool {
    let parts: Vec<&str> = last.split('-').collect();
    if parts.is_empty() || parts.len() > 2 {
        return false;
    }
    parts
        .iter()
        .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
}

fn slug_of(url: &Url) -> String {
    let last = url
        .path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    last.rsplit_once('-')
        .filter(|(_, id)| !id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit()))
        .map(|(_, id)| id.to_string())
        .unwrap_or_else(|| last.to_string())
}

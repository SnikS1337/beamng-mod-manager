//! Адаптер STMods.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock};

use url::Url;

use bmm_domain::models::Listing;

use crate::net::PoliteFetcher;
use crate::parsers::stmods as parser;

use super::common;
use super::{ModSource, SourceError};

static ROOT: LazyLock<Url> = LazyLock::new(|| Url::parse(parser::BASE).unwrap());
const SEARCH_FORM: [(&str, &str); 2] = [
    ("per_page_search", "50"),
    ("search_order_by", "`date_public` DESC"),
];

pub struct StModsSource {
    fetcher: Arc<PoliteFetcher>,
}

impl StModsSource {
    pub fn new(fetcher: Arc<PoliteFetcher>) -> Self {
        Self { fetcher }
    }

    fn search_url(query: &str) -> Option<Url> {
        let mut url = ROOT.join("search.php").ok()?;
        url.query_pairs_mut()
            .append_pair("search_request", query)
            .append_pair("partition", "beamng");
        Some(url)
    }

    fn feed_url(page: u32) -> Url {
        if page <= 1 {
            ROOT.join("beamng_drive/").unwrap()
        } else {
            let mut url = ROOT.join("beamng_drive/").unwrap();
            url.query_pairs_mut().append_pair("page", &page.to_string());
            url
        }
    }
}

impl ModSource for StModsSource {
    fn id(&self) -> &'static str {
        parser::SOURCE_ID
    }

    fn display_name(&self) -> &'static str {
        "STMods"
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
        parts.len() >= 3 && parts[0].eq_ignore_ascii_case("beamng_drive")
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
            Some(&SEARCH_FORM),
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

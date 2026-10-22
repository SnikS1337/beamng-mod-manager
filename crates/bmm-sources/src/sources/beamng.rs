//! Адаптер официального репозитория beamng.com/resources.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock};

use url::Url;

use bmm_domain::models::Listing;

use crate::net::PoliteFetcher;
use crate::parsers::beamng as parser;

use super::common;
use super::{ModSource, SourceError};

static ROOT: LazyLock<Url> = LazyLock::new(|| Url::parse(parser::BASE).unwrap());

pub struct BeamNgRepoSource {
    fetcher: Arc<PoliteFetcher>,
}

impl BeamNgRepoSource {
    pub fn new(fetcher: Arc<PoliteFetcher>) -> Self {
        Self { fetcher }
    }

    fn feed_url(page: u32) -> Url {
        let mut url = ROOT.join("resources/").unwrap();
        if page > 1 {
            url.query_pairs_mut().append_pair("page", &page.to_string());
        }
        url
    }
}

impl ModSource for BeamNgRepoSource {
    fn id(&self) -> &'static str {
        parser::SOURCE_ID
    }

    fn display_name(&self) -> &'static str {
        "BeamNG (официальный)"
    }

    fn supports_search(&self) -> bool {
        false
    }

    fn home(&self) -> Url {
        ROOT.clone()
    }

    fn owns(&self, url: &Url) -> bool {
        common::belongs_to(&ROOT, url)
    }

    fn is_mod_page(&self, url: &Url) -> bool {
        let parts = common::segments(url);
        parts.len() >= 2 && parts[0].eq_ignore_ascii_case("resources") && is_resource_slug(parts[1])
    }

    fn feed_covers(&self, _url: &Url) -> bool {
        true
    }

    fn search(&self, _query: &str, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
        Ok(Vec::new())
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
            &parser::slug_of,
            &parser::parse_detail,
            cancel,
        )
    }
}

fn is_resource_slug(slug: &str) -> bool {
    let Some((_, id)) = slug.rsplit_once('.') else {
        return false;
    };
    !id.is_empty() && id.chars().all(|ch| ch.is_ascii_digit())
}

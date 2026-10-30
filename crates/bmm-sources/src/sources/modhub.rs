//! Адаптер ModHub (modhub.us).
//!
//! Сайт закрыт Cloudflare, как и Modland: страницы грузит скрытый WebView2, а
//! проверку «вы человек?» проходит человек в отдельном окне.

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock};

use url::Url;

use bmm_domain::models::Listing;
use bmm_domain::text::ModKind;

use crate::net::PoliteFetcher;
use crate::parsers::modhub as parser;

use super::common;
use super::{ModSource, SourceError};

static ROOT: LazyLock<Url> = LazyLock::new(|| Url::parse(parser::BASE).unwrap());
/// Страниц поиска за раз. Сайт грузится скрытым браузером по одной странице, а
/// подходящий мод почти всегда на первой: вторая удваивала время поиска страниц.
const SEARCH_PAGES: u32 = 1;

pub struct ModHubSource {
    fetcher: Arc<PoliteFetcher>,
}

impl ModHubSource {
    pub fn new(fetcher: Arc<PoliteFetcher>) -> Self {
        Self { fetcher }
    }

    /// `/search/{запрос}/beamng-drive-mods` — поиск сразу только по BeamNG.
    pub fn search_url(query: &str, page: u32) -> Option<Url> {
        let query = query.trim();
        if query.trim_matches(['.', ' ']).is_empty() {
            return None;
        }
        let mut url = ROOT.join("search/").ok()?;
        url.path_segments_mut()
            .ok()?
            .pop_if_empty()
            .push(query)
            .push(parser::SECTION);
        if page > 1 {
            url.query_pairs_mut().append_pair("page", &page.to_string());
        }
        Some(url)
    }

    /// Лента новых: раздел BeamNG по умолчанию отсортирован от новых к старым.
    fn feed_url(page: u32) -> Url {
        if page <= 1 {
            ROOT.join(&format!("category/{}", parser::SECTION)).unwrap()
        } else {
            ROOT.join(&format!("category/{}/page/{page}", parser::SECTION))
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
            ROOT.join(&format!("category/{}/{section}", parser::SECTION))
                .unwrap()
        } else {
            ROOT.join(&format!(
                "category/{}/{section}/page/{page}",
                parser::SECTION
            ))
            .unwrap()
        }
    }
}

impl ModSource for ModHubSource {
    fn id(&self) -> &'static str {
        parser::SOURCE_ID
    }

    fn display_name(&self) -> &'static str {
        "ModHub"
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
        parser::is_mod_url(url)
    }

    fn feed_covers(&self, url: &Url) -> bool {
        parser::is_mod_url(url)
    }

    fn search(&self, query: &str, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
        let Some(url) = Self::search_url(query, 1) else {
            return Ok(Vec::new());
        };
        common::read_search(
            &self.fetcher,
            self.id(),
            &url,
            |page| Self::search_url(query, page),
            SEARCH_PAGES,
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
        let category = match kind {
            ModKind::Vehicle => "cars",
            ModKind::Map => "maps",
            ModKind::Addon => "addons",
        };
        Ok(html
            .map(|html| {
                let mut found = parser::parse_list(&html);
                // В карточке раздела нет: раздел — это страница, которую открыли.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_is_limited_to_beamng() {
        assert_eq!(
            ModHubSource::search_url("bmw m5", 1).unwrap().as_str(),
            "https://www.modhub.us/search/bmw%20m5/beamng-drive-mods"
        );
        assert_eq!(
            ModHubSource::search_url("bmw", 2).unwrap().as_str(),
            "https://www.modhub.us/search/bmw/beamng-drive-mods?page=2"
        );
        assert!(ModHubSource::search_url(" . ", 1).is_none());
        // Слэш в запросе не уводит в другой раздел сайта.
        assert!(
            ModHubSource::search_url("a/b", 1)
                .unwrap()
                .as_str()
                .contains("/search/a%2Fb/beamng-drive-mods")
        );
    }

    #[test]
    fn sections_and_feed_have_their_addresses() {
        assert_eq!(
            ModHubSource::feed_url(1).as_str(),
            "https://www.modhub.us/category/beamng-drive-mods"
        );
        assert_eq!(
            ModHubSource::feed_url(3).as_str(),
            "https://www.modhub.us/category/beamng-drive-mods/page/3"
        );
        assert_eq!(
            ModHubSource::browse_url(ModKind::Map, 2).as_str(),
            "https://www.modhub.us/category/beamng-drive-mods/maps/page/2"
        );
        assert_eq!(
            ModHubSource::browse_url(ModKind::Addon, 1).as_str(),
            "https://www.modhub.us/category/beamng-drive-mods/other"
        );
    }
}

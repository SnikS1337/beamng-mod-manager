//! Адаптер Telegram-канала с модами (`t.me/s/m0dsbeamng`, открытая веб-версия).
//!
//! Скачивать отсюда нельзя — архивы отдаёт только приложение Telegram. Канал даёт
//! названия, версии, имена и размеры архивов: этого хватает, чтобы найти мод и
//! заметить его новую версию.
//!
//! Каждая версия мода — новый пост, поэтому карточку мода читаем не по её адресу,
//! а ищем в канале самый новый пост того же мода ([`parser::identity`]).

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};

use url::Url;

use bmm_domain::library::FileSnapshot;
use bmm_domain::models::Listing;
use bmm_domain::text::TitleParser;

use crate::files::FileProbe;
use crate::net::PoliteFetcher;
use crate::parsers::telegram as parser;

use super::common;
use super::{ModSource, SourceError};

static HOME: LazyLock<Url> = LazyLock::new(|| {
    Url::parse(&format!("https://{}/s/{}", parser::HOST, parser::CHANNEL)).unwrap()
});
/// Насколько похожи названия, чтобы считать пост новой версией того же мода, если
/// название в канале чуть поменяли («Audi A6 (C6)» → «Audi A6/RS6 (C6)»). «A4 B8»
/// и «A4 B6» так не сходятся.
const SAME_MOD: f64 = 0.8;

pub struct TelegramSource {
    fetcher: Arc<PoliteFetcher>,
    /// `before` для страниц ленты начиная со второй: веб-версия листается не номерами
    /// страниц, а номером самого старого поста. `None` — дальше постов нет.
    cursors: Mutex<HashMap<u32, Option<i64>>>,
}

impl TelegramSource {
    pub fn new(fetcher: Arc<PoliteFetcher>) -> Self {
        Self {
            fetcher,
            cursors: Mutex::new(HashMap::new()),
        }
    }

    pub fn search_url(query: &str) -> Option<Url> {
        let query = query.trim();
        if query.trim_matches(['.', ' ', '#']).is_empty() {
            return None;
        }
        let mut url = HOME.clone();
        url.query_pairs_mut().append_pair("q", query);
        Some(url)
    }

    fn feed_url(before: Option<i64>) -> Url {
        let mut url = HOME.clone();
        if let Some(before) = before {
            url.query_pairs_mut()
                .append_pair("before", &before.to_string());
        }
        url
    }

    /// Страница, на которой виден пост `post` и архивы сразу за ним.
    fn around_url(post: i64) -> Url {
        Self::feed_url(Some(post + 13))
    }

    fn load_page(&self, url: &Url, cancel: &AtomicBool) -> Result<parser::Page, SourceError> {
        let Some(html) = common::load(&self.fetcher, self.id(), url, None, cancel)? else {
            return Err(SourceError::unavailable("канал не найден"));
        };
        let page = parser::parse_page(&html);
        if !page.channel {
            return Err(SourceError::unavailable(
                "страница канала не читается: возможно, изменилась вёрстка Telegram",
            ));
        }
        Ok(page)
    }

    /// `None` — курсор этой страницы ещё не знаем; `Some(None)` — её нет, канал кончился.
    fn cursor(&self, page: u32) -> Option<Option<i64>> {
        self.cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .get(&page)
            .copied()
    }

    fn remember_cursor(&self, page: u32, before: Option<i64>) {
        self.cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(page, before);
    }

    /// Новейший выпуск того же мода в канале, если он новее `base`.
    fn newest_like(
        &self,
        base: &Listing,
        cancel: &AtomicBool,
    ) -> Result<Option<Listing>, SourceError> {
        let name = TitleParser::parse(&base.title).name;
        let query = if name.trim().is_empty() {
            base.title.clone()
        } else {
            name
        };
        let Some(url) = Self::search_url(&query) else {
            return Ok(None);
        };
        let page = self.load_page(&url, cancel)?;
        let found = parser::search_releases(&page.messages);
        Ok(found
            .iter()
            .map(parser::to_listing)
            .filter(|listing| same_mod(&listing.source_id, &base.source_id))
            .filter(|listing| post_of(listing) > post_of(base))
            .max_by_key(post_of))
    }
}

fn post_of(listing: &Listing) -> i64 {
    listing
        .external_id
        .as_deref()
        .and_then(|id| id.parse().ok())
        .or_else(|| parser::post_id(&listing.url))
        .unwrap_or(0)
}

/// Один мод: то же название без версии или почти то же (поменяли пару слов).
fn same_mod(a: &str, b: &str) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let left: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let right: std::collections::HashSet<&str> = b.split_whitespace().collect();
    let common = left.intersection(&right).count();
    let union = left.len() + right.len() - common;
    union > 0 && common as f64 / union as f64 >= SAME_MOD
}

impl ModSource for TelegramSource {
    fn id(&self) -> &'static str {
        parser::SOURCE_ID
    }

    fn display_name(&self) -> &'static str {
        "Telegram"
    }

    fn supports_search(&self) -> bool {
        true
    }

    fn home(&self) -> Url {
        HOME.clone()
    }

    fn owns(&self, url: &Url) -> bool {
        parser::is_channel_url(url)
    }

    fn is_mod_page(&self, url: &Url) -> bool {
        parser::post_id(url).is_some()
    }

    fn feed_covers(&self, url: &Url) -> bool {
        self.is_mod_page(url)
    }

    fn search(&self, query: &str, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
        let Some(url) = Self::search_url(query) else {
            return Ok(Vec::new());
        };
        let page = self.load_page(&url, cancel)?;
        let mut listings: Vec<Listing> = parser::search_releases(&page.messages)
            .iter()
            .rev()
            .map(parser::to_listing)
            .collect();
        let mut seen = std::collections::HashSet::new();
        listings.retain(|listing| {
            !listing.source_id.is_empty() && seen.insert(listing.source_id.clone())
        });
        Ok(listings)
    }

    fn feed_page(&self, page: u32, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
        let page = page.max(1);
        let before = if page == 1 {
            None
        } else {
            if self.cursor(page).is_none() {
                // Курсора нет (страницы просят не по порядку): идём от первой.
                for earlier in 1..page {
                    match self.cursor(earlier + 1) {
                        Some(None) => return Ok(Vec::new()),
                        Some(Some(_)) => {}
                        None => {
                            self.feed_page(earlier, cancel)?;
                        }
                    }
                }
            }
            match self.cursor(page).flatten() {
                Some(before) => Some(before),
                None => return Ok(Vec::new()), // дальше постов нет
            }
        };
        let loaded = self.load_page(&Self::feed_url(before), cancel)?;
        let (found, orphan) = parser::releases(&loaded.messages);
        self.remember_cursor(page + 1, parser::next_before(&loaded, orphan, before));
        Ok(found.iter().rev().map(parser::to_listing).collect())
    }

    fn listing(&self, url: &Url, cancel: &AtomicBool) -> Result<Option<Listing>, SourceError> {
        let Some(post) = parser::post_id(url) else {
            return Ok(None);
        };
        let page = self.load_page(&Self::around_url(post), cancel)?;
        // Здесь видны и посты с архивами: пост без архива — не выпуск мода.
        let (strict, _) = parser::releases(&page.messages);
        let Some(release) = strict.iter().find(|release| {
            release.post == post || release.files.iter().any(|file| file.post == post)
        }) else {
            // Пост удалили или это не выпуск мода.
            return Ok(None);
        };
        let base = parser::to_listing(release);
        Ok(Some(self.newest_like(&base, cancel)?.unwrap_or(base)))
    }

    fn probes_files(&self) -> bool {
        true
    }

    /// Имя и размер архива канал пишет прямо в посте — ходить никуда не нужно.
    fn probe_file(
        &self,
        listing: &Listing,
        _previous: Option<&FileSnapshot>,
        _open_page: bool,
        _cancel: &AtomicBool,
    ) -> FileProbe {
        let file = match listing.offers.as_slice() {
            [offer] => offer.host_file_id.as_ref().map(|name| FileSnapshot {
                url: offer.url.to_string(),
                name: Some(name.clone()),
                size: offer.size_bytes,
                exact: false,
                page_key: None,
                offer: None,
                seen_at: None,
                gone: false,
            }),
            _ => None,
        };
        FileProbe {
            file,
            opened_page: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::net::{FetchError, PageFetcher, PoliteOptions, RawResponse};

    /// Подставной Telegram: отдаёт страницы из `fixtures/telegram` по адресу.
    struct Channel {
        pages: Mutex<Vec<(String, String)>>,
        calls: Mutex<Vec<String>>,
        status: Mutex<u16>,
    }

    impl Channel {
        fn new(pages: &[(&str, &str)]) -> Arc<Self> {
            Arc::new(Self {
                pages: Mutex::new(
                    pages
                        .iter()
                        .map(|(query, file)| (query.to_string(), fixture(file)))
                        .collect(),
                ),
                calls: Mutex::new(Vec::new()),
                status: Mutex::new(200),
            })
        }

        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
    }

    fn fixture(name: &str) -> String {
        if name.starts_with('<') {
            return name.to_string();
        }
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/telegram")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    impl PageFetcher for Channel {
        fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
            let query = url.query().unwrap_or("").to_string();
            self.calls.lock().unwrap().push(query.clone());
            let text = self
                .pages
                .lock()
                .unwrap()
                .iter()
                .find(|(known, _)| *known == query || (known == "*"))
                .map(|(_, text)| text.clone())
                .unwrap_or_else(|| fixture("search-none-real.html"));
            Ok(RawResponse {
                status: *self.status.lock().unwrap(),
                text,
                final_url: url.clone(),
                retry_after: None,
                etag: None,
                last_modified: None,
            })
        }

        fn post_form(&self, _url: &Url, _form: &[(&str, &str)]) -> Result<RawResponse, FetchError> {
            unreachable!()
        }
    }

    fn source(channel: &Arc<Channel>) -> TelegramSource {
        let options = PoliteOptions {
            pause: std::time::Duration::ZERO,
            jitter: std::time::Duration::ZERO,
            cache_ttl: std::time::Duration::ZERO,
            retries: 0,
            ..PoliteOptions::default()
        };
        let fetcher: Arc<dyn PageFetcher> = channel.clone();
        TelegramSource::new(Arc::new(PoliteFetcher::new(fetcher, options)))
    }

    fn url(raw: &str) -> Url {
        Url::parse(raw).unwrap()
    }

    #[test]
    fn addresses() {
        assert_eq!(
            TelegramSource::search_url("Audi A4").unwrap().as_str(),
            "https://t.me/s/m0dsbeamng?q=Audi+A4"
        );
        assert!(TelegramSource::search_url(" # ").is_none());
        assert_eq!(
            TelegramSource::feed_url(None).as_str(),
            "https://t.me/s/m0dsbeamng"
        );
        assert_eq!(
            TelegramSource::feed_url(Some(18345)).as_str(),
            "https://t.me/s/m0dsbeamng?before=18345"
        );
        let channel = Channel::new(&[]);
        let telegram = source(&channel);
        assert!(telegram.owns(&url("https://t.me/m0dsbeamng/18360")));
        assert!(telegram.is_mod_page(&url("https://t.me/m0dsbeamng/18360")));
        assert!(!telegram.is_mod_page(&url("https://t.me/s/m0dsbeamng?q=audi")));
        assert!(!telegram.owns(&url("https://t.me/other/1")));
        assert!(!telegram.owns(&url("https://www.modland.net/m0dsbeamng/1")));
    }

    #[test]
    fn feed_pages_follow_the_channel_back_in_time() {
        let channel = Channel::new(&[
            ("", "feed-real.html"),
            ("before=18345", "feed-before-real.html"),
        ]);
        let telegram = source(&channel);
        let cancel = AtomicBool::new(false);
        let first = telegram.feed_page(1, &cancel).unwrap();
        assert_eq!(first[0].title, "Audi A4/S4 B8 v1.9", "новые первыми");
        assert_eq!(first.len(), 3);
        let second = telegram.feed_page(2, &cancel).unwrap();
        assert!(
            second
                .iter()
                .any(|listing| listing.title == "Audi RS6 C8 v1.4")
        );
        // Архив RLS 2.4 был первым на странице без своего описания: следующая
        // страница начнётся с него.
        assert_eq!(telegram.cursor(3), Some(Some(18316)));
        assert_eq!(channel.calls(), ["", "before=18345"]);
    }

    #[test]
    fn feed_page_out_of_order_walks_from_the_first() {
        let channel = Channel::new(&[
            ("", "feed-real.html"),
            ("before=18345", "feed-before-real.html"),
        ]);
        let telegram = source(&channel);
        let second = telegram.feed_page(2, &AtomicBool::new(false)).unwrap();
        assert!(!second.is_empty());
        assert_eq!(channel.calls(), ["", "before=18345"]);
    }

    #[test]
    fn end_of_the_channel_is_an_empty_page() {
        let lone = r#"<section class="tgme_channel_history"><div class="tgme_widget_message_wrap"><div class="tgme_widget_message" data-post="m0dsbeamng/1"><div class="tgme_widget_message_text">Привет</div></div></div></section>"#;
        let channel = Channel::new(&[("", lone)]);
        let telegram = source(&channel);
        let cancel = AtomicBool::new(false);
        assert!(telegram.feed_page(1, &cancel).unwrap().is_empty());
        assert!(telegram.feed_page(2, &cancel).unwrap().is_empty());
        assert_eq!(channel.calls().len(), 1, "старше первого поста не ходим");
    }

    #[test]
    fn not_a_channel_page_is_an_error_not_an_empty_feed() {
        let channel = Channel::new(&[(
            "*",
            "<html><body><div class=\"tgme_page\">Telegram</div></body></html>",
        )]);
        let telegram = source(&channel);
        let error = telegram.feed_page(1, &AtomicBool::new(false)).unwrap_err();
        assert!(error.message.contains("вёрстка"));
        assert!(telegram.search("audi", &AtomicBool::new(false)).is_err());
    }

    #[test]
    fn search_gives_one_listing_per_mod_newest_first() {
        let channel = Channel::new(&[("q=Audi+A4", "search-a4-real.html")]);
        let telegram = source(&channel);
        let found = telegram.search("Audi A4", &AtomicBool::new(false)).unwrap();
        assert_eq!(found[0].title, "Audi A4/S4 B8 v1.9");
        let ids: std::collections::HashSet<&str> = found
            .iter()
            .map(|listing| listing.source_id.as_str())
            .collect();
        assert_eq!(ids.len(), found.len());
        assert!(
            telegram
                .search(" ", &AtomicBool::new(false))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn old_post_of_a_mod_leads_to_its_newest_version() {
        // Привязка к посту v1.7 (17383): проверка находит v1.9.
        let channel = Channel::new(&[
            ("before=17396", "search-a4-real.html"),
            ("q=Audi+A4%2FS4+B8", "search-a4-real.html"),
        ]);
        let telegram = source(&channel);
        let found = telegram
            .listing(
                &url("https://t.me/m0dsbeamng/17383"),
                &AtomicBool::new(false),
            )
            .unwrap()
            .unwrap();
        assert_eq!(found.mod_version.as_deref(), Some("1.9"));
        assert_eq!(found.url.as_str(), "https://t.me/m0dsbeamng/18360");
        assert_eq!(found.size_bytes, crate::parsers::parse_size("230.7 MB"));
    }

    #[test]
    fn post_by_its_archive_address_works_too() {
        let channel = Channel::new(&[("before=18378", "feed-real.html")]);
        let telegram = source(&channel);
        let found = telegram
            .listing(
                &url("https://t.me/m0dsbeamng/18365"),
                &AtomicBool::new(false),
            )
            .unwrap()
            .unwrap();
        assert_eq!(found.title, "Audi A4/S4 B8 v1.9");
        assert_eq!(found.url.as_str(), "https://t.me/m0dsbeamng/18360");
    }

    #[test]
    fn newest_post_is_not_replaced_by_an_older_one() {
        let channel = Channel::new(&[
            ("before=18373", "feed-real.html"),
            ("*", "search-a4-real.html"),
        ]);
        let telegram = source(&channel);
        let found = telegram
            .listing(
                &url("https://t.me/m0dsbeamng/18360"),
                &AtomicBool::new(false),
            )
            .unwrap()
            .unwrap();
        assert_eq!(found.url.as_str(), "https://t.me/m0dsbeamng/18360");
    }

    #[test]
    fn deleted_or_non_mod_post_is_missing() {
        let channel = Channel::new(&[("before=18364", "feed-real.html")]);
        let telegram = source(&channel);
        let cancel = AtomicBool::new(false);
        // 18351 — скриншот без архива.
        assert_eq!(
            telegram
                .listing(&url("https://t.me/m0dsbeamng/18351"), &cancel)
                .unwrap(),
            None
        );
        assert_eq!(
            telegram
                .listing(&url("https://t.me/m0dsbeamng"), &cancel)
                .unwrap(),
            None
        );
        assert_eq!(
            telegram
                .listing(&url("https://t.me/other/18360"), &cancel)
                .unwrap(),
            None
        );
    }

    #[test]
    fn similar_titles_count_as_one_mod_but_different_bodies_do_not() {
        use crate::parsers::telegram::identity;
        assert!(same_mod(
            &identity("2004-2011 Audi A6 (C6) V0.3.1, by fastlane"),
            &identity("2004-2011 Audi A6/RS6 (C6) V0.5.4, by fastlane")
        ));
        assert!(!same_mod(
            &identity("Audi A4/S4 B8 v1.9"),
            &identity("Audi A4/S4 B6 (V1.0)")
        ));
        assert!(!same_mod("", ""));
    }

    #[test]
    fn archive_name_and_size_come_from_the_post() {
        let channel = Channel::new(&[]);
        let telegram = source(&channel);
        let page = parser::parse_page(&fixture("feed-real.html"));
        let (found, _) = parser::releases(&page.messages);
        let audi = parser::to_listing(&found[2]);
        let probe = telegram.probe_file(&audi, None, true, &AtomicBool::new(false));
        let file = probe.file.unwrap();
        assert_eq!(
            file.name.as_deref(),
            Some("Audi_A4_S8_B8_tg_m0dsbeamng.zip")
        );
        assert_eq!(file.url, "https://t.me/m0dsbeamng/18365");
        assert!(!file.exact);
        assert!(!probe.opened_page);
        assert!(channel.calls().is_empty(), "никуда не ходили");
    }
}

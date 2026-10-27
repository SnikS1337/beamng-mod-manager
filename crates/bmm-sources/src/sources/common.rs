//! Общие шаги адаптеров: загрузка, поиск по страницам, заголовок карточки.

use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

use regex::Regex;
use url::Url;

use bmm_domain::models::Listing;
use bmm_domain::text::TitleParser;

use crate::net::PoliteFetcher;
use crate::parsers::clean;

use super::SourceError;

static H1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)<h1[^>]*>(?<t>.*?)</h1>").unwrap());
static TITLE_TAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<title[^>]*>(?<t>.*?)</title>").unwrap());
static LEADING_MOD: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(?:Скачать\s+)?мод\s+").unwrap());
static HTML_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
/// Хвост `<title>` с адресом сайта: « - MODSGAMING.RU - Моды для …».
static SITE_TAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\s+-\s+[\p{L}\p{Nd}-]+\.(?:ru|net|org|com|io|de)\b.*$").unwrap()
});
/// Служебные части перед адресом сайта: раздел («BeamNG Машины»), «Моды».
static SECTION_TAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+-\s+(?:моды|mods|beamng(?:\s+[\p{L}]+)?)\s*$").unwrap());
static BEAMNG_SUFFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\s+-\s+BeamNG\.drive$").unwrap());

pub fn load(
    fetcher: &PoliteFetcher,
    source: &str,
    url: &Url,
    form: Option<&[(&str, &str)]>,
    cancel: &AtomicBool,
) -> Result<Option<String>, SourceError> {
    if cancel.load(Ordering::Relaxed) {
        return Err(SourceError::cancelled());
    }
    let result = match form {
        Some(form) => fetcher.post_form_cancellable(source, url, form, cancel),
        None => fetcher.get_cancellable(source, url, cancel),
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(SourceError::cancelled());
    }
    if result.status == crate::net::SourceStatus::Ok
        && let Some(response) = result.response
    {
        if matches!(response.status, 404 | 410) {
            return Ok(None);
        }
        if (200..300).contains(&response.status) {
            return Ok(Some(response.text));
        }
    }
    Err(SourceError {
        status: if result.status == crate::net::SourceStatus::Ok {
            crate::net::SourceStatus::Unavailable
        } else {
            result.status
        },
        message: result
            .message
            .unwrap_or_else(|| "сайт не ответил".to_string()),
    })
}

#[allow(clippy::too_many_arguments)]
pub fn read_search<F, P>(
    fetcher: &PoliteFetcher,
    source: &str,
    first: &Url,
    page_url: F,
    pages: u32,
    form: Option<&[(&str, &str)]>,
    parse: P,
    is_mod_page: &dyn Fn(&Url) -> bool,
    cancel: &AtomicBool,
) -> Result<Vec<Listing>, SourceError>
where
    F: Fn(u32) -> Option<Url>,
    P: Fn(&str) -> Vec<Listing>,
{
    let Some(html) = load(fetcher, source, first, form, cancel)? else {
        return Ok(Vec::new());
    };
    let mut found = parse(&html);
    for page in 2..=pages {
        if found.is_empty() {
            break;
        }
        let Some(next) = page_url(page) else {
            break;
        };
        let more = match load(fetcher, source, &next, None, cancel) {
            Ok(Some(html)) => html,
            Ok(None) => break,
            Err(_) => break,
        };
        let parsed = parse(&more);
        let added: Vec<Listing> = parsed
            .into_iter()
            .filter(|item| {
                !found
                    .iter()
                    .any(|existing| existing.source_id == item.source_id)
            })
            .collect();
        if added.is_empty() {
            break;
        }
        found.extend(added);
    }
    Ok(found
        .into_iter()
        .filter(|item| is_mod_page(&item.url))
        .collect())
}

pub fn page_title(html: &str) -> String {
    let raw = H1
        .captures(html)
        .or_else(|| TITLE_TAG.captures(html))
        .and_then(|captures| captures.name("t"))
        .map(|value| value.as_str())
        .unwrap_or("");
    let text = clean(raw);
    let text = HTML_COMMENT.replace_all(&text, " ");
    let mut text = SITE_TAIL.replace(&text, "").into_owned();
    loop {
        let shorter = SECTION_TAIL.replace(&text, "").into_owned();
        if shorter == text {
            break;
        }
        text = shorter;
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text = LEADING_MOD.replace(&text, "");
    BEAMNG_SUFFIX.replace(&text, "").trim().to_string()
}

/// На странице есть хоть что-то, по чему видно обновление: версия, размер или файл.
/// Ничего нет — это лента/заглушка, а не карточка мода (как `SiteAdapter.Readable` в legacy).
pub fn readable(listing: &Listing) -> bool {
    listing.mod_version.is_some() || listing.size_bytes.is_some() || !listing.offers.is_empty()
}

#[allow(clippy::too_many_arguments)]
pub fn read_listing(
    fetcher: &PoliteFetcher,
    source: &str,
    url: &Url,
    owns: &dyn Fn(&Url) -> bool,
    is_mod_page: &dyn Fn(&Url) -> bool,
    slug: &dyn Fn(&Url) -> String,
    parse_detail: &dyn Fn(&str, &Listing) -> Listing,
    cancel: &AtomicBool,
) -> Result<Option<Listing>, SourceError> {
    // Раздел, лента, поиск или чужой сайт — не карточка; по таким адресам даже не ходим.
    if !owns(url) || !is_mod_page(url) {
        return Ok(None);
    }
    let Some(html) = load(fetcher, source, url, None, cancel)? else {
        return Ok(None);
    };
    let title = page_title(&html);
    let parsed = TitleParser::parse(&title);
    let mut basis = Listing::new(source, url.clone(), slug(url), title);
    basis.mod_version = parsed.mod_version;
    basis.game_version = parsed.game_version;
    let listing = parse_detail(&html, &basis);
    // Лента или страница «сайт на обслуживании», открытая как карточка, не даёт
    // ни версии, ни размера, ни файла: это «не прочитали», а не мод без обновлений.
    if !readable(&listing) {
        return Ok(None);
    }
    Ok(Some(listing))
}

pub fn belongs_to(root: &Url, url: &Url) -> bool {
    // Старые http-ссылки Modland ещё встречаются в базе; не ломаем их.
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let site = root
        .host_str()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_lowercase();
    let host = url
        .host_str()
        .unwrap_or("")
        .trim_start_matches("www.")
        .to_lowercase();
    host == site || host.ends_with(&format!(".{site}"))
}

pub fn segments(url: &Url) -> Vec<&str> {
    url.path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::net::{FetchError, PageFetcher, PoliteOptions, RawResponse, SourceStatus};

    struct ChallengePage;

    impl PageFetcher for ChallengePage {
        fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
            Ok(RawResponse {
                status: 200,
                text: "<html><head><title>Один момент…</title></head><body></body></html>"
                    .to_string(),
                final_url: url.clone(),
                retry_after: None,
                etag: None,
                last_modified: None,
            })
        }
    }

    #[test]
    fn challenge_page_is_not_an_empty_success() {
        let fetcher = PoliteFetcher::new(Arc::new(ChallengePage), PoliteOptions::default());
        let url = Url::parse("https://www.modland.net/search/sienna").unwrap();
        let result = load(&fetcher, "modland", &url, None, &AtomicBool::new(false));
        let error = result.expect_err("страница проверки — не результат поиска");
        assert_eq!(error.status, SourceStatus::NeedsHumanCheck);
    }

    #[test]
    fn page_title_drops_site_tail_and_template_comments() {
        // На ModsGaming у страницы мода нет <h1>, а <title> тащит за собой раздел,
        // шаблонный комментарий uCoz и рекламу сайта — всё это попадало в название мода.
        let html = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/modsgaming/mod-real.html"),
        )
        .unwrap();
        let title = page_title(&html);
        assert!(!title.contains("<!--"), "{title}");
        assert!(!title.to_lowercase().contains("modsgaming"), "{title}");
        assert!(!title.contains("Моды"), "{title}");
        assert!(title.starts_with("BMW M2 G87"), "{title}");
    }

    #[test]
    fn page_title_keeps_dashes_inside_mod_name() {
        let html = "<title>Rolls - Royce Cullinan v1.3 - BeamNG Машины - Моды - MODSGAMING.RU - Моды для FS 25</title>";
        assert_eq!(page_title(html), "Rolls - Royce Cullinan v1.3");
        let plain = "<h1>Мод Rolls - Royce Phantom</h1>";
        assert_eq!(page_title(plain), "Rolls - Royce Phantom");
    }
}

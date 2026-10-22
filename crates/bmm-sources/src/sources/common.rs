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
    LazyLock::new(|| Regex::new(r"^(?:Скачать\s+)?[Мм]од\s+").unwrap());
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
        Some(form) => fetcher.post_form(source, url, form),
        None => fetcher.get(source, url),
    };
    if cancel.load(Ordering::Relaxed) {
        return Err(SourceError::cancelled());
    }
    if let Some(response) = result.response {
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
    let text = LEADING_MOD.replace(&text, "");
    BEAMNG_SUFFIX.replace(&text, "").trim().to_string()
}

pub fn read_listing(
    fetcher: &PoliteFetcher,
    source: &str,
    url: &Url,
    owns: &dyn Fn(&Url) -> bool,
    slug: &dyn Fn(&Url) -> String,
    parse_detail: &dyn Fn(&str, &Listing) -> Listing,
    cancel: &AtomicBool,
) -> Result<Option<Listing>, SourceError> {
    if !owns(url) {
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
    Ok(Some(parse_detail(&html, &basis)))
}

pub fn belongs_to(root: &Url, url: &Url) -> bool {
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

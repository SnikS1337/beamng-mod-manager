//! Разбор списков и страниц Modland.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use bmm_domain::hosts::{self, PAID_HOSTS};
use bmm_domain::models::{Listing, Offer, OfferKind, PriceStatus};
use bmm_domain::pricing::PaidDetector;
use bmm_domain::text::TitleParser;

use super::{clean, decode, join_url, parse_size, strip_tags};

pub const SOURCE_ID: &str = "modland";
pub const BASE: &str = "https://www.modland.net/";

static POST_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<div[^>]*class="[^"]*\bpost-medium\b[^"]*"[^>]*>"#).unwrap());
static TITLE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)post-title[^>]*>\s*<a[^>]*href="(?<h>[^"]+)"[^>]*>(?<t>.*?)</a>"#).unwrap()
});
static ANCHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<a[^>]*href="(?<h>[^"]+)"[^>]*>(?<t>.*?)</a>"#).unwrap());
static POST_IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<img[^>]*class="[^"]*\bpost-img\b[^"]*"[^>]*\bsrc="(?<s>[^"]+)""#).unwrap()
});
static H1_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<h1[^>]*class="[^"]*post-title[^"]*"[^>]*>(?<b>.*?)</h1>"#).unwrap()
});
static EXTERNAL_TITLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"title="External Download - (?<u>https?://[^"\s]+)""#).unwrap());
static LATEST_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Latest Version\s+(?<v>\d+(?:\.\d+)*)").unwrap());
static GAME_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Game Version\s*:?\s*(?<v>\d+(?:\.\d+)*(?:\.x)?)").unwrap());

pub fn parse_list(html: &str) -> Vec<Listing> {
    let starts: Vec<usize> = POST_START
        .find_iter(html)
        .map(|found| found.start())
        .collect();
    let mut result = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(html.len());
        let body = &html[*start..end];
        let Some(link) = TITLE_LINK.captures(body) else {
            continue;
        };
        let (Some(href), Some(title_html)) = (link.name("h"), link.name("t")) else {
            continue;
        };
        let title = clean(title_html.as_str());
        if title.is_empty() {
            continue;
        }
        let Some(url) = join_url(BASE, href.as_str()) else {
            continue;
        };

        let mut author = None;
        let mut category = None;
        for anchor in ANCHOR.captures_iter(body) {
            let Some(anchor_href) = anchor.name("h") else {
                continue;
            };
            let href = decode(anchor_href.as_str());
            if author.is_none() && href.contains("/user/") {
                author = anchor
                    .name("t")
                    .map(|value| clean(value.as_str()))
                    .filter(|value| !value.is_empty());
                continue;
            }
            if category.is_none()
                && let Some(anchor_url) = join_url(BASE, &href)
                && anchor_url
                    .host_str()
                    .is_some_and(|host| host.ends_with("modland.net"))
            {
                let segments: Vec<&str> = anchor_url
                    .path_segments()
                    .map(|segments| segments.filter(|part| !part.is_empty()).collect())
                    .unwrap_or_default();
                if segments.len() == 2
                    && segments[0] == "beamng.drive-mods"
                    && !segments[1].ends_with(".html")
                    && segments[1] != "page"
                {
                    category = anchor
                        .name("t")
                        .map(|value| clean(value.as_str()))
                        .filter(|value| !value.is_empty());
                }
            }
        }

        let parsed = TitleParser::parse(&title);
        let mut listing = Listing::new(SOURCE_ID, url.clone(), slug_of(&url), title.clone());
        listing.preview_url = preview_of(body);
        listing.author = author;
        listing.category = category;
        listing.size_bytes = parse_size(&strip_tags(body));
        listing.mod_version = parsed.mod_version;
        listing.game_version = parsed.game_version;
        listing.price = if has_paid_badge(title_html.as_str()) {
            PriceStatus::Paid
        } else {
            PaidDetector::detect(&title, None, &[])
        };
        result.push(listing);
    }
    result
}

pub fn parse_detail(html: &str, basis: &Listing) -> Listing {
    let mut listing = basis.clone();
    let mut offers = Vec::new();
    let mut seen = HashSet::new();
    let mut download_urls = Vec::new();
    let saw_external = html.to_lowercase().contains("external download links");

    for anchor in ANCHOR.captures_iter(html) {
        let Some(href) = anchor.name("h") else {
            continue;
        };
        let href = decode(href.as_str());
        let Ok(url) = url::Url::parse(&href) else {
            continue;
        };
        collect_offer(
            &url,
            saw_external,
            &mut offers,
            &mut download_urls,
            &mut seen,
        );
    }
    if saw_external {
        for external in EXTERNAL_TITLE.captures_iter(html) {
            let Some(url) = external.name("u") else {
                continue;
            };
            let Ok(url) = url::Url::parse(&decode(url.as_str())) else {
                continue;
            };
            collect_offer(
                &url,
                saw_external,
                &mut offers,
                &mut download_urls,
                &mut seen,
            );
        }
    }

    let text = clean(html);
    if let Some(version) = LATEST_VERSION
        .captures(&text)
        .and_then(|captures| captures.name("v"))
    {
        listing.mod_version = Some(version.as_str().to_string());
    }
    if let Some(version) = GAME_VERSION
        .captures(&text)
        .and_then(|captures| captures.name("v"))
    {
        listing.game_version = Some(version.as_str().to_string());
    }
    listing.size_bytes = basis.size_bytes.or_else(|| parse_size(&text));
    let price = if H1_TITLE
        .captures(html)
        .map(|captures| {
            captures
                .name("b")
                .map(|value| has_paid_badge(value.as_str()))
                .unwrap_or(false)
        })
        .unwrap_or(false)
    {
        PriceStatus::Paid
    } else {
        PaidDetector::detect(&basis.title, None, &download_urls)
    };
    if price != PriceStatus::Unknown {
        listing.price = price;
    }
    listing.offers = offers;
    listing
}

fn collect_offer(
    url: &url::Url,
    saw_external: bool,
    offers: &mut Vec<Offer>,
    download_urls: &mut Vec<url::Url>,
    seen: &mut HashSet<String>,
) {
    if !seen.insert(url.to_string()) {
        return;
    }
    let host = url.host_str().unwrap_or("").to_lowercase();
    if saw_external && hosts::host_matches(url, PAID_HOSTS) {
        offers.push(Offer {
            kind: OfferKind::PaidLink,
            host: host.trim_start_matches("www.").to_string(),
            url: url.clone(),
            host_file_id: None,
            size_bytes: None,
        });
        download_urls.push(url.clone());
    } else if let Some(link) = hosts::try_parse_url(url) {
        offers.push(Offer {
            kind: OfferKind::ExternalHost,
            host: link.host,
            url: url.clone(),
            host_file_id: Some(link.file_id),
            size_bytes: None,
        });
        download_urls.push(url.clone());
    }
}

pub fn has_paid_badge(title_html: &str) -> bool {
    title_html.to_lowercase().contains("title=\"paid content\"")
}

pub fn slug_of(url: &url::Url) -> String {
    url.path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|name| name.rsplit_once('.').map(|(stem, _)| stem).or(Some(name)))
        .unwrap_or("")
        .to_string()
}

fn preview_of(body: &str) -> Option<url::Url> {
    let captures = POST_IMAGE.captures(body)?;
    let raw = decode(captures.name("s")?.as_str());
    let url = url::Url::parse(&raw).ok()?;
    let host = url.host_str()?.to_lowercase();
    if url.scheme() != "https" {
        return None;
    }
    if host == "modland.net" || host.ends_with(".modland.net") {
        Some(url)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/modland")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn parses_list_with_author_category_and_price() {
        let list = parse_list(&fixture("list-page.html"));
        assert_eq!(list.len(), 2);
        assert!(list[0].title.starts_with("Ibishu Miramar Dirt Race"));
        assert_eq!(list[0].author.as_deref(), Some("John Der33"));
        assert_eq!(list[0].category.as_deref(), Some("Cars"));
        assert_eq!(list[0].source_id, "ibishu-miramar-dirt-race-0.9.0-wV9h");
        assert_eq!(list[0].size_bytes, Some(1_142_947));
        assert_eq!(list[0].price, PriceStatus::Unknown);
        assert_eq!(list[1].price, PriceStatus::Paid);
        assert_eq!(list[1].category.as_deref(), Some("Maps"));
        assert_eq!(list[1].mod_version.as_deref(), Some("0.2.3"));
    }

    #[test]
    fn parses_real_search_page() {
        let list = parse_list(&fixture("search-real.html"));
        assert_eq!(list.len(), 6);
        assert!(list.iter().any(|item| item.title.contains("Sienna")));
    }

    #[test]
    fn detail_reads_game_version_and_direct_host() {
        let list = parse_list(&fixture("list-page.html"));
        let detail = parse_detail(&fixture("mod-free.html"), &list[0]);
        assert_eq!(detail.game_version.as_deref(), Some("0.9.0.5.4267"));
        assert!(detail.price != PriceStatus::Paid);
        assert!(
            detail
                .offers
                .iter()
                .all(|offer| offer.kind != OfferKind::PaidLink)
        );
    }

    #[test]
    fn detail_reads_paid_links() {
        let list = parse_list(&fixture("list-page.html"));
        let detail = parse_detail(&fixture("mod-paid.html"), &list[1]);
        assert_eq!(detail.price, PriceStatus::Paid);
        assert!(
            detail
                .offers
                .iter()
                .any(|offer| offer.kind == OfferKind::PaidLink),
            "{:?}",
            detail.offers
        );
    }

    #[test]
    fn paid_badge_from_title_html() {
        assert!(has_paid_badge(r#"<span title="Paid content">$</span>"#));
        assert!(!has_paid_badge("<span>Sienna</span>"));
    }
}

//! Разбор ModsGaming (uCoz): списки, поиск и карточка мода.

use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use bmm_domain::models::{Listing, Offer, OfferKind, PriceStatus};
use bmm_domain::text::TitleParser;

use super::{clean, decode, join_url, line_text, parse_size, strip_tags};

pub const SOURCE_ID: &str = "modsgaming";
pub const BASE: &str = "https://www.modsgaming.us/";

static ENTRY_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<div[^>]*\bid="entryID(?<id>\d+)"[^>]*>"#).unwrap());
static TITLE_ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a(?<attrs>[^>]*\bclass="[^"]*\bvidmattit\b[^"]*"[^>]*)>(?<t>.*?)</a>"#)
        .unwrap()
});
static HREF_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bhref="(?<h>[^"]+)""#).unwrap());
static SEARCH_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<td[^>]*class="search-td-1"[^>]*>\s*<a[^>]*\bhref="(?<h>[^"]+)"[^>]*>(?<t>.*?)</a>"#,
    )
    .unwrap()
});
static SEARCH_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)class="search-span-2"[^>]*>.*?(?<d>\d{4}-\d{2}-\d{2}(?:\s+\d{2}:\d{2}:\d{2})?)"#,
    )
    .unwrap()
});
static ENTRY_ID: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-(?<id>\d+)$").unwrap());
static AUTHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)АВТОР\s+МОДА\s*:\s*(?<a>[^\r\n]+?)\s*(?:\r|\n|$)").unwrap());
static AUTHOR_EN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bAuthor\s*:\s*(?<a>[^\r\n]+?)\s*(?:\r|\n|$)").unwrap());
static ANCHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<a(?<attrs>[^>]*)>(?<t>.*?)</a>").unwrap());
static DETAIL_IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)class="(?:detimg|screendiv-1)"[^>]*background:\s*url\(\s*['"]?(?<u>/_ld/[^'"\)]+)"#,
    )
    .unwrap()
});

pub fn parse_list(html: &str) -> Vec<Listing> {
    let starts: Vec<(usize, usize, String)> = ENTRY_START
        .captures_iter(html)
        .filter_map(|captures| {
            let whole = captures.get(0)?;
            Some((
                whole.start(),
                whole.end(),
                captures.name("id")?.as_str().to_string(),
            ))
        })
        .collect();
    let mut result = Vec::new();
    for (index, (_, start, id)) in starts.iter().enumerate() {
        let end = starts
            .get(index + 1)
            .map(|(next, _, _)| *next)
            .unwrap_or(html.len());
        let body = &html[*start..end];
        let Some(anchor) = TITLE_ANCHOR.captures(body) else {
            continue;
        };
        let Some(attrs) = anchor.name("attrs") else {
            continue;
        };
        let Some(href) = HREF_ATTR
            .captures(attrs.as_str())
            .and_then(|captures| captures.name("h"))
        else {
            continue;
        };
        let Some(url) = join_url(BASE, href.as_str()) else {
            continue;
        };
        let title = anchor
            .name("t")
            .map(|value| clean(value.as_str()))
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let parsed = TitleParser::parse(&title);
        let mut listing = Listing::new(SOURCE_ID, url.clone(), id.as_str(), title);
        listing.mod_version = parsed.mod_version;
        listing.game_version = parsed.game_version;
        listing.category = category_of(&url);
        listing.price = if parsed.has_paid_marker {
            PriceStatus::Paid
        } else {
            PriceStatus::Unknown
        };
        result.push(listing);
    }
    result
}

pub fn parse_search(html: &str) -> Vec<Listing> {
    let matches: Vec<(usize, regex::Captures<'_>)> = SEARCH_LINK
        .captures_iter(html)
        .filter_map(|captures| {
            let start = captures.get(0)?.start();
            Some((start, captures))
        })
        .collect();
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for (index, (start, captures)) in matches.iter().enumerate() {
        let Some(href) = captures.name("h") else {
            continue;
        };
        let Some(url) = join_url(BASE, href.as_str()) else {
            continue;
        };
        if !url.path().starts_with("/load/beamng_drive/") {
            continue;
        }
        let Some(id) = ENTRY_ID
            .captures(url.path().trim_end_matches('/'))
            .and_then(|found| found.name("id"))
            .map(|id| id.as_str().to_string())
        else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        let title = captures
            .name("t")
            .map(|value| clean(value.as_str()))
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        let end = matches
            .get(index + 1)
            .map(|(next, _)| *next)
            .unwrap_or(html.len());
        let slice = &html[*start..end];
        let published_at = SEARCH_DATE
            .captures(slice)
            .and_then(|found| found.name("d"))
            .and_then(|value| parse_datetime(value.as_str()));
        push_search_listing(&mut result, url, &id, title, published_at);
    }

    // ModsGaming.us: карточки поиска — обычные `a.vidmattit` без `entryID`.
    for anchor in TITLE_ANCHOR.captures_iter(html) {
        let Some(attrs) = anchor.name("attrs") else {
            continue;
        };
        let Some(href) = HREF_ATTR
            .captures(attrs.as_str())
            .and_then(|captures| captures.name("h"))
        else {
            continue;
        };
        let Some(url) = join_url(BASE, href.as_str()) else {
            continue;
        };
        if !is_mod_path(&url) {
            continue;
        }
        let Some(id) = ENTRY_ID
            .captures(url.path().trim_end_matches('/'))
            .and_then(|found| found.name("id"))
            .map(|id| id.as_str().to_string())
        else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        let title = anchor
            .name("t")
            .map(|value| clean(value.as_str()))
            .unwrap_or_default();
        if title.is_empty() {
            continue;
        }
        push_search_listing(&mut result, url, &id, title, None);
    }
    result
}

fn push_search_listing(
    result: &mut Vec<Listing>,
    url: url::Url,
    id: &str,
    title: String,
    published_at: Option<chrono::DateTime<chrono::FixedOffset>>,
) {
    let parsed = TitleParser::parse(&title);
    let mut listing = Listing::new(SOURCE_ID, url.clone(), id, title);
    listing.mod_version = parsed.mod_version;
    listing.game_version = parsed.game_version;
    listing.category = category_of(&url);
    listing.published_at = published_at;
    listing.price = if parsed.has_paid_marker {
        PriceStatus::Paid
    } else {
        PriceStatus::Unknown
    };
    result.push(listing);
}

fn is_mod_path(url: &url::Url) -> bool {
    let parts: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    let page_like = parts.last().is_some_and(|last| {
        let pieces: Vec<&str> = last.split('-').collect();
        pieces.len() <= 2
            && pieces
                .iter()
                .all(|part| !part.is_empty() && part.chars().all(|ch| ch.is_ascii_digit()))
    });
    if !parts
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case("load"))
    {
        return false;
    }
    let min_parts = match parts.get(1).map(|part| part.to_ascii_lowercase()) {
        Some(kind) if kind == "beamng" => 5,
        Some(kind) if kind == "beamng_drive" => 4,
        _ => return false,
    };
    parts.len() >= min_parts && !page_like
}

pub fn parse_detail(html: &str, basis: &Listing) -> Listing {
    let mut listing = basis.clone();
    let without_comments = Regex::new(r"(?s)<!--.*?-->")
        .unwrap()
        .replace_all(html, "")
        .into_owned();
    let text = line_text(&without_comments);
    let author = AUTHOR.captures(&text).or_else(|| AUTHOR_EN.captures(&text));
    if let Some(author) = author.and_then(|captures| captures.name("a")) {
        listing.author = Some(decode(author.as_str()).trim().to_string());
    }
    if let Some(preview) = image_of(&without_comments, basis.url.as_str()) {
        listing.preview_url = Some(preview);
    }

    let mut size = basis.size_bytes;
    let anchors: Vec<_> = ANCHOR
        .captures_iter(&without_comments)
        .filter_map(|captures| {
            let attrs = captures.name("attrs")?.as_str();
            if !attrs.contains("filedowl-a") {
                return None;
            }
            let href = HREF_ATTR
                .captures(attrs)
                .and_then(|found| found.name("h"))?;
            let whole = captures.get(0)?;
            Some((href.as_str().to_string(), whole.end()))
        })
        .collect();
    if let Some((href, end)) = anchors.first()
        && let Some(url) = join_url(basis.url.as_str(), href)
    {
        let after = &without_comments[*end..without_comments.len().min(*end + 300)];
        if let Some(value) = parse_size(&strip_tags(after)) {
            size = Some(value);
        }
        listing.offers = vec![Offer {
            kind: OfferKind::Direct,
            host: "modsgaming".to_string(),
            url,
            host_file_id: None,
            size_bytes: size,
        }];
    }
    listing.size_bytes = size;
    listing
}

pub fn image_of(html: &str, base: &str) -> Option<url::Url> {
    let raw = DETAIL_IMAGE.captures(html)?.name("u")?.as_str();
    let url = join_url(base, raw.trim())?;
    (url.scheme() == "https").then_some(url)
}

pub fn category_of(url: &url::Url) -> Option<String> {
    let parts: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    (parts.len() >= 3 && parts[0] == "load").then(|| parts[2].to_string())
}

fn parse_datetime(value: &str) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
    let cleaned = value.trim();
    if let Ok(date) = NaiveDateTime::parse_from_str(cleaned, "%Y-%m-%d %H:%M:%S") {
        return Some(Utc.from_utc_datetime(&date).fixed_offset());
    }
    NaiveDate::parse_from_str(cleaned, "%Y-%m-%d")
        .ok()
        .and_then(|date| date.and_hms_opt(0, 0, 0))
        .map(|date| Utc.from_utc_datetime(&date).fixed_offset())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/modsgaming")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn parses_list_versions_and_category() {
        let list = parse_list(&fixture("list-page.html"));
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].source_id, "160435");
        assert_eq!(list[0].mod_version.as_deref(), Some("5.1"));
        assert_eq!(list[0].game_version.as_deref(), Some("0.39.x"));
        assert_eq!(list[0].category.as_deref(), Some("mashiny"));
        assert_eq!(list[0].price, PriceStatus::Unknown);
    }

    #[test]
    fn parses_detail_author_size_and_internal_link() {
        let list = parse_list(&fixture("list-page.html"));
        let detail = parse_detail(&fixture("mod-page.html"), &list[0]);
        assert_eq!(detail.author.as_deref(), Some("Infiniti"));
        assert_eq!(detail.size_bytes, Some(331_769_446));
        assert_eq!(detail.offers.len(), 1);
        assert_eq!(detail.offers[0].url.path(), "/load/0-0-1-160435-20");
        assert_eq!(detail.offers[0].kind, OfferKind::Direct);
    }

    #[test]
    fn search_keeps_only_beamng_and_dates() {
        let list = parse_search(&fixture("search-real.html"));
        assert_eq!(list.len(), 9);
        assert!(
            list.iter()
                .all(|item| item.url.path().starts_with("/load/beamng_drive/"))
        );
        assert!(list.iter().any(|item| item.published_at.is_some()));
    }

    #[test]
    fn real_list_has_ten_cards_with_game_version() {
        let list = parse_list(&fixture("list-real.html"));
        assert_eq!(list.len(), 10);
        assert_eq!(list[0].source_id, "160435");
        assert_eq!(list[0].mod_version.as_deref(), Some("5.1"));
        assert!(
            list.iter()
                .all(|item| item.game_version.as_deref() == Some("0.39.x")),
            "{:?}",
            list.iter()
                .map(|item| item.game_version.as_deref())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn real_search_has_ids_versions_and_dates() {
        let list = parse_search(&fixture("search-real.html"));
        assert_eq!(list.len(), 9);
        let ids: HashSet<&str> = list.iter().map(|item| item.source_id.as_str()).collect();
        for id in [
            "133899", "141677", "160200", "142469", "134101", "128066", "148563", "118383",
            "130575",
        ] {
            assert!(ids.contains(id), "{id}");
        }
        assert!(
            list.iter()
                .all(|item| !item.title.contains('<')
                    && item.title.to_lowercase().contains("sienna")),
            "подсветка снята"
        );
        let newest = list.iter().find(|item| item.source_id == "160200").unwrap();
        assert_eq!(newest.mod_version.as_deref(), Some("2.2"));
        assert_eq!(newest.game_version.as_deref(), Some("0.39.x"));
        assert_eq!(
            newest
                .published_at
                .map(|date| date.format("%Y").to_string()),
            Some("2026".to_string())
        );
    }

    #[test]
    fn detail_takes_the_page_image() {
        let html =
            r#"<div class="detimg" style="background: url('/_ld/abc123.jpg') no-repeat"></div>"#;
        let image = image_of(html, BASE).expect("картинка страницы");
        assert_eq!(image.as_str(), "https://www.modsgaming.us/_ld/abc123.jpg");
    }

    #[test]
    fn parses_modsgaming_us_search_and_detail() {
        let html = r#"
            <table class="vidmattable"><tbody><tr><td class="vidmattd">
            <div><a href="/load/beamng/maps/rcd_russian_car_drift_v1_1_0_40_x/75-1-0-123015" class="vidmattit"><b>RCD</b> - Russian Car Drift v1.1 (0.40.x)</a></div>
            </td></tr></tbody></table>
        "#;
        let list = parse_search(html);
        assert_eq!(list.len(), 1);
        assert_eq!(
            list[0].url.as_str(),
            "https://www.modsgaming.us/load/beamng/maps/rcd_russian_car_drift_v1_1_0_40_x/75-1-0-123015"
        );
        assert_eq!(list[0].mod_version.as_deref(), Some("1.1"));
        assert_eq!(list[0].game_version.as_deref(), Some("0.40.x"));
        assert_eq!(list[0].category.as_deref(), Some("maps"));

        let detail = parse_detail(
            r#"<a class="filedowl-a" href="/load/0-0-1-123015-20">Download</a>
               <span class="filesize-span">65.34 MB</span>
               <div class="detimg" style="background:url('/_ld/1230/49313218.png')"></div>
               <p>Author: Carlovers Games</p>"#,
            &list[0],
        );
        assert_eq!(detail.author.as_deref(), Some("Carlovers Games"));
        assert!(detail.size_bytes.unwrap_or_default() > 60_000_000);
        assert_eq!(detail.offers[0].url.path(), "/load/0-0-1-123015-20");
    }

    #[test]
    fn real_detail_reads_size_and_internal_offer() {
        let basis = parse_list(&fixture("list-real.html")).remove(0);
        let detail = parse_detail(&fixture("mod-real.html"), &basis);
        assert_eq!(detail.author.as_deref(), Some("Infiniti"));
        assert_eq!(detail.size_bytes, Some(331_769_446));
        assert_eq!(detail.offers.len(), 1);
        assert_eq!(detail.offers[0].url.path(), "/load/0-0-1-160435-20");
    }
}

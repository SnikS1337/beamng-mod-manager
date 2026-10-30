//! Разбор официального репозитория beamng.com/resources (XenForo Resource Manager).

use std::sync::LazyLock;

use regex::Regex;

use bmm_domain::models::{Listing, Offer, OfferKind, PriceStatus};

use super::{clean, decode, join_url, parse_epoch, parse_listing_date, parse_size};

pub const SOURCE_ID: &str = "beamng";
pub const BASE: &str = "https://beamng.com/";

static ITEM_START: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"<li[^>]*class="[^"]*\bresourceListItem\b[^"]*"[^>]*\bid="resource-(?<id>\d+)"[^>]*>"#,
    )
    .unwrap()
});
static TITLE_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<h3 class="title">(?<t>.*?)</h3>"#).unwrap());
static TITLE_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a\s+href="(?<h>resources/[^"?]+/)"[^>]*>(?<n>[^<]+)</a>"#).unwrap()
});
static VERSION_SPAN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<span class="(?:version|muted)">(?<v>[^<]*)</span>"#).unwrap());
static AUTHOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<a\s+href="resources/authors/[^"]*">(?<a>[^<]+)</a>"#).unwrap());
static CATEGORY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"<a\s+href="resources/categories/[^"]*">(?<c>[^<]+)</a>"#).unwrap()
});
static UPDATED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<dl class="resourceUpdated">.*?<(?:abbr|span)(?<attrs>[^>]*)>(?<txt>[^<]*)</(?:abbr|span)>"#,
    )
    .unwrap()
});
static DATA_TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"data-time="(?<t>\d+)""#).unwrap());
static TITLE_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"title="(?<t>[^"]+)""#).unwrap());
static UNIQUE_ID: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)Unique ID:</dt>\s*<dd[^>]*>\s*(?<id>[A-Za-z0-9]+)\s*</dd>").unwrap()
});
static DOWNLOAD_BTN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<a[^>]*href="(?<h>resources/[^"]+/download\?version=\d+)"[^>]*>\s*Download Now\s*(?:<small[^>]*>\s*(?<sz>[^<]*?)\s*</small>)?"#,
    )
    .unwrap()
});
static H1_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<h1>.*?<span class="muted">(?<v>[^<]+)</span>"#).unwrap());

pub fn parse_list(html: &str) -> Vec<Listing> {
    let starts: Vec<(usize, usize, String)> = ITEM_START
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
        let Some(title_block) = TITLE_BLOCK.captures(body) else {
            continue;
        };
        let title_html = title_block.name("t").map(|m| m.as_str()).unwrap_or("");
        let Some(link) = TITLE_LINK.captures(title_html) else {
            continue;
        };
        let (Some(href), Some(name)) = (link.name("h"), link.name("n")) else {
            continue;
        };
        let Some(url) = join_url(BASE, href.as_str()) else {
            continue;
        };
        let name = clean(name.as_str());
        if name.is_empty() {
            continue;
        }

        let mod_version = VERSION_SPAN
            .captures(title_html)
            .and_then(|captures| captures.name("v"))
            .map(|value| clean(value.as_str()))
            .filter(|value| !value.is_empty());
        let author = AUTHOR
            .captures(body)
            .and_then(|captures| captures.name("a"))
            .map(|value| clean(value.as_str()));
        let category = CATEGORY
            .captures(body)
            .and_then(|captures| captures.name("c"))
            .map(|value| clean(value.as_str()));
        let updated_at = UPDATED.captures(body).and_then(|captures| {
            let attrs = captures.name("attrs").map(|m| m.as_str()).unwrap_or("");
            let text = captures.name("txt").map(|m| m.as_str()).unwrap_or("");
            DATA_TIME
                .captures(attrs)
                .and_then(|data| parse_epoch(data.name("t")?.as_str()))
                .or_else(|| {
                    TITLE_ATTR
                        .captures(attrs)
                        .and_then(|data| parse_listing_date(data.name("t")?.as_str()))
                })
                .or_else(|| parse_listing_date(&decode(text)))
        });

        let mut listing = Listing::new(SOURCE_ID, url, id.as_str(), name);
        listing.mod_version = mod_version;
        listing.author = author;
        listing.category = category;
        listing.updated_at = updated_at;
        result.push(listing);
    }
    result
}

pub fn parse_detail(html: &str, basis: &Listing) -> Listing {
    let mut listing = basis.clone();
    if let Some(uid) = UNIQUE_ID
        .captures(html)
        .and_then(|captures| captures.name("id"))
    {
        listing.external_id = Some(uid.as_str().to_string());
    }
    let mut size = basis.size_bytes;
    if let Some(button) = DOWNLOAD_BTN.captures(html)
        && let Some(href) = button.name("h")
        && let Some(url) = join_url(BASE, href.as_str())
    {
        if let Some(value) = button
            .name("sz")
            .and_then(|value| parse_size(value.as_str()))
        {
            size = Some(value);
        }
        listing.offers = vec![Offer {
            kind: OfferKind::Direct,
            host: "beamng.com".to_string(),
            url,
            host_file_id: None,
            size_bytes: size,
        }];
        listing.price = PriceStatus::Free;
    }
    listing.size_bytes = size;
    if listing.author.is_none() {
        listing.author = AUTHOR
            .captures(html)
            .and_then(|captures| captures.name("a"))
            .map(|value| clean(value.as_str()));
    }
    if listing.category.is_none() {
        listing.category = CATEGORY
            .captures(html)
            .and_then(|captures| captures.name("c"))
            .map(|value| clean(value.as_str()));
    }
    if let Some(version) = H1_VERSION
        .captures(html)
        .and_then(|captures| captures.name("v"))
    {
        listing.mod_version = Some(clean(version.as_str()));
    }
    listing
}

pub fn slug_of(url: &url::Url) -> String {
    let last = url
        .path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("");
    let digits = Regex::new(r"\.(\d+)$").unwrap();
    digits
        .captures(last)
        .and_then(|captures| captures.get(1))
        .map(|value| value.as_str().to_string())
        .unwrap_or_else(|| last.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/beamng")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn parses_list_fixture() {
        let list = parse_list(&fixture("list-2026-10-18.html"));
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].source_id, "39785");
        assert!(list[0].title.contains("Autodrom Most"), "{}", list[0].title);
        assert_eq!(list[0].mod_version.as_deref(), Some("0.0.1"));
        assert_eq!(list[0].author.as_deref(), Some("Dogin"));
        assert_eq!(
            list.iter()
                .map(|item| item.source_id.as_str())
                .collect::<Vec<_>>(),
            vec!["39785", "39786", "39806"]
        );
    }

    #[test]
    fn parses_real_list_dates_and_beta_prefix() {
        let list = parse_list(&fixture("list-real.html"));
        assert_eq!(list.len(), 4);
        assert_eq!(list[0].source_id, "39753");
        assert_eq!(list[0].title, "Object Spawner");
        assert_eq!(list[0].mod_version.as_deref(), Some("1.0"));
        assert_eq!(list[0].author.as_deref(), Some("Bikk-"));
        assert_eq!(list[0].updated_at.unwrap().timestamp(), 1_791_897_661);
        assert_eq!(list[1].source_id, "39760");
        assert_eq!(list[1].title, "ACNG - Tire Heat, Pressure & Wear");
        assert_eq!(list[1].mod_version.as_deref(), Some("0.3.1-beta"));
    }

    #[test]
    fn parses_detail_with_direct_file() {
        let list = parse_list(&fixture("list-real.html"));
        let detail = parse_detail(&fixture("mod-real.html"), &list[1]);
        assert_eq!(detail.external_id.as_deref(), Some("MOKU3EJNG"));
        assert_eq!(detail.mod_version.as_deref(), Some("0.3.1-beta"));
        assert_eq!(detail.size_bytes, Some(102_604));
        assert_eq!(detail.price, PriceStatus::Free);
        assert_eq!(detail.offers.len(), 1);
        assert_eq!(detail.offers[0].kind, OfferKind::Direct);
        assert_eq!(detail.offers[0].host, "beamng.com");
    }

    #[test]
    fn slug_from_resource_url() {
        let url = url::Url::parse("https://beamng.com/resources/acng.39760/").unwrap();
        assert_eq!(slug_of(&url), "39760");
    }

    #[test]
    fn real_page_by_short_address_reads_version_and_download() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/beamng/mod-906-real.html");
        let html = std::fs::read_to_string(path).unwrap();
        let basis = Listing::new(
            SOURCE_ID,
            url::Url::parse("https://beamng.com/resources/crash-test-dummy.906/").unwrap(),
            "906",
            "Crash Test Dummy",
        );
        let detail = parse_detail(&html, &basis);
        assert_eq!(detail.mod_version.as_deref(), Some("1.1"));
        assert!(!detail.offers.is_empty(), "кнопка Download Now");
    }
}

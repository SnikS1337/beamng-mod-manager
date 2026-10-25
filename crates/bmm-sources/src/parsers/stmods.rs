//! Разбор STMods: карточки списка, поиск и страница мода.

use std::sync::LazyLock;

use regex::Regex;

use bmm_domain::models::{Listing, Offer, OfferKind, PriceStatus};
use bmm_domain::text::TitleParser;

use super::{clean, decode, join_url, parse_size, strip_tags};

pub const SOURCE_ID: &str = "stmods";
pub const BASE: &str = "https://stmods.org/";

static CARD_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<div[^>]*class="row index-card"[^>]*>"#).unwrap());
static ANCHOR_OPEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)<a(?<attrs>[^>]*)>").unwrap());
static HREF_ATTR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"\bhref="(?<h>[^"]+)""#).unwrap());
static CARD_IMAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)background-image:\s*url\(\s*['"]?(?<u>[^'"\)]+)"#).unwrap());
static CARD_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<h3 class="text-left">\s*<b>(?<n>.*?)</b>\s*(?:<small>(?<s>.*?)</small>)?"#)
        .unwrap()
});
static AUTHOR_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)Автор:\s*<a[^>]*>(?<a>.*?)</a>"#).unwrap());
static CARD_TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div class="col-xs-12 index-card-text">(?<t>.*?)</div>"#).unwrap()
});
/// «Категория: <a href="../../cars/">Легковые автомобили</a>» на странице мода.
static DETAIL_CATEGORY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)Категория:\s*<a[^>]*href="(?<h>[^"]*)"[^>]*>(?<n>.*?)</a>"#).unwrap()
});
static DOWNLOAD_ANCHOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<a[^>]*href="(?<h>/download_file\.php\?[^"]+)"[^>]*>(?<t>.*?)</a>"#).unwrap()
});
static HOST_SMALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?is)<small[^>]*>\s*с\s+(?<host>[^<]+?)\s*</small>"#).unwrap());

pub fn parse_list(html: &str) -> Vec<Listing> {
    let starts: Vec<usize> = CARD_START
        .find_iter(html)
        .map(|found| found.start())
        .collect();
    let mut result = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(html.len());
        let card = &html[*start..end];
        let Some(url) = card_url(card) else {
            continue;
        };
        if !url.path().starts_with("/beamng_drive/") {
            continue;
        }
        let Some(title) = CARD_TITLE.captures(card) else {
            continue;
        };
        let name = title
            .name("n")
            .map(|value| clean(value.as_str()))
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let tail = title
            .name("s")
            .map(|value| clean(value.as_str()))
            .unwrap_or_default();
        let full = if tail.is_empty() {
            name
        } else {
            format!("{name} {tail}")
        };
        let parsed = TitleParser::parse(&full);
        let author = AUTHOR_LINK
            .captures(card)
            .and_then(|captures| captures.name("a"))
            .map(|value| clean(value.as_str()));
        let preview = card_preview(card);
        let mut listing = Listing::new(SOURCE_ID, url.clone(), slug_of(&url), full);
        listing.mod_version = parsed.mod_version;
        listing.game_version = parsed.game_version;
        listing.author = author;
        listing.preview_url = preview;
        listing.category = CARD_TEXT
            .captures(card)
            .and_then(|captures| captures.name("t"))
            .and_then(|text| category_hint(&strip_tags(text.as_str())))
            .map(str::to_string);
        listing.price = if parsed.has_paid_marker {
            PriceStatus::Paid
        } else {
            PriceStatus::Unknown
        };
        result.push(listing);
    }
    result
}

/// Раздела в карточке поиска STMods нет: все моды лежат в `/beamng_drive/mods/`.
/// Подсказка по описанию: «20 конфигураций» бывает только у машин, «карта» — у карт.
/// Не уверены — `None`, тип возьмут из названия или со страницы мода.
pub fn category_hint(text: &str) -> Option<&'static str> {
    let text = text.to_lowercase();
    if text.contains("конфигурац") {
        Some("cars")
    } else if ["карта ", "карту ", "карты ", "новая карта", "map "]
        .iter()
        .any(|word| text.contains(word))
    {
        Some("maps")
    } else {
        None
    }
}

pub fn is_update_marked(card_html: &str) -> bool {
    card_html.contains(">ОБНОВЛЕНИЕ<")
}

pub fn parse_detail(html: &str, basis: &Listing) -> Listing {
    let mut listing = basis.clone();
    let mut size = basis.size_bytes;
    if let Some(download) = DOWNLOAD_ANCHOR.captures(html)
        && let Some(href) = download.name("h")
        && let Some(url) = join_url(BASE, href.as_str())
    {
        let text = download.name("t").map(|value| value.as_str()).unwrap_or("");
        if let Some(value) = parse_size(&strip_tags(text)) {
            size = Some(value);
        }
        let host = HOST_SMALL
            .captures(text)
            .and_then(|captures| captures.name("host"))
            .map(|value| clean(value.as_str()).to_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "stmods".to_string());
        listing.offers = vec![Offer {
            kind: OfferKind::ExternalHost,
            host,
            url,
            host_file_id: None,
            size_bytes: size,
        }];
    }
    if let Some(author) = AUTHOR_LINK
        .captures(html)
        .and_then(|captures| captures.name("a"))
    {
        listing.author = Some(clean(author.as_str()));
    }
    // Раздел со страницы точнее подсказки по описанию: «../../cars/» → «cars».
    if let Some(found) = DETAIL_CATEGORY.captures(html) {
        let slug = found
            .name("h")
            .map(|value| value.as_str().trim_matches(|c| c == '/' || c == '.'))
            .and_then(|value| value.rsplit('/').next())
            .filter(|value| !value.is_empty());
        let name = found.name("n").map(|value| clean(value.as_str()));
        listing.category = slug.map(str::to_string).or(name).or(listing.category);
    }
    listing.size_bytes = size;
    listing
}

pub fn slug_of(url: &url::Url) -> String {
    url.path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

fn card_url(card: &str) -> Option<url::Url> {
    for anchor in ANCHOR_OPEN.captures_iter(card) {
        let attrs = anchor.name("attrs")?.as_str();
        if !attrs.contains("index-card-img") {
            continue;
        }
        let href = HREF_ATTR
            .captures(attrs)
            .and_then(|captures| captures.name("h"))?;
        return join_url(BASE, href.as_str());
    }
    None
}

fn card_preview(card: &str) -> Option<url::Url> {
    let raw = CARD_IMAGE.captures(card)?.name("u")?.as_str();
    let url = join_url(BASE, decode(raw).trim())?;
    (url.scheme() == "https").then_some(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/stmods")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn parses_real_list() {
        let list = parse_list(&fixture("list-real.html"));
        assert_eq!(list.len(), 7);
        assert_eq!(list[1].mod_version.as_deref(), Some("3.3"));
        assert_eq!(list[1].author.as_deref(), Some("matissator"));
        assert!(list[1].title.starts_with("BMW E34"));
        assert!(
            list.iter()
                .all(|item| item.url.path().starts_with("/beamng_drive/"))
        );
        assert!(list[1].preview_url.is_some());
    }

    #[test]
    fn parses_detail_with_size_and_host() {
        let list = parse_list(&fixture("list-real.html"));
        let detail = parse_detail(&fixture("mod-real.html"), &list[1]);
        assert_eq!(detail.mod_version.as_deref(), Some("3.3"));
        assert_eq!(detail.author.as_deref(), Some("matissator"));
        assert_eq!(detail.size_bytes, Some(223_053_086));
        assert_eq!(detail.offers.len(), 1);
        assert_eq!(detail.offers[0].kind, OfferKind::ExternalHost);
        assert_eq!(detail.offers[0].host, "modsfire");
    }

    #[test]
    fn parses_search_pages() {
        let search = parse_list(&fixture("search-real.html"));
        assert_eq!(search.len(), 3);
        let dated = parse_list(&fixture("search-beamng-2026-10-18.html"));
        assert_eq!(dated.len(), 2);
        assert!(dated[0].title.contains("BMW E34"));
        assert_eq!(dated[0].author.as_deref(), Some("matissator"));
        assert!(dated[0].preview_url.is_some());
    }

    #[test]
    fn category_from_detail_page() {
        let list = parse_list(&fixture("list-real.html"));
        let detail = parse_detail(&fixture("mod-real.html"), &list[1]);
        assert_eq!(detail.category.as_deref(), Some("cars"));
        assert_eq!(
            bmm_domain::text::ModKind::from_category("cars"),
            Some(bmm_domain::text::ModKind::Vehicle)
        );
    }

    #[test]
    fn category_hint_from_search_card() {
        // Настоящая выдача: «BMW F30/F80. - 20 конфигураций» — машина.
        let search = parse_list(&fixture("search-beamng-2026-10-18.html"));
        assert!(
            search
                .iter()
                .any(|item| item.category.as_deref() == Some("cars"))
        );
        assert_eq!(
            category_hint("Новая карта для игры: город и трасса"),
            Some("maps")
        );
        assert_eq!(category_hint("Добавлены 5 конфигураций"), Some("cars"));
        // «карточка», «картон» — не карта; неизвестно — без типа, а не догадка.
        assert_eq!(category_hint("Исправлена карточка мода"), None);
        assert_eq!(category_hint("Звуки двигателя"), None);
    }

    #[test]
    fn update_mark() {
        assert!(is_update_marked("<div class=\"label\">ОБНОВЛЕНИЕ</div>"));
        assert!(!is_update_marked("<div>МОД</div>"));
    }

    #[test]
    fn real_search_card_keeps_exact_preview_and_author() {
        let list = parse_list(&fixture("search-beamng-2026-10-18.html"));
        assert_eq!(list.len(), 2);
        assert_eq!(
            list[0].title,
            "BMW E34 версия 3.3 для BeamNG.drive (v0.39.x)"
        );
        assert_eq!(list[0].mod_version.as_deref(), Some("3.3"));
        assert_eq!(list[0].author.as_deref(), Some("matissator"));
        assert_eq!(
            list[0].preview_url.as_ref().map(url::Url::as_str),
            Some(
                "https://stmods.org/beamng_drive/mods/bmw_e34_by_matissator_for_beamng/bmw_e34_by_matissator_for_beamng_img1pre.jpg?6acc95e59b8c2"
            )
        );
    }
}

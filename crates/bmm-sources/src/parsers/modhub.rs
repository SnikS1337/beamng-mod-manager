//! Разбор ModHub (modhub.us): списки, поиск и страница мода.
//!
//! Сайт общий для многих игр; BeamNG — раздел `/beamng-drive-mods/`. Размера файла
//! и автора в списке нет: автор — в «Credits» на странице мода, а загрузивший
//! («uploader») часто лишь перезаливает чужой мод. Версия — в названии
//! («Universal Monster Frame v4.1.96 0.40.x»).

use std::sync::LazyLock;

use regex::Regex;

use bmm_domain::models::{Listing, PriceStatus};
use bmm_domain::pricing::PaidDetector;
use bmm_domain::text::TitleParser;

use super::{clean, decode, join_url};

pub const SOURCE_ID: &str = "modhub";
pub const BASE: &str = "https://www.modhub.us/";
/// Раздел BeamNG в адресах модов и списков.
pub const SECTION: &str = "beamng-drive-mods";

/// Карточка списка. Блоки «TOP Mods» и «Popular» вверху и сбоку — другой разметки,
/// поэтому в выдачу не попадают.
static ITEM_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<div class="game-div">"#).unwrap());
static ITEM_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<h4>\s*<a href="(?<h>[^"]+)"[^>]*>(?<t>.*?)</a>\s*</h4>"#).unwrap()
});
static ITEM_IMAGE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)class="main-img"[^>]*background-image:\s*url\(\s*['"]?(?<u>[^'"\)]+)"#)
        .unwrap()
});
static H1: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<h1[^>]*>(?<t>.*?)</h1>").unwrap());
static CREDITS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div class="credits[^"]*">.*?<div class="content">(?<c>.*?)</div>\s*</div>"#)
        .unwrap()
});
static UPLOADER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)<h3 class="author-name">(?<a>.*?)</h3>"#).unwrap());
static MIRROR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)class="btn btn-mirror">.*?</i>\s*(?<h>[^<]+?)\s*</button>"#).unwrap()
});
static DESCRIPTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div class="description">.*?<div class="content">(?<d>.*?)</div>"#).unwrap()
});
/// «In versions 4.1.96:» / «Changes v1.3» в описании.
static DESCRIPTION_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:in versions?|changes|version)\s*:?\s*v?(?<v>\d+(?:\.\d+){1,3})\b")
        .unwrap()
});

/// Версия игры в конце названия: «… v1.3 0.40.x», «BMW M8 CS 0.39».
static TRAILING_GAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+(?<g>0\.[2-9]\d(?:\.(?:x|\d+))?)\s*$").unwrap());

/// Версии из названия ModHub. Сайт дописывает версию игры в конец названия без
/// пометки, и общий разбор принял бы «0.39» за версию мода.
pub fn versions(title: &str) -> (Option<String>, Option<String>) {
    let (rest, game) = match TRAILING_GAME.captures(title) {
        Some(captures) => (
            &title[..captures.get(0).map_or(title.len(), |found| found.start())],
            captures.name("g").map(|game| game.as_str().to_string()),
        ),
        None => (title, None),
    };
    let parsed = TitleParser::parse(rest);
    (parsed.mod_version, game.or(parsed.game_version))
}

pub fn parse_list(html: &str) -> Vec<Listing> {
    let starts: Vec<usize> = ITEM_START
        .find_iter(html)
        .map(|found| found.start())
        .collect();
    let mut result: Vec<Listing> = Vec::new();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(html.len());
        let body = &html[*start..end];
        // Рекламные блоки той же разметки — без заголовка-ссылки.
        let Some(link) = ITEM_TITLE.captures(body) else {
            continue;
        };
        let (Some(href), Some(title_html)) = (link.name("h"), link.name("t")) else {
            continue;
        };
        let title = clean(title_html.as_str());
        if title.is_empty() {
            continue;
        }
        let Some(url) = join_url(BASE, &decode(href.as_str())) else {
            continue;
        };
        if !is_mod_url(&url) {
            continue;
        }
        let source_id = slug_of(&url);
        if result.iter().any(|item| item.source_id == source_id) {
            continue;
        }
        let mut listing = listing_for(url, source_id, &title);
        listing.preview_url = ITEM_IMAGE
            .captures(body)
            .and_then(|captures| captures.name("u"))
            .and_then(|raw| own_image(&decode(raw.as_str())));
        result.push(listing);
    }
    result
}

pub fn parse_detail(html: &str, basis: &Listing) -> Listing {
    let mut listing = basis.clone();
    if let Some(title) = H1
        .captures(html)
        .and_then(|captures| captures.name("t"))
        .map(|title| clean(title.as_str()))
        .filter(|title| !title.is_empty())
    {
        let (mod_version, game_version) = versions(&title);
        listing.mod_version = mod_version.or(listing.mod_version);
        listing.game_version = game_version.or(listing.game_version);
        listing.title = title;
    }
    if listing.mod_version.is_none()
        && let Some(version) = DESCRIPTION
            .captures(html)
            .and_then(|captures| captures.name("d"))
            .map(|text| clean(text.as_str()))
            .and_then(|text| description_version(&text))
    {
        listing.mod_version = Some(version);
    }
    listing.author = credits(html).or_else(|| uploader(html)).or(listing.author);
    if listing.preview_url.is_none() {
        listing.preview_url = cover_of(html, &listing.source_id);
    }
    let price = PaidDetector::detect(&listing.title, None, &[]);
    if price != PriceStatus::Unknown {
        listing.price = price;
    }
    listing
}

/// Версия мода из описания. «Game version: 0.38» и «BeamNG version 0.40» — версия
/// игры, а не мода: приняв её за версию мода, программа сравнила бы «0.38» с
/// установленной «0.1» и показала ложное обновление.
fn description_version(text: &str) -> Option<String> {
    DESCRIPTION_VERSION
        .captures_iter(text)
        .find_map(|captures| {
            let whole = captures.get(0)?;
            let before = text[..whole.start()].to_lowercase();
            let tail: String = before.chars().rev().take(16).collect::<String>();
            let tail: String = tail.chars().rev().collect();
            let about_game = ["game", "beamng", "игры", "игра"]
                .iter()
                .any(|word| tail.contains(word));
            (!about_game).then(|| captures.name("v").map(|v| v.as_str().to_string()))?
        })
}

/// Файлообменники под кнопками скачивания («modsfire.com», «mods.to»): скачивание
/// идёт формой на сайте, прямой ссылки на странице нет.
pub fn mirrors(html: &str) -> Vec<String> {
    MIRROR
        .captures_iter(html)
        .filter_map(|captures| captures.name("h"))
        .map(|host| clean(host.as_str()).to_lowercase())
        .filter(|host| !host.is_empty())
        .collect()
}

fn listing_for(url: url::Url, source_id: String, title: &str) -> Listing {
    let (mod_version, game_version) = versions(title);
    let mut listing = Listing::new(SOURCE_ID, url, source_id, title);
    listing.mod_version = mod_version;
    listing.game_version = game_version;
    listing.price = PaidDetector::detect(title, None, &[]);
    listing
}

/// Авторы из «Credits»: первая строка после слова «Credits», имена через запятую.
fn credits(html: &str) -> Option<String> {
    let block = CREDITS.captures(html)?.name("c")?.as_str();
    let text = super::line_text(block);
    let names: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.eq_ignore_ascii_case("credits"))
        .flat_map(|line| line.split([',', ';']))
        .map(|name| name.trim().trim_end_matches('.').to_string())
        // Сайт-источник в «Credits» («mods.to», «modsfire.com») — не автор; а вот
        // «J.D. Smith» — автор, хоть и с точками.
        .filter(|name| !name.is_empty() && !looks_like_site(name) && name.chars().count() <= 40)
        .collect();
    (!names.is_empty()).then(|| names.join(", "))
}

/// «mods.to», «www.modsfire.com»: слова через точки без пробелов.
fn looks_like_site(name: &str) -> bool {
    !name.contains(' ')
        && name.contains('.')
        && name.split('.').all(|part| {
            !part.is_empty() && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}

fn uploader(html: &str) -> Option<String> {
    UPLOADER
        .captures(html)
        .and_then(|captures| captures.name("a"))
        .map(|name| clean(name.as_str()))
        .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("guest"))
}

/// Обложка мода: миниатюра с его адресом в имени файла.
fn cover_of(html: &str, slug: &str) -> Option<url::Url> {
    if slug.is_empty() {
        return None;
    }
    let marker = format!("/thumb_{slug}_");
    let at = html.find(&marker)?;
    let start = html[..at].rfind("/uploads/")?;
    let end = html[at..]
        .find(['"', '\'', ')', '&', ' '])
        .map(|offset| at + offset)?;
    own_image(&html[start..end])
}

fn own_image(raw: &str) -> Option<url::Url> {
    let url = join_url(BASE, raw)?;
    let host = url.host_str()?.to_ascii_lowercase();
    (url.scheme() == "https" && (host == "modhub.us" || host.ends_with(".modhub.us")))
        .then_some(url)
}

/// Страница мода BeamNG: `/beamng-drive-mods/{имя}`.
pub fn is_mod_url(url: &url::Url) -> bool {
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if host != "modhub.us" && !host.ends_with(".modhub.us") {
        return false;
    }
    let parts: Vec<&str> = url
        .path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect())
        .unwrap_or_default();
    parts.len() == 2 && parts[0].eq_ignore_ascii_case(SECTION)
}

pub fn slug_of(url: &url::Url) -> String {
    url.path()
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/modhub")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    fn page(slug: &str) -> Listing {
        let url = url::Url::parse(&format!("{BASE}{SECTION}/{slug}")).unwrap();
        Listing::new(SOURCE_ID, url, slug, slug)
    }

    #[test]
    fn list_reads_mods_but_not_top_and_ads() {
        let list = parse_list(&fixture("list-main-real.html"));
        assert_eq!(
            list.len(),
            8,
            "{:?}",
            list.iter().map(|item| &item.title).collect::<Vec<_>>()
        );
        assert_eq!(list[0].title, "BMW M8 CS 0.39");
        assert_eq!(list[0].source_id, "bmw-m8-cs-039");
        assert_eq!(
            list[0].url.as_str(),
            "https://www.modhub.us/beamng-drive-mods/bmw-m8-cs-039"
        );
        assert!(
            list[0]
                .preview_url
                .as_ref()
                .is_some_and(|url| url.as_str().contains("thumb_bmw-m8-cs-039_")),
            "{:?}",
            list[0].preview_url
        );
        assert!(list.iter().all(|item| item.source == SOURCE_ID));
    }

    #[test]
    fn versions_come_from_the_title() {
        let list = parse_list(&fixture("list-main-real.html"));
        let corvette = list
            .iter()
            .find(|item| item.title.starts_with("Chevrolet Corvette C6 v1.3"))
            .expect("Corvette в списке");
        assert_eq!(corvette.mod_version.as_deref(), Some("1.3"));
        assert_eq!(corvette.game_version.as_deref(), Some("0.40.x"));
    }

    #[test]
    fn category_pages_and_search_use_the_same_cards() {
        for name in [
            "list-cars-real.html",
            "list-maps-real.html",
            "list-other-page2-real.html",
            "search-bmw-real.html",
        ] {
            let list = parse_list(&fixture(name));
            assert_eq!(list.len(), 8, "{name}");
            assert!(list.iter().all(|item| is_mod_url(&item.url)), "{name}");
        }
        let bmw = parse_list(&fixture("search-bmw-real.html"));
        assert!(
            bmw.iter()
                .all(|item| item.title.to_lowercase().contains("bmw"))
        );
    }

    #[test]
    fn detail_reads_credits_versions_and_mirror() {
        let html = fixture("mod-monster-frame-real.html");
        let detail = parse_detail(&html, &page("universal-monster-frame-v4196-040x"));
        assert_eq!(detail.title, "Universal Monster Frame v4.1.96 0.40.x");
        assert_eq!(detail.mod_version.as_deref(), Some("4.1.96"));
        assert_eq!(detail.game_version.as_deref(), Some("0.40.x"));
        assert_eq!(
            detail.author.as_deref(),
            Some("Gameboy3800, O1ler, Abroopt, Nathan H, Ashmaker000"),
            "автор — из Credits, а не перезаливший"
        );
        assert!(detail.preview_url.as_ref().is_some_and(|url| {
            url.as_str()
                .contains("thumb_universal-monster-frame-v4196-040x_")
        }));
        assert_eq!(mirrors(&html), ["modsfire.com"]);
    }

    #[test]
    fn credits_skip_site_names() {
        let html = fixture("mod-bmw-m8-real.html");
        let detail = parse_detail(&html, &page("bmw-m8-cs-039"));
        assert_eq!(
            detail.author.as_deref(),
            Some("CarPay"),
            "«mods.to» — не автор"
        );
        assert_eq!(mirrors(&html), ["mods.to"]);
        assert_eq!(detail.game_version.as_deref(), Some("0.39"));
    }

    #[test]
    fn addon_page_has_version_and_host() {
        let html = fixture("mod-percheron-addon-real.html");
        let detail = parse_detail(&html, &page("cherrier-percheron-addon"));
        assert_eq!(detail.mod_version.as_deref(), Some("1.0"));
        assert_eq!(mirrors(&html), ["modsbase.com"]);
    }

    #[test]
    fn game_version_at_the_end_is_not_the_mod_version() {
        assert_eq!(versions("BMW M8 CS 0.39"), (None, Some("0.39".to_string())));
        assert_eq!(
            versions("Chevrolet Corvette C6 v1.3 0.40.x"),
            (Some("1.3".to_string()), Some("0.40.x".to_string()))
        );
        assert_eq!(
            versions("Cherrier Percheron Addon v1.0").0.as_deref(),
            Some("1.0")
        );
    }

    #[test]
    fn only_beamng_mod_pages_are_mod_urls() {
        let yes = url::Url::parse("https://www.modhub.us/beamng-drive-mods/bmw-m8-cs-039").unwrap();
        assert!(is_mod_url(&yes));
        for no in [
            "https://www.modhub.us/category/beamng-drive-mods/cars",
            "https://www.modhub.us/farming-simulator-25-mods/john-deere",
            "https://www.modhub.us/search/bmw/beamng-drive-mods",
            "https://evil.test/beamng-drive-mods/x",
        ] {
            assert!(!is_mod_url(&url::Url::parse(no).unwrap()), "{no}");
        }
    }

    #[test]
    fn game_version_in_description_is_not_the_mod_version() {
        assert_eq!(description_version("Game version: 0.38"), None);
        assert_eq!(description_version("Tested on BeamNG version 0.40"), None);
        assert_eq!(description_version("Версия игры: 0.38"), None);
        assert_eq!(
            description_version("Game version: 0.38. Changes v1.3: new wheels"),
            Some("1.3".to_string()),
            "версия игры пропускается, версия мода дальше находится"
        );
        assert_eq!(
            description_version("In versions 4.1.96: fixes"),
            Some("4.1.96".to_string())
        );
    }

    #[test]
    fn credits_keep_names_with_dots_but_drop_sites() {
        let html = r#"<div class="credits x"><div class="title">Credits</div><div class="content">J.D. Smith, mods.to, Car Maker</div></div>"#;
        assert_eq!(credits(html).as_deref(), Some("J.D. Smith, Car Maker"));
    }
}

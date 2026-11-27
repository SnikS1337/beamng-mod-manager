use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use bmm_domain::models::Listing;
use chrono::Utc;
use regex::Regex;
use url::Url;

#[derive(Clone, Copy)]
enum DetailRule {
    Offers,
    Marker(&'static str),
}

struct SiteSpec {
    id: &'static str,
    fixture_dir: &'static str,
    list_fixture: &'static str,
    list_url: &'static str,
    parse_list: fn(&str) -> Vec<Listing>,
    parse_detail: fn(&str, &Listing) -> Listing,
    detail_rule: DetailRule,
}

const SITES: [SiteSpec; 4] = [
    SiteSpec {
        id: "modland",
        fixture_dir: "modland",
        list_fixture: "list-real.html",
        list_url: "https://www.modland.net/beamng.drive-mods",
        parse_list: crate::parsers::modland::parse_list,
        parse_detail: crate::parsers::modland::parse_detail,
        detail_rule: DetailRule::Offers,
    },
    SiteSpec {
        id: "stmods",
        fixture_dir: "stmods",
        list_fixture: "list-real.html",
        list_url: "https://stmods.org/beamng_drive/",
        parse_list: crate::parsers::stmods::parse_list,
        parse_detail: crate::parsers::stmods::parse_detail,
        detail_rule: DetailRule::Offers,
    },
    SiteSpec {
        id: "modsgaming",
        fixture_dir: "modsgaming",
        list_fixture: "list-real.html",
        list_url: "https://www.modsgaming.us/load/beamng/70",
        parse_list: crate::parsers::modsgaming::parse_list,
        parse_detail: crate::parsers::modsgaming::parse_detail,
        detail_rule: DetailRule::Offers,
    },
    SiteSpec {
        id: "modhub",
        fixture_dir: "modhub",
        list_fixture: "list-main-real.html",
        list_url: "https://www.modhub.us/category/beamng-drive-mods",
        parse_list: crate::parsers::modhub::parse_list,
        parse_detail: crate::parsers::modhub::parse_detail,
        detail_rule: DetailRule::Marker("/mod/download"),
    },
];

fn is_blocked_status(status: u16) -> bool {
    status == 403
}

fn validate_list(site: &SiteSpec, html: &str) -> Result<Vec<Listing>, String> {
    let listings = (site.parse_list)(html);
    if listings.is_empty() {
        return Err(format!("{}: список распарсился в 0 карточек", site.id));
    }
    if listings
        .iter()
        .any(|listing| listing.title.trim().is_empty() || listing.url.host_str().is_none())
    {
        return Err(format!(
            "{}: карточка потеряла заголовок или ссылку",
            site.id
        ));
    }
    if !listings.iter().any(|listing| {
        listing.preview_url.is_some()
            || listing.author.is_some()
            || listing.category.is_some()
            || listing.size_bytes.is_some()
    }) {
        return Err(format!("{}: карточки потеряли метаданные", site.id));
    }
    Ok(listings)
}

fn validate_detail(site: &SiteSpec, html: &str, basis: &Listing) -> Result<Listing, String> {
    let detail = (site.parse_detail)(html, basis);
    if detail.title.trim().is_empty() {
        return Err(format!("{}: карточка потеряла заголовок", site.id));
    }
    match site.detail_rule {
        DetailRule::Offers => {
            if detail.offers.is_empty() {
                return Err(format!("{}: карточка потеряла блок скачивания", site.id));
            }
            if detail
                .offers
                .iter()
                .any(|offer| offer.url.host_str().is_none())
            {
                return Err(format!("{}: ссылка скачивания потеряла адрес", site.id));
            }
        }
        DetailRule::Marker(marker) => {
            if !html.contains(marker) {
                return Err(format!("{}: карточка потеряла форму скачивания", site.id));
            }
        }
    }
    Ok(detail)
}

static SCRIPT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<script\b[^>]*>.*?</script>").unwrap());
static STYLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?is)<style\b[^>]*>.*?</style>").unwrap());
static COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
static TOKEN_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<(?:meta|input)\b[^>]*(?:csrf|nonce|token|ssid)[^>]*>").unwrap()
});
static TOKEN_ATTR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?i)\s(?:data-csrf|csrf-token|nonce|data-token|authenticity_token)\s*=\s*(?:"[^"]*"|'[^']*'|[^\s>]+)"#).unwrap()
});
static EXTERNAL_MARKER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)External Download Links:").unwrap());

fn sanitize_html(html: &str, source_url: &str) -> String {
    let mut clean = SCRIPT.replace_all(html, "").into_owned();
    clean = STYLE.replace_all(&clean, "").into_owned();
    clean = COMMENT
        .replace_all(&clean, |captures: &regex::Captures<'_>| {
            if captures[0]
                .trim()
                .eq_ignore_ascii_case("<!-- end Files -->")
            {
                captures[0].to_string()
            } else {
                String::new()
            }
        })
        .into_owned();
    clean = TOKEN_TAG.replace_all(&clean, "").into_owned();
    clean = TOKEN_ATTR.replace_all(&clean, "").into_owned();
    format!(
        "<!-- saved from url={} at {} -->\n{}",
        source_url,
        Utc::now().format("%Y-%m-%d"),
        clean.trim()
    )
}

fn save_fixture(dir: &Path, name: &str, html: &str, source_url: &str) -> Result<PathBuf, String> {
    fs::create_dir_all(dir).map_err(|error| format!("не создана папка fixtures: {error}"))?;
    let path = dir.join(name);
    fs::write(&path, sanitize_html(html, source_url))
        .map_err(|error| format!("не сохранён {}: {error}", path.display()))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use bmm_domain::models::Listing;
    use url::Url;

    use super::{
        SITES, is_blocked_status, sanitize_html, save_fixture, validate_detail, validate_list,
    };

    #[test]
    fn sanitize_removes_active_and_service_markup_but_keeps_parser_content() {
        let html = r#"<!doctype html>
<html><head>
<style>.post { color: red; }</style>
<meta name="csrf-token" content="secret">
<script>window.token = "secret";</script>
</head><body>
<!-- internal comment -->
<input type="hidden" name="_token" value="secret">
<div class="post" data-csrf="secret"><a href="https://example.test/mod.html">Title</a></div>
</body></html>"#;

        let clean = sanitize_html(html, "https://example.test/page");

        assert!(!clean.to_lowercase().contains("<script"));
        assert!(!clean.to_lowercase().contains("<style"));
        assert!(!clean.contains("secret"));
        assert!(!clean.to_lowercase().contains("data-csrf"));
        assert!(!clean.contains("internal comment"));
        assert!(clean.contains("class=\"post\""));
        assert!(clean.contains("Title"));
        assert!(clean.contains("saved from url=https://example.test/page"));
    }

    #[test]
    fn sanitize_keeps_the_structural_files_marker() {
        let clean = sanitize_html(
            r#"<div id="files"><a href="/download/1">file</a></div><!-- end Files -->"#,
            "https://example.test/mod",
        );
        assert!(clean.contains("<!-- end Files -->"));
    }

    #[test]
    fn sanitize_removes_session_ids() {
        let clean = sanitize_html(
            r#"<input type="hidden" name="ssid" value="207710563511102552472"><div>Title</div>"#,
            "https://example.test/mod",
        );
        assert!(!clean.contains("ssid"));
        assert!(!clean.contains("207710563511102552472"));
    }

    #[test]
    fn real_list_fixtures_pass_the_layout_check() {
        for site in SITES {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures")
                .join(site.fixture_dir)
                .join(site.list_fixture);
            let html = std::fs::read_to_string(path).unwrap();
            let listings = validate_list(&site, &html).unwrap_or_else(|error| panic!("{error}"));
            assert!(!listings.is_empty(), "{}", site.id);
        }
    }

    #[test]
    fn layout_check_rejects_an_empty_list() {
        assert!(validate_list(&SITES[0], "<html><body>no cards</body></html>").is_err());
    }

    #[test]
    fn layout_check_rejects_a_detail_without_a_download_block() {
        let site = &SITES[1];
        let basis = Listing::new(
            site.id,
            Url::parse("https://stmods.org/beamng_drive/mods/test").unwrap(),
            "test",
            "Test",
        );
        assert!(validate_detail(site, "<html><h1>Test</h1></html>", &basis).is_err());
    }

    #[test]
    fn modhub_detail_is_checked_by_its_download_form() {
        let site = &SITES[3];
        let basis = Listing::new(
            site.id,
            Url::parse("https://www.modhub.us/beamng-drive-mods/test").unwrap(),
            "test",
            "Test",
        );
        assert!(validate_detail(site, "<html><h1>Test</h1></html>", &basis).is_err());
        assert!(
            validate_detail(
                site,
                r#"<html><h1>Test</h1><form action="https://www.modhub.us/mod/download" method="POST"></form></html>"#,
                &basis,
            )
            .is_ok()
        );
    }

    #[test]
    fn latest_live_fixtures_are_clean_and_parse() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        for site in SITES {
            let dir = root.join(site.fixture_dir);
            let Some(list_path) = std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            name.starts_with("list-live-") && name.ends_with(".html")
                        })
                })
                .max()
            else {
                continue;
            };
            let list_html = std::fs::read_to_string(&list_path).unwrap();
            assert!(!list_html.to_lowercase().contains("<script"), "{}", site.id);
            assert!(!list_html.to_lowercase().contains("<style"), "{}", site.id);
            assert!(
                !list_html.to_lowercase().contains("csrf-token"),
                "{}",
                site.id
            );
            assert!(!list_html.to_lowercase().contains("nonce="), "{}", site.id);
            assert!(!list_html.contains("name=\"ssid\""), "{}", site.id);
            let listings = validate_list(&site, &list_html).unwrap();
            let date = list_path
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| name.strip_prefix("list-live-"))
                .and_then(|name| name.strip_suffix(".html"))
                .unwrap();
            let detail_path = dir.join(format!("mod-live-{date}.html"));
            if detail_path.is_file() {
                let detail_html = std::fs::read_to_string(detail_path).unwrap();
                assert!(
                    !detail_html.to_lowercase().contains("<script"),
                    "{}",
                    site.id
                );
                assert!(
                    !detail_html.to_lowercase().contains("<style"),
                    "{}",
                    site.id
                );
                assert!(
                    !detail_html.to_lowercase().contains("csrf-token"),
                    "{}",
                    site.id
                );
                assert!(
                    !detail_html.to_lowercase().contains("nonce="),
                    "{}",
                    site.id
                );
                assert!(!detail_html.contains("name=\"ssid\""), "{}", site.id);
                validate_detail(&site, &detail_html, &listings[0]).unwrap();
            }
        }
    }

    #[test]
    fn cloudflare_forbidden_is_a_blocked_site_not_a_parser_failure() {
        assert!(is_blocked_status(403));
        assert!(!is_blocked_status(200));
        assert!(!is_blocked_status(404));
    }

    #[test]
    fn save_fixture_writes_only_sanitized_html() {
        let temp = tempfile::tempdir().unwrap();
        let path = save_fixture(
            temp.path(),
            "list-live-2026-11-27.html",
            "<script>secret</script><div class=\"post\">Title</div>",
            "https://example.test/list",
        )
        .unwrap();

        assert_eq!(path.file_name().unwrap(), "list-live-2026-11-27.html");
        let text = std::fs::read_to_string(path).unwrap();
        assert!(!text.contains("secret"));
        assert!(text.contains("Title"));
        assert!(text.contains("saved from url=https://example.test/list"));
    }
}

#[test]
#[ignore = "ходит на настоящие сайты"]
fn live_site_layouts() {
    use crate::net::{HttpPageFetcher, PageFetcher};

    let client = HttpPageFetcher::new().unwrap();
    let update = std::env::var_os("UPDATE_SITE_FIXTURES").is_some();
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut checked = 0;

    for site in SITES {
        let list_url = Url::parse(site.list_url).unwrap();
        let list = client.get(&list_url).unwrap_or_else(|error| {
            panic!("{}: не загружена лента {}: {error}", site.id, site.list_url)
        });
        if is_blocked_status(list.status) {
            println!(
                "{}: пропущен — сайт вернул 403 (Cloudflare/запрет)",
                site.id
            );
            continue;
        }
        assert_eq!(
            list.status, 200,
            "{}: лента вернула {} вместо 200",
            site.id, list.status
        );
        let listings = validate_list(&site, &list.text).unwrap_or_else(|error| panic!("{error}"));
        println!("{}: карточек в ленте — {}", site.id, listings.len());
        if update {
            let path = save_fixture(
                &fixtures.join(site.fixture_dir),
                &format!("list-live-{date}.html"),
                &list.text,
                list.final_url.as_str(),
            )
            .unwrap();
            println!("  сохранено: {}", path.display());
        }

        let basis = &listings[0];
        println!(
            "{}: проверяем карточку {} — {}",
            site.id, basis.url, basis.title
        );
        let detail = client.get(&basis.url).unwrap_or_else(|error| {
            panic!("{}: не загружена карточка {}: {error}", site.id, basis.url)
        });
        if is_blocked_status(detail.status) {
            println!("{}: карточка пропущена — сайт вернул 403", site.id);
            continue;
        }
        assert_eq!(
            detail.status, 200,
            "{}: карточка вернула {} вместо 200",
            site.id, detail.status
        );
        let parsed = validate_detail(&site, &detail.text, basis).unwrap_or_else(|error| {
            if let Some(found) = EXTERNAL_MARKER.find(&detail.text) {
                let snippet: String = detail.text[found.start()..].chars().take(6000).collect();
                println!("{snippet}");
            }
            panic!(
                "{error}; url={}; html={} bytes; external={}",
                basis.url,
                detail.text.len(),
                detail
                    .text
                    .to_lowercase()
                    .contains("external download links")
            )
        });
        checked += 1;
        println!(
            "{}: карточка «{}», ссылок скачивания — {}",
            site.id,
            parsed.title,
            parsed.offers.len()
        );
        if update {
            let path = save_fixture(
                &fixtures.join(site.fixture_dir),
                &format!("mod-live-{date}.html"),
                &detail.text,
                detail.final_url.as_str(),
            )
            .unwrap();
            println!("  сохранено: {}", path.display());
        }
    }
    println!("live-smoke: полностью проверено сайтов — {checked}");
}

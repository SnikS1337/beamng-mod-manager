//! Страница файла на modsfire.com: имя файла и размер, который пишет сайт.
//!
//! Через modsfire раздают моды STMods, ModsGaming и Modland. Размер на странице
//! округлён до сотых мегабайта («243.84 MB») — для сверки «тот же ли файл» этого
//! хватает, точным его не считаем.

use std::sync::LazyLock;

use regex::Regex;

use super::{clean, parse_size};

/// Что написано о файле на его странице.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedFile {
    pub name: String,
    pub size_bytes: Option<i64>,
}

static FILENAME_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<bdi[^>]*class="[^"]*mf-dl__filename[^"]*"[^>]*>(?<n>.*?)</bdi>"#).unwrap()
});
static SMALL_TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div[^>]*class="[^"]*small-title[^"]*"[^>]*>\s*<h3[^>]*>(?<n>.*?)</h3>"#)
        .unwrap()
});
static TITLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?is)<title>\s*Download file\s+(?<n>.+?)\s+-\s+[^<]*ModsFire").unwrap()
});
/// Блок «File size»: подсказка в `title` или (после скриптов) в `data-bs-original-title`.
static SIZE_BLOCK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?s)title="File size"[^>]*>(?<b>.*?)</p>"#).unwrap());
static DESCRIPTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?is)<meta[^>]*name="description"[^>]*content="Download file\s+(?<d>[^"]+)""#)
        .unwrap()
});

fn archive_name(raw: &str) -> Option<String> {
    let name = clean(raw);
    let lower = name.to_ascii_lowercase();
    (!name.is_empty()
        && name.len() <= 255
        && !name.contains(['/', '\\'])
        && [".zip", ".rar", ".7z"]
            .iter()
            .any(|extension| lower.ends_with(extension)))
    .then_some(name)
}

/// Страница «Page Not Found»: файла по этому адресу больше нет. Сайт отвечает так
/// с кодом 404, но браузерное окно кода не видит — узнаём по тексту.
pub fn is_missing(html: &str) -> bool {
    static MISSING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<title>\s*Page Not Found\s*-\s*ModsFire").unwrap());
    MISSING.is_match(html)
}

/// Имя и размер файла со страницы modsfire; `None` — это не страница файла
/// (заглушка Cloudflare, «файл удалён», другая вёрстка).
pub fn parse(html: &str) -> Option<HostedFile> {
    let name = [&*FILENAME_TAG, &*SMALL_TITLE, &*TITLE]
        .iter()
        .find_map(|regex| {
            regex
                .captures(html)
                .and_then(|captures| captures.name("n"))
                .and_then(|found| archive_name(found.as_str()))
        })?;
    let size_bytes = SIZE_BLOCK
        .captures(html)
        .and_then(|captures| captures.name("b"))
        .and_then(|block| parse_size(&clean(block.as_str())))
        .or_else(|| {
            // «Download file NAME - 243.84 MB MB from ModsFire.com…»
            DESCRIPTION
                .captures(html)
                .and_then(|captures| captures.name("d"))
                .and_then(|text| {
                    let text = text.as_str();
                    let after = text.strip_prefix(name.as_str()).unwrap_or(text);
                    parse_size(after)
                })
        });
    Some(HostedFile { name, size_bytes })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/modsfire/file-real.html");
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn real_file_page_gives_name_and_size() {
        let file = parse(&fixture()).expect("страница файла");
        assert_eq!(file.name, "E34_matissator_UNZIP.7z");
        assert_eq!(file.size_bytes, Some((243.84 * 1024.0 * 1024.0) as i64));
    }

    #[test]
    fn raw_page_before_scripts_still_reads() {
        // До скриптов подсказка лежит в `title`, а не в `data-bs-original-title`.
        let html = fixture().replace(r#"title="" data-bs-original-title="#, "title=");
        let file = parse(&html).unwrap();
        assert_eq!(file.name, "E34_matissator_UNZIP.7z");
        assert!(file.size_bytes.is_some());
    }

    #[test]
    fn size_comes_from_the_description_when_the_block_is_gone() {
        let html = r#"<title>Download file car_v2.zip - First Step - ModsFire.com</title>
<meta name="description" content="Download file car_v2.zip - 12.5 MB MB from ModsFire.com, without any restrictions">"#;
        let file = parse(html).unwrap();
        assert_eq!(file.name, "car_v2.zip");
        assert_eq!(file.size_bytes, Some((12.5 * 1024.0 * 1024.0) as i64));
    }

    #[test]
    fn version_like_digits_in_the_name_are_not_read_as_size() {
        // «2 MB» внутри имени не должен стать размером.
        let html = r#"<title>Download file pack_2_MB_edition_v1.0.zip - First Step - ModsFire.com</title>
<meta name="description" content="Download file pack_2_MB_edition_v1.0.zip - 300 KB MB from ModsFire.com">"#;
        let file = parse(html).unwrap();
        assert_eq!(file.size_bytes, Some(300 * 1024));
    }

    #[test]
    fn deleted_file_page_is_recognised() {
        // modsfire.com/2G9uMu8tD03V394 (Lada 2101 на ModsGaming), 2026-11-06.
        let html = "<html><head><title>Page Not Found - ModsFire.com</title></head><body>Page Not Found\nSorry, but the page you were trying to view does not exist.</body></html>";
        assert!(is_missing(html));
        assert_eq!(parse(html), None);
        assert!(!is_missing(&fixture()));
        assert!(
            !is_missing("<title>Just a moment...</title>"),
            "заглушка — не удаление"
        );
    }

    #[test]
    fn challenge_or_missing_file_is_not_a_file() {
        assert_eq!(
            parse("<title>Just a moment...</title><body>Checking</body>"),
            None
        );
        assert_eq!(parse("<title>File not found - ModsFire.com</title>"), None);
        // Имя не архива (страница чего-то другого) — тоже нет.
        let html = r#"<div class="small-title"><h3>Premium plans</h3></div>"#;
        assert_eq!(parse(html), None);
        let path = r#"<div class="small-title"><h3>../evil.zip</h3></div>"#;
        assert_eq!(parse(path), None);
    }
}

//! Общие помощники разбора HTML: сущности, теги, размеры, даты.

pub mod beamng;
pub mod modland;
pub mod modsgaming;
pub mod stmods;

use std::sync::LazyLock;

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, TimeZone, Utc};
use regex::Regex;

static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]+>").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
static SIZE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?<n>\d{1,3}(?:[ \u{a0}]\d{3})+(?:[.,]\d+)?|\d+(?:[.,]\d+)?)\s*(?<u>KB|MB|GB|КБ|МБ|ГБ)\b",
    )
    .unwrap()
});

pub fn decode(text: &str) -> String {
    html_escape::decode_html_entities(text).into_owned()
}

pub fn strip_tags(html: &str) -> String {
    TAGS.replace_all(html, " ").into_owned()
}

pub fn line_text(html: &str) -> String {
    decode(&TAGS.replace_all(html, "\n"))
}

pub fn clean(html: &str) -> String {
    let decoded = decode(&strip_tags(html));
    SPACES.replace_all(decoded.trim(), " ").into_owned()
}

pub fn parse_size(text: &str) -> Option<i64> {
    let caps = SIZE.captures(text)?;
    let number: String = caps
        .name("n")?
        .as_str()
        .chars()
        .filter(|ch| !ch.is_whitespace() && *ch != '\u{a0}')
        .collect::<String>()
        .replace(',', ".");
    let value: f64 = number.parse().ok()?;
    let unit = caps.name("u")?.as_str().to_uppercase();
    let factor = match unit.as_str() {
        "KB" | "КБ" => 1024.0,
        "MB" | "МБ" => 1024.0 * 1024.0,
        _ => 1024.0 * 1024.0 * 1024.0,
    };
    Some((value * factor) as i64)
}

pub fn parse_epoch(value: &str) -> Option<DateTime<FixedOffset>> {
    let seconds: i64 = value.trim().parse().ok()?;
    Utc.timestamp_opt(seconds, 0)
        .single()
        .map(|date| date.fixed_offset())
}

pub fn parse_listing_date(text: &str) -> Option<DateTime<FixedOffset>> {
    let cleaned = text.trim().replace(" at ", " ");
    let formats = ["%b %d, %Y %I:%M %p", "%b %d, %Y", "%b %d, %Y %H:%M"];
    for format in formats {
        if let Ok(date) = NaiveDateTime::parse_from_str(&cleaned, format) {
            return Some(Utc.from_utc_datetime(&date).fixed_offset());
        }
        if let Ok(date) = NaiveDate::parse_from_str(&cleaned, format) {
            return date
                .and_hms_opt(0, 0, 0)
                .map(|date| Utc.from_utc_datetime(&date).fixed_offset());
        }
    }
    None
}

pub fn join_url(base: &str, href: &str) -> Option<url::Url> {
    let href = decode(href);
    let base = url::Url::parse(base).ok()?;
    base.join(href.trim()).ok()
}

pub fn is_http(url: &url::Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_entities_and_tags() {
        assert_eq!(clean("<b>Toyota</b> &amp; <i>BMW</i>"), "Toyota & BMW");
        assert_eq!(parse_size("1 234,5 МБ"), Some(1_294_467_072));
        assert_eq!(parse_size("517.85 MB"), Some(543_005_081));
        assert_eq!(parse_size("1,5 GB"), Some(1_610_612_736));
    }

    #[test]
    fn parses_epoch_and_xenforo_date() {
        assert_eq!(
            parse_epoch("1791897661").unwrap().timestamp(),
            1_791_897_661
        );
        assert!(parse_listing_date("Oct 13, 2026 at 3:17 PM").is_some());
        assert!(parse_listing_date("Oct 13, 2026").is_some());
    }
}

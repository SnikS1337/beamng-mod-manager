//! Канал Telegram с модами: открытая веб-версия `t.me/s/<канал>`.
//!
//! Выпуск мода в канале — два сообщения подряд: пост с картинками и текстом
//! («Audi A4/S4 B8 v1.9», описание) и следом пост с архивом
//! (`Audi_A4_S8_B8_tg_m0dsbeamng.zip`, 230.7 MB). Каждая новая версия — новый пост,
//! поэтому мод узнаём не по адресу, а по названию без версии ([`identity`]).
//! Посты без архива (новости, скриншоты) — не выпуски.

use std::sync::LazyLock;

use chrono::{DateTime, FixedOffset};
use regex::Regex;
use url::Url;

use bmm_domain::filename::from_file_name;
use bmm_domain::models::{Listing, Offer, OfferKind, PriceStatus};
use bmm_domain::text::{TitleParser, fold_latin};

use super::{decode, parse_size};

pub const SOURCE_ID: &str = "telegram";
pub const CHANNEL: &str = "m0dsbeamng";
pub const HOST: &str = "t.me";

/// Пост с архивом идёт за постом с текстом не дальше стольких номеров: номера
/// между ними занимают картинки альбома (у Telegram их до десяти).
const MAX_ID_GAP: i64 = 12;
/// …и не позже: архив выкладывают сразу за описанием.
const MAX_SECONDS_GAP: i64 = 15 * 60;
/// Название из первой строки поста — не длиннее (дальше обычно описание).
const MAX_TITLE: usize = 120;

/// Одно сообщение канала, как его показывает веб-версия.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub id: i64,
    pub at: Option<DateTime<FixedOffset>>,
    /// Текст поста построчно; `None` — у поста нет текста.
    pub text: Option<String>,
    pub files: Vec<PostedFile>,
    /// Первая картинка поста или альбома.
    pub photo: Option<Url>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostedFile {
    pub name: String,
    /// Размер, как его пишет Telegram («230.7 MB»): округлён до десятых.
    pub size_bytes: Option<i64>,
    /// Номер поста с этим файлом.
    pub post: i64,
}

/// Страница веб-версии: сообщения от старых к новым и откуда грузить более старые.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Page {
    pub messages: Vec<Message>,
    /// `data-before` кнопки «ещё»: номер самого старого сообщения на странице.
    pub before: Option<i64>,
    /// Это страница канала (а не заглушка, ошибка или чужая вёрстка).
    pub channel: bool,
}

/// Выпуск мода: пост с названием и архивы за ним.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    /// Пост с названием (у выпуска без текста — пост с архивом).
    pub post: i64,
    pub title: String,
    pub files: Vec<PostedFile>,
    pub at: Option<DateTime<FixedOffset>>,
    pub photo: Option<Url>,
}

static MESSAGE_START: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<div class="tgme_widget_message_wrap[^"]*"[^>]*>"#).unwrap());
static POST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"data-post="(?<c>[A-Za-z0-9_]+)/(?<id>\d+)""#).unwrap());
static TIME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"<time[^>]*datetime="(?<t>[^"]+)""#).unwrap());
static TEXT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)<div class="tgme_widget_message_text[^"]*"[^>]*>(?<t>.*?)</div>"#).unwrap()
});
static DOCUMENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?s)<a class="tgme_widget_message_document_wrap"[^>]*href="(?<h>[^"]*)"[^>]*>.*?tgme_widget_message_document_title[^>]*>(?<n>[^<]*)<.*?tgme_widget_message_document_extra[^>]*>(?<s>[^<]*)<"#,
    )
    .unwrap()
});
static PHOTO: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"tgme_widget_message_photo_wrap[^>]*background-image:url\('(?<u>https://[^']+)'\)"#,
    )
    .unwrap()
});
static MORE_BEFORE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"tme_messages_more[^>]*data-before="(?<b>\d+)""#).unwrap());
static HISTORY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"class="tgme_channel_history"#).unwrap());
static BY_AUTHOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[\s,(])(?:by|от)\s+(?<a>[\p{L}\p{Nd}_.\- ]{2,40}?)\s*\)?\s*$").unwrap()
});
static PRICE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\d+\s*(?:rub|руб|₽|\$|usd|eur|€)(?:[^\p{L}]|$)").unwrap());
static SPACES: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s+").unwrap());
/// Версия, которую общий разбор названий не берёт: «(V1.0)», «версия 2.8.0».
static LOOSE_VERSION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?:^|[\s(\[,])(?:v|ver\.?|version|версия)\s*(?<v>\d{1,3}(?:\.\d{1,4}){0,3}[a-z]?)(?:$|[\s)\],])",
    )
    .unwrap()
});
static BREAK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
static INLINE_TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<[^>]+>").unwrap());
/// Слова, которые пишут рядом с версией, а не в названии мода.
const NOT_NAME: &[&str] = &[
    "update",
    "upd",
    "updated",
    "release",
    "released",
    "new",
    "fix",
    "hotfix",
    "patch",
    "обновление",
    "обновлен",
    "обновлён",
    "обновленный",
    "обновлённый",
    "обнова",
    "релиз",
    "версия",
    "version",
    "ver",
    "публичная",
    "public",
];

/// Сообщения со страницы `t.me/s/<канал>` (лента, поиск, окрестность поста).
pub fn parse_page(html: &str) -> Page {
    let mut page = Page {
        channel: HISTORY.is_match(html),
        before: MORE_BEFORE
            .captures(html)
            .and_then(|captures| captures["b"].parse().ok()),
        ..Page::default()
    };
    let starts: Vec<usize> = MESSAGE_START
        .find_iter(html)
        .map(|found| found.start())
        .collect();
    for (index, start) in starts.iter().enumerate() {
        let end = starts.get(index + 1).copied().unwrap_or(html.len());
        if let Some(message) = parse_message(&html[*start..end]) {
            page.messages.push(message);
        }
    }
    // Веб-версия отдаёт сообщения от старых к новым; поиск — тоже. Не полагаемся.
    page.messages.sort_by_key(|message| message.id);
    page.messages.dedup_by_key(|message| message.id);
    page
}

fn parse_message(block: &str) -> Option<Message> {
    let post = POST.captures(block)?;
    if !post["c"].eq_ignore_ascii_case(CHANNEL) {
        // Пересланное из другого канала показывается под его адресом — не наш пост.
        return None;
    }
    let id: i64 = post["id"].parse().ok()?;
    let at = TIME
        .captures(block)
        .and_then(|captures| DateTime::parse_from_rfc3339(&captures["t"]).ok());
    let text = TEXT
        .captures(block)
        .map(|captures| text_of(&captures["t"]))
        .filter(|text| !text.trim().is_empty());
    let files = DOCUMENT
        .captures_iter(block)
        .filter_map(|captures| {
            let name = decode(captures["n"].trim()).trim().to_string();
            let archive = {
                let lower = name.to_lowercase();
                [".zip", ".rar", ".7z"]
                    .iter()
                    .any(|ext| lower.ends_with(ext))
            };
            (archive && !name.contains(['/', '\\'])).then(|| PostedFile {
                post: post_id_of(&captures["h"]).unwrap_or(id),
                size_bytes: parse_size(&decode(&captures["s"])),
                name,
            })
        })
        .collect();
    let photo = PHOTO
        .captures(block)
        .and_then(|captures| Url::parse(&decode(&captures["u"])).ok());
    Some(Message {
        id,
        at,
        text,
        files,
        photo,
    })
}

/// Текст поста: строки по `<br>`, без тегов и сущностей. Встроенные теги (жирный,
/// подсветка найденного в поиске `<mark>`) строку не рвут: «<mark>Audi</mark> A4».
fn text_of(html: &str) -> String {
    let with_breaks = BREAK.replace_all(html, "\n");
    decode(&INLINE_TAG.replace_all(&with_breaks, ""))
        .lines()
        .map(|line| SPACES.replace_all(line.trim(), " ").into_owned())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn post_id_of(href: &str) -> Option<i64> {
    let url = Url::parse(&decode(href)).ok()?;
    post_id(&url)
}

/// Номер поста из адреса `t.me/<канал>/<номер>` или `t.me/s/<канал>/<номер>`.
pub fn post_id(url: &Url) -> Option<i64> {
    if !is_channel_url(url) {
        return None;
    }
    let parts: Vec<&str> = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect();
    let rest = match parts.as_slice() {
        ["s", channel, rest @ ..] | [channel, rest @ ..]
            if channel.eq_ignore_ascii_case(CHANNEL) =>
        {
            rest
        }
        _ => return None,
    };
    match rest {
        [id] => id.parse().ok().filter(|id: &i64| *id > 0),
        _ => None,
    }
}

/// Адрес на `t.me` этого канала (любая его страница).
pub fn is_channel_url(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    if host != HOST && host != "www.t.me" && host != "telegram.me" {
        return false;
    }
    let mut parts = url
        .path_segments()
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty());
    match parts.next() {
        Some("s") => parts
            .next()
            .is_some_and(|channel| channel.eq_ignore_ascii_case(CHANNEL)),
        Some(channel) => channel.eq_ignore_ascii_case(CHANNEL),
        None => false,
    }
}

pub fn post_url(id: i64) -> Url {
    Url::parse(&format!("https://{HOST}/{CHANNEL}/{id}")).unwrap()
}

/// Ссылка, которая открывает пост сразу в приложении Telegram.
pub fn app_link(id: i64) -> String {
    format!("tg://resolve?domain={CHANNEL}&post={id}")
}

/// Выпуски на странице и самый новый «осиротевший» архив в её начале.
///
/// Архив, чей пост с названием остался на более старой странице, здесь не выпуск:
/// следующую страницу грузим так, чтобы он попал на неё вместе с названием
/// (см. [`next_before`]).
pub fn releases(messages: &[Message]) -> (Vec<Release>, Option<i64>) {
    collect(messages, false)
}

/// Как [`releases`], но для поиска: поиск показывает только посты, где нашлись слова,
/// и пост с архивом часто в выдачу не попадает. Описание с картинками там тоже выпуск —
/// без архива и размера, но с названием и версией.
pub fn search_releases(messages: &[Message]) -> Vec<Release> {
    collect(messages, true).0
}

fn collect(messages: &[Message], loose: bool) -> (Vec<Release>, Option<i64>) {
    let mut found: Vec<Release> = Vec::new();
    let mut open: Option<Release> = None;
    let mut orphan: Option<i64> = None;
    let mut seen_text = false;
    let close = |open: &mut Option<Release>, found: &mut Vec<Release>| {
        if let Some(release) = open.take()
            && (!release.files.is_empty() || (loose && release.photo.is_some()))
        {
            found.push(release);
        }
    };
    for message in messages {
        let title = message.text.as_deref().and_then(title_of);
        match (title, message.files.is_empty()) {
            // Пост с текстом без архива: описание выпуска (архив следом) или новость.
            (Some(title), true) => {
                close(&mut open, &mut found);
                seen_text = true;
                open = Some(Release {
                    post: message.id,
                    title,
                    files: Vec::new(),
                    at: message.at,
                    photo: message.photo.clone(),
                });
            }
            // Архив с подписью — выпуск сам по себе.
            (Some(title), false) => {
                close(&mut open, &mut found);
                seen_text = true;
                found.push(Release {
                    post: message.id,
                    title,
                    files: message.files.clone(),
                    at: message.at,
                    photo: message.photo.clone(),
                });
            }
            // Архив без подписи: к описанию перед ним, если оно рядом.
            (None, false) => {
                let fits = open.as_ref().is_some_and(|release| {
                    message.id - release.post <= MAX_ID_GAP
                        && match (release.at, message.at) {
                            (Some(text), Some(file)) => {
                                (file - text).num_seconds().abs() <= MAX_SECONDS_GAP
                            }
                            _ => true,
                        }
                });
                if fits {
                    if let Some(release) = open.as_mut() {
                        release.files.extend(message.files.iter().cloned());
                    }
                } else if !loose && !seen_text && found.is_empty() && open.is_none() {
                    // Начало страницы ленты: описание — на более старой. В поиске
                    // такого нет: поиск часто находит только пост с архивом по имени
                    // файла («hawaii» → hawaii_haleakala_maui_v1.2.1_tg_m0dsbeamng.zip),
                    // и это выпуск — с названием из имени файла.
                    orphan = Some(message.id);
                } else {
                    close(&mut open, &mut found);
                    if let Some(title) = message
                        .files
                        .first()
                        .and_then(|file| title_from_file(&file.name))
                    {
                        found.push(Release {
                            post: message.id,
                            title,
                            files: message.files.clone(),
                            at: message.at,
                            photo: message.photo.clone(),
                        });
                    }
                }
            }
            // Картинка или стикер без текста: не прерывает выпуск.
            (None, true) => {}
        }
    }
    close(&mut open, &mut found);
    (found, orphan)
}

/// `before` для следующей, более старой страницы. Архив без названия в начале этой
/// страницы попадает на следующую вместе со своим описанием. Страница из одних таких
/// архивов не держит на месте: идём дальше по `data-before`.
pub fn next_before(page: &Page, orphan: Option<i64>, current: Option<i64>) -> Option<i64> {
    let oldest = page
        .before
        .or_else(|| page.messages.first().map(|message| message.id))?;
    let wanted = match orphan {
        Some(id) => id + 1,
        None => oldest,
    };
    let limit = current.unwrap_or(i64::MAX);
    if wanted < limit && wanted > 1 {
        Some(wanted)
    } else if oldest < limit && oldest > 1 {
        Some(oldest)
    } else {
        None
    }
}

/// Название выпуска — первая строка поста без значков и хэштегов.
pub fn title_of(text: &str) -> Option<String> {
    let line = text.lines().map(str::trim).find(|line| {
        let cleaned = clean_title(line);
        !cleaned.is_empty() && !cleaned.starts_with('#')
    })?;
    let cleaned = clean_title(line);
    (cleaned.chars().any(char::is_alphanumeric)).then(|| shorten(&cleaned))
}

fn clean_title(line: &str) -> String {
    let kept: String = line
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch.is_whitespace() || ".,-–—_/()[]&+'’:!#№%".contains(ch)
            {
                ch
            } else {
                ' '
            }
        })
        .collect();
    // «… #audi» в конце строки — тег, не название.
    let without_tags: Vec<&str> = kept
        .split_whitespace()
        .filter(|word| !(word.starts_with('#') && word.len() > 1))
        .collect();
    let joined = without_tags.join(" ");
    let trimmed =
        joined.trim_matches(|ch: char| matches!(ch, ' ' | '-' | '–' | '—' | ',' | ':' | '!'));
    if trimmed.is_empty() && kept.trim().starts_with('#') {
        return "#".to_string();
    }
    trimmed.to_string()
}

fn shorten(title: &str) -> String {
    if title.chars().count() <= MAX_TITLE {
        return title.to_string();
    }
    let cut: String = title.chars().take(MAX_TITLE).collect();
    match cut.rfind(' ') {
        Some(space) if space > MAX_TITLE / 2 => cut[..space].to_string(),
        _ => cut,
    }
}

fn title_from_file(file_name: &str) -> Option<String> {
    let guessed = from_file_name(file_name);
    let name = guessed.name.trim();
    if name.is_empty() {
        return None;
    }
    Some(match guessed.version {
        Some(version) => format!("{name} v{version}"),
        None => name.to_string(),
    })
}

/// Название без автора и версии — по нему один и тот же мод узнаётся во всех постах:
/// «Audi A4/S4 B8 v1.8» и «Audi A4/S4 B8 v1.9» — один мод. Сверх общего разбора
/// названий убираем то, что канал пишет рядом с версией: «update», «публичная
/// версия», цену («- 500 rub»).
pub fn identity(title: &str) -> String {
    let (name, _) = split_author(&PRICE.replace_all(title, " "));
    let parsed = TitleParser::parse(&name);
    let base = if parsed.name.trim().is_empty() {
        name
    } else {
        parsed.name
    };
    let folded = fold_latin(&base).to_lowercase();
    let words: Vec<String> = folded
        .split(|ch: char| !ch.is_alphanumeric())
        .filter(|word| !word.is_empty() && !NOT_NAME.contains(word))
        .map(str::to_string)
        .collect();
    words.join(" ")
}

/// «2004-2011 Audi A6 (C6) V0.5.4, by fastlane» → название и автор.
fn split_author(title: &str) -> (String, Option<String>) {
    match BY_AUTHOR.captures(title) {
        Some(captures) => {
            let whole = captures.get(0).map_or(title.len(), |found| found.start());
            let author = captures["a"].trim().to_string();
            let name = title[..whole]
                .trim_end_matches([' ', ',', '-', '(', '|'])
                .to_string();
            if name.trim().is_empty() {
                (title.to_string(), None)
            } else {
                (name, Some(author))
            }
        }
        None => (title.to_string(), None),
    }
}

/// Запись для проверки и поиска.
pub fn to_listing(release: &Release) -> Listing {
    let (name, author) = split_author(&release.title);
    let parsed = TitleParser::parse(&name);
    let mut listing = Listing::new(
        SOURCE_ID,
        post_url(release.post),
        identity(&release.title),
        release.title.clone(),
    );
    listing.mod_version = parsed
        .mod_version
        .or_else(|| {
            LOOSE_VERSION
                .captures(&name)
                .map(|captures| captures["v"].to_string())
        })
        .or_else(|| {
            release
                .files
                .first()
                .and_then(|file| from_file_name(&file.name).version)
        });
    listing.game_version = parsed.game_version;
    listing.author = author;
    // Несколько архивов (части, варианты) — один размер не сравнить с одним файлом.
    listing.size_bytes = match release.files.as_slice() {
        [single] => single.size_bytes,
        _ => None,
    };
    listing.preview_url = release.photo.clone();
    listing.price = if parsed.has_paid_marker || PRICE.is_match(&release.title) {
        PriceStatus::Paid
    } else {
        PriceStatus::Free
    };
    listing.offers = release
        .files
        .iter()
        .map(|file| Offer {
            kind: OfferKind::ExternalHost,
            host: HOST.to_string(),
            url: post_url(file.post),
            host_file_id: Some(file.name.clone()),
            size_bytes: file.size_bytes,
        })
        .collect();
    listing.published_at = release.at;
    listing.updated_at = release.at;
    listing.external_id = Some(release.post.to_string());
    listing
}

/// Выпуски страницы, новые первыми, — как записи.
pub fn parse_listings(html: &str) -> Vec<Listing> {
    let page = parse_page(html);
    let found = search_releases(&page.messages);
    let mut listings: Vec<Listing> = found.iter().rev().map(to_listing).collect();
    // Один мод в выдаче — одна запись, самая новая.
    let mut seen = std::collections::HashSet::new();
    listings
        .retain(|listing| !listing.source_id.is_empty() && seen.insert(listing.source_id.clone()));
    listings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/telegram")
            .join(name);
        std::fs::read_to_string(path).unwrap()
    }

    fn at(text: &str) -> Option<DateTime<FixedOffset>> {
        DateTime::parse_from_rfc3339(text).ok()
    }

    fn text_post(id: i64, time: &str, text: &str) -> Message {
        Message {
            id,
            at: at(time),
            text: Some(text.to_string()),
            files: Vec::new(),
            photo: None,
        }
    }

    fn file_post(id: i64, time: &str, name: &str, size: Option<i64>) -> Message {
        Message {
            id,
            at: at(time),
            text: None,
            files: vec![PostedFile {
                name: name.to_string(),
                size_bytes: size,
                post: id,
            }],
            photo: None,
        }
    }

    #[test]
    fn feed_page_gives_releases_with_titles_files_and_dates() {
        let page = parse_page(&fixture("feed-real.html"));
        assert!(page.channel);
        assert_eq!(page.before, Some(18345));
        let (found, orphan) = releases(&page.messages);
        assert_eq!(orphan, None);
        let titles: Vec<&str> = found.iter().map(|release| release.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "ZL9 Wheelpack v3.1",
                "GMC Yukon 2021-2024 v1.0",
                "Audi A4/S4 B8 v1.9"
            ],
            "новости, скриншоты и посты без архива — не выпуски"
        );
        let audi = &found[2];
        assert_eq!(audi.post, 18360);
        assert_eq!(audi.files.len(), 1);
        assert_eq!(audi.files[0].name, "Audi_A4_S8_B8_tg_m0dsbeamng.zip");
        assert_eq!(audi.files[0].post, 18365);
        assert_eq!(audi.files[0].size_bytes, parse_size("230.7 MB"));
        assert_eq!(audi.at, at("2026-11-06T13:48:29+00:00"));
        assert!(
            audi.photo
                .as_ref()
                .is_some_and(|url| url.host_str().unwrap().ends_with("telesco.pe"))
        );

        let listing = to_listing(audi);
        assert_eq!(listing.source, SOURCE_ID);
        assert_eq!(listing.url.as_str(), "https://t.me/m0dsbeamng/18360");
        assert_eq!(listing.mod_version.as_deref(), Some("1.9"));
        assert_eq!(listing.source_id, identity("Audi A4/S4 B8 v1.8"));
        assert_eq!(
            listing.offers[0].url.as_str(),
            "https://t.me/m0dsbeamng/18365"
        );
        assert_eq!(listing.price, PriceStatus::Free);
    }

    #[test]
    fn archive_at_the_start_of_a_page_waits_for_its_title() {
        // Пост с архивом RLS 2.4 — первый на странице, его описание — на более старой.
        let page = parse_page(&fixture("feed-before-real.html"));
        let (found, orphan) = releases(&page.messages);
        assert_eq!(orphan, Some(18315));
        assert!(
            !found
                .iter()
                .any(|release| release.files.iter().any(|file| file.post == 18315))
        );
        let titles: Vec<&str> = found.iter().map(|release| release.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "RLS Career Overhaul, публичная версия 2.8.0",
                "Ibishu Yoru Evo v1.2",
                "Custom Neck Fx (Pov Camera) v1.0",
                "Audi RS6 C8 v1.4",
            ]
        );
        // Следующая страница начнётся с этого архива — вместе с его описанием.
        assert_eq!(next_before(&page, orphan, Some(18345)), Some(18316));
        assert_eq!(next_before(&page, None, Some(18345)), Some(18315));
        let rls = to_listing(&found[0]);
        assert_eq!(rls.mod_version.as_deref(), Some("2.8.0"));
        let neck = to_listing(&found[2]);
        assert_eq!(neck.size_bytes, Some(13_619), "13.3 KB");
    }

    #[test]
    fn page_of_only_orphans_does_not_stall() {
        let page = Page {
            messages: vec![file_post(500, "2026-11-01T10:00:00+00:00", "a.zip", None)],
            before: Some(500),
            channel: true,
        };
        let (found, orphan) = releases(&page.messages);
        assert!(found.is_empty());
        assert_eq!(orphan, Some(500));
        assert_eq!(next_before(&page, orphan, Some(600)), Some(501));
        // 501 вернул бы ту же страницу: идём дальше по data-before.
        assert_eq!(next_before(&page, orphan, Some(501)), Some(500));
        // Дальше некуда — стоп, а не та же страница по кругу.
        assert_eq!(next_before(&page, orphan, Some(500)), None);
        assert_eq!(next_before(&Page::default(), None, None), None);
        let first = Page {
            messages: vec![text_post(1, "2026-11-01T10:00:00+00:00", "x")],
            before: Some(1),
            channel: true,
        };
        assert_eq!(
            next_before(&first, None, None),
            None,
            "старше первого поста ничего нет"
        );
    }

    #[test]
    fn search_page_lists_every_version_newest_first() {
        let listings = parse_listings(&fixture("search-a4-real.html"));
        let a4 = listings
            .iter()
            .find(|listing| listing.source_id == identity("Audi A4/S4 B8 v1.9"))
            .unwrap();
        assert_eq!(
            a4.mod_version.as_deref(),
            Some("1.9"),
            "одна запись — самая новая"
        );
        assert_eq!(a4.url.as_str(), "https://t.me/m0dsbeamng/18360");
        let ids: Vec<&str> = listings
            .iter()
            .map(|listing| listing.source_id.as_str())
            .collect();
        let unique: std::collections::HashSet<&&str> = ids.iter().collect();
        assert_eq!(unique.len(), ids.len());
        // Поиск показывает и описания, чей архив не попал в выдачу: это тоже выпуски.
        let b6 = listings
            .iter()
            .find(|listing| listing.source_id == identity("Audi A4/S4 B6 (V1.0)"))
            .unwrap();
        assert_eq!(b6.mod_version.as_deref(), Some("1.0"), "«(V1.0)»");
        assert!(listings.len() >= 6, "{}", listings.len());
        // Старый пост «- 500 rub» — тот же мод: в выдаче одна запись, самая новая.
        assert!(
            !listings
                .iter()
                .any(|listing| listing.title.contains("500 rub"))
        );
        let paid = to_listing(&Release {
            post: 12696,
            title: "Audi A4/S4 B8 - 500 rub".to_string(),
            files: Vec::new(),
            at: None,
            photo: None,
        });
        assert_eq!(paid.price, PriceStatus::Paid);
        assert_eq!(
            paid.source_id,
            identity("Audi A4/S4 B8 v1.9"),
            "цена — не часть названия"
        );
    }

    #[test]
    fn search_hit_on_the_archive_alone_is_a_release() {
        let messages = vec![file_post(
            17000,
            "2026-08-01T10:00:00+00:00",
            "hawaii_haleakala_maui_v1.2.1_tg_m0dsbeamng.zip",
            Some(300_000_000),
        )];
        let found = search_releases(&messages);
        assert_eq!(found.len(), 1);
        let listing = to_listing(&found[0]);
        assert_eq!(listing.title, "hawaii haleakala maui v1.2.1");
        assert_eq!(listing.mod_version.as_deref(), Some("1.2.1"));
        assert_eq!(listing.url.as_str(), "https://t.me/m0dsbeamng/17000");
        assert_eq!(listing.size_bytes, Some(300_000_000));
        // В ленте тот же пост в начале страницы — ждёт описания со следующей.
        let (strict, orphan) = releases(&messages);
        assert!(strict.is_empty());
        assert_eq!(orphan, Some(17000));
    }

    #[test]
    fn empty_search_is_a_channel_page_without_posts() {
        let page = parse_page(&fixture("search-none-real.html"));
        assert!(page.channel);
        assert!(page.messages.is_empty());
        assert!(parse_listings(&fixture("search-none-real.html")).is_empty());
        let other = parse_page("<html><title>Just a moment...</title></html>");
        assert!(!other.channel);
    }

    #[test]
    fn titles_lose_emoji_tags_and_stay_short() {
        assert_eq!(
            title_of("🔥 Audi RS6 C8 v1.4 🔥\nописание").as_deref(),
            Some("Audi RS6 C8 v1.4")
        );
        assert_eq!(
            title_of("#audi\nAudi A4 B5 (V1.0)").as_deref(),
            Some("Audi A4 B5 (V1.0)")
        );
        assert_eq!(
            title_of("Audi A4 B9⚡️ #Audi").as_deref(),
            Some("Audi A4 B9")
        );
        assert_eq!(title_of("🔥🔥🔥"), None);
        assert_eq!(title_of("   \n  "), None);
        let long = format!("Mod {}", "слово ".repeat(60));
        assert!(title_of(&long).unwrap().chars().count() <= MAX_TITLE);
    }

    #[test]
    fn identity_ignores_version_author_and_case() {
        let same = [
            "2004-2011 Audi A6/RS6 (C6) V0.5.3, by fastlane",
            "2004-2011 Audi A6/RS6 (C6) V0.5.4, by fastlane",
            "2004-2011 audi a6/rs6 (c6) v0.6",
        ];
        let keys: Vec<String> = same.iter().map(|title| identity(title)).collect();
        assert!(keys.windows(2).all(|pair| pair[0] == pair[1]), "{keys:?}");
        assert_ne!(
            identity("Audi A4/S4 B8 v1.9"),
            identity("Audi A4/S4 B6 (V1.0)")
        );
        assert_eq!(
            identity("Audi A/S/RS5 B10 2026, update V1.3.1"),
            identity("Audi A/S/RS5 B10 2026, update V1.2")
        );
        let rs6 = to_listing(&Release {
            post: 1,
            title: "Audi RS6 Sedan RELEASE by BlueMods, v1.1".to_string(),
            files: vec![PostedFile {
                name: "rs6.zip".to_string(),
                size_bytes: None,
                post: 2,
            }],
            at: None,
            photo: None,
        });
        assert_eq!(rs6.mod_version.as_deref(), Some("1.1"));
    }

    #[test]
    fn archive_pairs_only_with_a_nearby_description() {
        let messages = vec![
            text_post(100, "2026-11-01T10:00:00+00:00", "Новости канала"),
            text_post(101, "2026-11-01T12:00:00+00:00", "Lada 2107 v2.0"),
            file_post(
                106,
                "2026-11-01T12:00:01+00:00",
                "lada_2107_tg_m0dsbeamng.zip",
                Some(50_000_000),
            ),
            // Через час, без описания: выпуск по имени файла.
            file_post(
                110,
                "2026-11-01T13:00:00+00:00",
                "bmw_e34_v1.3_tg_m0dsbeamng.zip",
                None,
            ),
            text_post(120, "2026-11-01T14:00:00+00:00", "Kia Rio v1.0"),
            // Слишком далеко по номеру: это не архив описания.
            file_post(140, "2026-11-01T14:00:05+00:00", "kia_rio.zip", None),
        ];
        let (found, orphan) = releases(&messages);
        assert_eq!(orphan, None);
        let titles: Vec<&str> = found.iter().map(|release| release.title.as_str()).collect();
        assert_eq!(titles, ["Lada 2107 v2.0", "bmw e34 v1.3", "kia rio"]);
        assert_eq!(found[0].files[0].post, 106);
        assert_eq!(found[1].post, 110);
    }

    #[test]
    fn several_archives_belong_to_one_release_without_a_single_size() {
        let messages = vec![
            text_post(1, "2026-11-01T10:00:00+00:00", "Big Map v3.0"),
            file_post(
                2,
                "2026-11-01T10:00:01+00:00",
                "big_map_part1.zip",
                Some(1_000),
            ),
            file_post(
                3,
                "2026-11-01T10:00:02+00:00",
                "big_map_part2.zip",
                Some(2_000),
            ),
        ];
        let (found, _) = releases(&messages);
        assert_eq!(found.len(), 1);
        let listing = to_listing(&found[0]);
        assert_eq!(listing.offers.len(), 2);
        assert_eq!(listing.size_bytes, None);
        assert_eq!(listing.mod_version.as_deref(), Some("3.0"));
    }

    #[test]
    fn archive_with_its_own_caption_is_a_release() {
        let mut message = file_post(7, "2026-11-01T10:00:00+00:00", "x.zip", Some(10_000));
        message.text = Some("Volvo 240 v1.2\nописание".to_string());
        let (found, orphan) = releases(&[message]);
        assert_eq!(orphan, None);
        assert_eq!(found[0].title, "Volvo 240 v1.2");
    }

    #[test]
    fn post_addresses() {
        let id = |raw: &str| post_id(&Url::parse(raw).unwrap());
        assert_eq!(id("https://t.me/m0dsbeamng/18365"), Some(18365));
        assert_eq!(id("https://t.me/s/m0dsbeamng/18365"), Some(18365));
        assert_eq!(id("https://t.me/M0dsBeamNG/5?single"), Some(5));
        assert_eq!(id("https://t.me/m0dsbeamng"), None);
        assert_eq!(id("https://t.me/other/18365"), None);
        assert_eq!(id("https://t.me/m0dsbeamng/abc"), None);
        assert_eq!(id("https://t.me/m0dsbeamng/0"), None);
        assert_eq!(id("https://evil.example/m0dsbeamng/1"), None);
        assert_eq!(id("tg://resolve?domain=m0dsbeamng&post=1"), None);
        assert!(is_channel_url(
            &Url::parse("https://t.me/s/m0dsbeamng?q=audi").unwrap()
        ));
        assert!(!is_channel_url(&Url::parse("https://t.me/s/").unwrap()));
        assert_eq!(app_link(18365), "tg://resolve?domain=m0dsbeamng&post=18365");
    }

    #[test]
    fn forwarded_posts_of_other_channels_are_skipped() {
        let html = r#"<section class="tgme_channel_history"><div class="tgme_widget_message_wrap"><div class="tgme_widget_message" data-post="other/5"><div class="tgme_widget_message_text">X v1</div></div></div></section>"#;
        assert!(parse_page(html).messages.is_empty());
    }
}

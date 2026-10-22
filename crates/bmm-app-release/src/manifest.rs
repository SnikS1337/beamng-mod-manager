//! `latest.json` из выпуска: версия, откуда качать, размер, SHA-256.
//!
//! Описание пишет сборочная машина, но программа не верит ему на слово:
//! адрес только https и только GitHub, размер правдоподобный, сумма из 64
//! шестнадцатеричных знаков. Всё остальное считается повреждённым описанием.

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime};
use serde::Serialize;
use serde_json::Value;
use url::Url;

use crate::version::AppVersion;

/// Меньше мегабайта — точно не наша программа.
pub const MIN_SIZE: u64 = 1024 * 1024;
/// Защита от подмены огромным файлом.
pub const MAX_SIZE: u64 = 400 * 1024 * 1024;
/// Заметки длиннее не показываем целиком.
pub const MAX_NOTES_CHARS: usize = 500;

/// Разобранное и проверенное описание обновления.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateManifest {
    pub version: AppVersion,
    pub url: Url,
    pub size: u64,
    pub sha256: String,
    pub notes: Option<String>,
    pub published: Option<DateTime<FixedOffset>>,
}

impl UpdateManifest {
    /// Разобрать и проверить JSON. Ошибка — понятная причина, без паники.
    pub fn parse(json: &str) -> Result<Self, ManifestError> {
        parse_manifest(json)
    }

    /// Разобрать UTF-8 байты (ответ сервера или заранее прочитанный файл).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ManifestError> {
        let text = std::str::from_utf8(bytes).map_err(|_| ManifestError::Malformed)?;
        parse_manifest(text)
    }
}

/// Почему описание не принято.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("описание обновления повреждено")]
    Malformed,
    #[error("в описании обновления нет версии")]
    MissingVersion,
    #[error("адрес обновления не с GitHub или не https")]
    UntrustedUrl,
    #[error("размер обновления неправдоподобный")]
    ImplausibleSize,
    #[error("в описании обновления нет контрольной суммы")]
    MissingChecksum,
}

/// Куда можно ходить за обновлением: GitHub и его хранилище файлов, только https.
pub fn is_allowed_host(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    host == "github.com" || host.ends_with(".githubusercontent.com")
}

/// Разобрать `latest.json` со строгой проверкой полей.
pub fn parse_manifest(json: &str) -> Result<UpdateManifest, ManifestError> {
    let value: Value = serde_json::from_str(json).map_err(|_| ManifestError::Malformed)?;
    let object = value.as_object().ok_or(ManifestError::Malformed)?;

    let text = |key: &str| {
        object
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    };

    let version = text("version")
        .and_then(AppVersion::parse)
        .ok_or(ManifestError::MissingVersion)?;

    let raw_url = text("url").ok_or(ManifestError::UntrustedUrl)?;
    let url = Url::parse(raw_url).map_err(|_| ManifestError::UntrustedUrl)?;
    if !is_allowed_host(&url) {
        return Err(ManifestError::UntrustedUrl);
    }

    let size = object
        .get("size")
        .and_then(Value::as_u64)
        .ok_or(ManifestError::ImplausibleSize)?;
    if !(MIN_SIZE..=MAX_SIZE).contains(&size) {
        return Err(ManifestError::ImplausibleSize);
    }

    let sha256 = text("sha256")
        .map(str::to_ascii_lowercase)
        .ok_or(ManifestError::MissingChecksum)?;
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ManifestError::MissingChecksum);
    }

    let notes = text("notes").map(truncate_notes);
    let published = text("published").and_then(parse_published);

    Ok(UpdateManifest {
        version,
        url,
        size,
        sha256,
        notes,
        published,
    })
}

fn truncate_notes(notes: &str) -> String {
    if notes.chars().count() > MAX_NOTES_CHARS {
        notes.chars().take(MAX_NOTES_CHARS).collect()
    } else {
        notes.to_string()
    }
}

fn parse_published(text: &str) -> Option<DateTime<FixedOffset>> {
    if let Ok(value) = DateTime::parse_from_rfc3339(text) {
        return Some(value);
    }
    if let Ok(value) = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S") {
        return Some(value.and_utc().fixed_offset());
    }
    if let Ok(value) = NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S") {
        return Some(value.and_utc().fixed_offset());
    }
    if let Ok(value) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(value.and_hms_opt(0, 0, 0)?.and_utc().fixed_offset());
    }
    None
}

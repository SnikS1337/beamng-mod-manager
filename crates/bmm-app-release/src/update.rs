//! Проверка: вышла ли версия новее установленной.
//!
//! Здесь нет сети и диска: описание уже скачано (или передано байтами), остаётся
//! сравнить версии и объяснить результат плашке обновления.

use chrono::{DateTime, Utc};
use serde::Serialize;
use url::Url;

use crate::download::{CancelToken, DownloadError, UpdateDownloader};
use crate::manifest::UpdateManifest;
use crate::version::AppVersion;

/// Что происходит с обновлением программы.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStage {
    /// Ещё не проверяли или проверка выключена.
    Idle,
    Checking,
    UpToDate,
    Available,
    Downloading,
    /// Новая версия скачана и проверена: на выходе она запишется на место старой.
    ReadyToRestart,
    Failed,
}

/// Состояние для плашки и Tauri-событий. В нём всегда последний результат.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
    pub stage: UpdateStage,
    pub manifest: Option<UpdateManifest>,
    pub progress: f64,
    pub error: Option<String>,
    pub checked_at: Option<DateTime<Utc>>,
}

impl UpdateState {
    pub fn idle() -> Self {
        Self {
            stage: UpdateStage::Idle,
            manifest: None,
            progress: 0.0,
            error: None,
            checked_at: None,
        }
    }

    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            stage: UpdateStage::Failed,
            manifest: None,
            progress: 0.0,
            error: Some(error.into()),
            checked_at: Some(Utc::now()),
        }
    }
}

/// Откуда взять описание: адрес в выпуске GitHub или уже полученные байты
/// (например, из локальной ленты для проверки на сборочной машине).
#[derive(Debug, Clone, Copy)]
pub enum ManifestInput<'a> {
    Url(&'a Url),
    Json(&'a str),
    Bytes(&'a [u8]),
}

impl<'a> ManifestInput<'a> {
    pub fn url(url: &'a Url) -> Self {
        Self::Url(url)
    }

    pub fn json(json: &'a str) -> Self {
        Self::Json(json)
    }

    pub fn bytes(bytes: &'a [u8]) -> Self {
        Self::Bytes(bytes)
    }
}

/// Проверить обновление по адресу или по готовому описанию. Сетевой клиент
/// нужен только для [`ManifestInput::Url`]. Возвращает состояние, а не ошибку:
/// плашке нужен текст причины.
pub fn check(
    local_version: &AppVersion,
    input: ManifestInput<'_>,
    downloader: Option<&UpdateDownloader>,
    skipped: Option<&AppVersion>,
) -> UpdateState {
    check_with_cancel(
        local_version,
        input,
        downloader,
        skipped,
        &CancelToken::new(),
    )
}

/// Проверить обновление по адресу `latest.json` в выпуске GitHub.
pub fn check_url(
    local_version: &AppVersion,
    manifest_url: &Url,
    downloader: &UpdateDownloader,
    skipped: Option<&AppVersion>,
) -> UpdateState {
    check(
        local_version,
        ManifestInput::Url(manifest_url),
        Some(downloader),
        skipped,
    )
}

/// Проверить обновление по уже прочитанным байтам `latest.json`.
pub fn check_bytes(
    local_version: &AppVersion,
    manifest_bytes: &[u8],
    skipped: Option<&AppVersion>,
) -> UpdateState {
    check(
        local_version,
        ManifestInput::Bytes(manifest_bytes),
        None,
        skipped,
    )
}

/// Проверить обновление по строке `latest.json`.
pub fn check_json(
    local_version: &AppVersion,
    manifest_json: &str,
    skipped: Option<&AppVersion>,
) -> UpdateState {
    check(
        local_version,
        ManifestInput::Json(manifest_json),
        None,
        skipped,
    )
}

/// [`check`] с отменой сетевого запроса.
pub fn check_with_cancel(
    local_version: &AppVersion,
    input: ManifestInput<'_>,
    downloader: Option<&UpdateDownloader>,
    skipped: Option<&AppVersion>,
    cancel: &CancelToken,
) -> UpdateState {
    let now = Utc::now();
    let manifest = match input {
        ManifestInput::Url(url) => {
            let Some(downloader) = downloader else {
                return failed("не задан сетевой клиент", now);
            };
            match downloader.get_manifest(url, cancel) {
                Ok(manifest) => manifest,
                Err(DownloadError::Cancelled) => return UpdateState::idle(),
                Err(error) => return failed(error, now),
            }
        }
        ManifestInput::Json(json) => match UpdateManifest::parse(json) {
            Ok(manifest) => manifest,
            Err(error) => return failed(error, now),
        },
        ManifestInput::Bytes(bytes) => match UpdateManifest::from_bytes(bytes) {
            Ok(manifest) => manifest,
            Err(error) => return failed(error, now),
        },
    };
    evaluate_at(local_version, manifest, skipped, now)
}

/// Сравнить установленную версию с описанием (без сети).
pub fn evaluate(
    local_version: &AppVersion,
    manifest: UpdateManifest,
    skipped: Option<&AppVersion>,
) -> UpdateState {
    evaluate_at(local_version, manifest, skipped, Utc::now())
}

fn evaluate_at(
    local_version: &AppVersion,
    manifest: UpdateManifest,
    skipped: Option<&AppVersion>,
    now: DateTime<Utc>,
) -> UpdateState {
    if !manifest.version.is_newer_than(local_version) {
        return UpdateState {
            stage: UpdateStage::UpToDate,
            manifest: Some(manifest),
            progress: 0.0,
            error: None,
            checked_at: Some(now),
        };
    }
    if skipped.is_some_and(|skipped| *skipped == manifest.version) {
        let message = format!(
            "версия {} не запустилась у вас, ждём следующую",
            manifest.version.display()
        );
        return UpdateState {
            stage: UpdateStage::UpToDate,
            manifest: Some(manifest),
            error: Some(message),
            progress: 0.0,
            checked_at: Some(now),
        };
    }
    UpdateState {
        stage: UpdateStage::Available,
        manifest: Some(manifest),
        progress: 0.0,
        error: None,
        checked_at: Some(now),
    }
}

fn failed(error: impl std::fmt::Display, now: DateTime<Utc>) -> UpdateState {
    UpdateState {
        stage: UpdateStage::Failed,
        manifest: None,
        progress: 0.0,
        error: Some(error.to_string()),
        checked_at: Some(now),
    }
}

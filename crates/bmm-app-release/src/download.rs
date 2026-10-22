//! Скачивание описания и архива обновления.
//!
//! Перенаправления разбираются вручную и проверяются по одному: только GitHub
//! и только https. Архив пишется в `.part` и становится файлом только после
//! совпадения размера и SHA-256. Частичный файл при ошибке удаляется.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use reqwest::blocking::Client;
use reqwest::header::LOCATION;
use serde::Serialize;
use sha2::{Digest, Sha256};
use url::Url;

use crate::manifest::{ManifestError, UpdateManifest, is_allowed_host};
use crate::paths::with_suffix;
use crate::version::AppVersion;

/// Описание обновления больше не читаем: это точно не наш JSON.
pub const MAX_MANIFEST_BYTES: usize = 64 * 1024;
/// Сколько перенаправлений разрешено пройти.
pub const MAX_REDIRECTS: usize = 5;
/// Заголовок User-Agent по умолчанию.
pub const DEFAULT_USER_AGENT: &str = "BeamModManager/0.4 (personal use; update checks)";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const IO_TIMEOUT: Duration = Duration::from_secs(30);
const BUFFER_BYTES: usize = 80 * 1024;

/// Отмена загрузки или проверки: один флаг, который можно передать в другой поток.
#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Снова разрешить работу (повторная попытка после отмены).
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }
}

/// Ответ транспорта: заголовки и поток тела.
pub struct HttpResponse {
    pub status: u16,
    pub content_length: Option<u64>,
    pub location: Option<String>,
    pub body: Box<dyn Read + Send>,
}

/// Транспорт для тестов и настоящей сети. Реализация не следует
/// перенаправлениям: это делает [`UpdateDownloader`], проверяя каждый адрес.
pub trait HttpTransport: Send + Sync {
    fn send(&self, url: &Url) -> Result<HttpResponse, TransportError>;
}

/// Почему не удалось даже начать запрос.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TransportError {
    #[error("сервер не отвечает")]
    Timeout,
    #[error("нет связи с GitHub")]
    Network,
    #[error("{0}")]
    Io(String),
}

/// Настоящая сеть через `reqwest` (блокирующий клиент, без автоматических
/// перенаправлений, таймаут простоя 30 с).
pub struct ReqwestTransport {
    client: Client,
}

impl ReqwestTransport {
    pub fn new(user_agent: &str) -> Result<Self, DownloadError> {
        let client = Client::builder()
            .user_agent(user_agent)
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(IO_TIMEOUT)
            .build()
            .map_err(|error| DownloadError::Client(error.to_string()))?;
        Ok(Self { client })
    }
}

impl HttpTransport for ReqwestTransport {
    fn send(&self, url: &Url) -> Result<HttpResponse, TransportError> {
        let response = self
            .client
            .get(url.clone())
            .send()
            .map_err(map_reqwest_error)?;
        let status = response.status().as_u16();
        let content_length = response.content_length();
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        Ok(HttpResponse {
            status,
            content_length,
            location,
            body: Box::new(response),
        })
    }
}

fn map_reqwest_error(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else if error.is_connect() || error.is_request() {
        TransportError::Network
    } else {
        TransportError::Io(error.to_string())
    }
}

/// Ошибка проверки или скачивания. Текст сообщений совпадает с прежней
/// программой, чтобы плашка обновления не меняла формулировки.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadError {
    #[error("адрес обновления не с GitHub или не https")]
    UntrustedUrl,
    #[error("выпусков пока нет")]
    NoReleases,
    #[error("сервер ответил {0}")]
    Status(u16),
    #[error("слишком много перенаправлений")]
    TooManyRedirects,
    #[error("описание обновления слишком большое")]
    ManifestTooLarge,
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error("сервер не отвечает")]
    ServerTimeout,
    #[error("нет связи с GitHub")]
    Network,
    #[error("сервер перестал отвечать")]
    Stalled,
    #[error("размер файла на сервере не совпадает с описанием")]
    SizeMismatch,
    #[error("файл на сервере больше, чем в описании")]
    TooLarge,
    #[error("загрузка оборвалась")]
    Truncated,
    #[error("контрольная сумма не совпала: файл повреждён или подменён")]
    ChecksumMismatch,
    #[error("загрузка отменена")]
    Cancelled,
    #[error("не удалось скачать: {0}")]
    Io(String),
    #[error("не удалось создать сетевой клиент: {0}")]
    Client(String),
}

impl DownloadError {
    fn io(context: impl std::fmt::Display, error: io::Error) -> Self {
        Self::Io(format!("{context}: {error}"))
    }
}

/// Скачанный архив: где он лежит и для какой версии.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadedUpdate {
    pub path: PathBuf,
    pub version: AppVersion,
}

/// Сеть обновлений: описание и архив.
pub struct UpdateDownloader {
    transport: Arc<dyn HttpTransport>,
}

impl UpdateDownloader {
    pub fn new(transport: Arc<dyn HttpTransport>) -> Self {
        Self { transport }
    }

    /// Клиент для настоящей сети.
    pub fn reqwest(user_agent: &str) -> Result<Self, DownloadError> {
        Ok(Self::new(Arc::new(ReqwestTransport::new(user_agent)?)))
    }

    /// Скачать и разобрать `latest.json`.
    pub fn get_manifest(
        &self,
        manifest_url: &Url,
        cancel: &CancelToken,
    ) -> Result<UpdateManifest, DownloadError> {
        let response = self.send_following(manifest_url, cancel)?;
        match response.status {
            404 => return Err(DownloadError::NoReleases),
            status if !(200..300).contains(&status) => return Err(DownloadError::Status(status)),
            _ => {}
        }
        if response
            .content_length
            .is_some_and(|length| length > MAX_MANIFEST_BYTES as u64)
        {
            return Err(DownloadError::ManifestTooLarge);
        }
        let bytes = read_limited(response.body, MAX_MANIFEST_BYTES, cancel)?;
        UpdateManifest::from_bytes(&bytes).map_err(DownloadError::from)
    }

    /// Скачать архив в `target`. Промежуточные байты лежат в `target.part`.
    /// Файл `target` появляется только после совпадения размера и SHA-256.
    pub fn download(
        &self,
        manifest: &UpdateManifest,
        target: &Path,
        progress: &mut dyn FnMut(f64),
        cancel: &CancelToken,
    ) -> Result<(), DownloadError> {
        let part = with_suffix(target, ".part");
        let result = self.download_inner(manifest, target, &part, progress, cancel);
        if result.is_err() {
            let _ = fs::remove_file(&part);
        }
        result
    }

    /// Скачать архив в папку обновления: `<update_dir>/<версия>/update.zip`.
    pub fn download_into(
        &self,
        manifest: &UpdateManifest,
        update_dir: &Path,
        progress: &mut dyn FnMut(f64),
        cancel: &CancelToken,
    ) -> Result<DownloadedUpdate, DownloadError> {
        let directory = update_dir.join(manifest.version.display());
        fs::create_dir_all(&directory)
            .map_err(|error| DownloadError::io("не удалось создать папку обновления", error))?;
        let path = directory.join("update.zip");
        self.download(manifest, &path, progress, cancel)?;
        Ok(DownloadedUpdate {
            path,
            version: manifest.version,
        })
    }

    fn download_inner(
        &self,
        manifest: &UpdateManifest,
        target: &Path,
        part: &Path,
        progress: &mut dyn FnMut(f64),
        cancel: &CancelToken,
    ) -> Result<(), DownloadError> {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        if let Some(parent) = target.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|error| {
                    DownloadError::io("не удалось создать папку загрузки", error)
                })?;
            }
        }

        let response = self.send_following(&manifest.url, cancel)?;
        if !(200..300).contains(&response.status) {
            return Err(DownloadError::Status(response.status));
        }
        if let Some(length) = response.content_length {
            if length != manifest.size {
                return Err(DownloadError::SizeMismatch);
            }
        }

        let mut output = File::create(part)
            .map_err(|error| DownloadError::io("не удалось создать временный файл", error))?;
        let mut hasher = Sha256::new();
        let mut got = 0u64;
        let mut reported = -1.0f64;
        let mut body = response.body;
        let mut buffer = vec![0u8; BUFFER_BYTES];
        progress(0.0);

        loop {
            if cancel.is_cancelled() {
                return Err(DownloadError::Cancelled);
            }
            let read = body.read(&mut buffer).map_err(read_error)?;
            if read == 0 {
                break;
            }
            got = got.saturating_add(read as u64);
            if got > manifest.size {
                return Err(DownloadError::TooLarge);
            }
            hasher.update(&buffer[..read]);
            output
                .write_all(&buffer[..read])
                .map_err(|error| DownloadError::io("не удалось записать файл", error))?;

            let fraction = got as f64 / manifest.size as f64;
            if fraction - reported >= 0.01 || fraction >= 1.0 {
                reported = fraction;
                progress(fraction.min(1.0));
            }
        }

        output
            .flush()
            .and_then(|_| output.sync_all())
            .map_err(|error| DownloadError::io("не удалось сохранить файл", error))?;
        drop(output);

        if got != manifest.size {
            return Err(DownloadError::Truncated);
        }
        let digest = hex_lower(&hasher.finalize());
        if digest != manifest.sha256 {
            return Err(DownloadError::ChecksumMismatch);
        }

        fs::rename(part, target)
            .map_err(|error| DownloadError::io("не удалось переименовать файл", error))?;
        if reported < 1.0 {
            progress(1.0);
        }
        Ok(())
    }

    fn send_following(
        &self,
        url: &Url,
        cancel: &CancelToken,
    ) -> Result<HttpResponse, DownloadError> {
        let mut url = url.clone();
        for redirects in 0..=MAX_REDIRECTS {
            if cancel.is_cancelled() {
                return Err(DownloadError::Cancelled);
            }
            if !is_allowed_host(&url) {
                return Err(DownloadError::UntrustedUrl);
            }
            let response = self
                .transport
                .send(&url)
                .map_err(download_transport_error)?;
            if !(300..400).contains(&response.status) {
                return Ok(response);
            }
            let Some(location) = response.location else {
                return Err(DownloadError::TooManyRedirects);
            };
            if redirects == MAX_REDIRECTS {
                return Err(DownloadError::TooManyRedirects);
            }
            url = url
                .join(&location)
                .map_err(|_| DownloadError::UntrustedUrl)?;
        }
        Err(DownloadError::TooManyRedirects)
    }
}

fn read_limited(
    mut body: Box<dyn Read + Send>,
    limit: usize,
    cancel: &CancelToken,
) -> Result<Vec<u8>, DownloadError> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        if cancel.is_cancelled() {
            return Err(DownloadError::Cancelled);
        }
        let read = body.read(&mut buffer).map_err(read_error)?;
        if read == 0 {
            return Ok(bytes);
        }
        if bytes.len().saturating_add(read) > limit {
            return Err(DownloadError::ManifestTooLarge);
        }
        bytes.extend_from_slice(&buffer[..read]);
    }
}

fn download_transport_error(error: TransportError) -> DownloadError {
    match error {
        TransportError::Timeout => DownloadError::ServerTimeout,
        TransportError::Network => DownloadError::Network,
        TransportError::Io(message) => DownloadError::Io(message),
    }
}

fn read_error(error: io::Error) -> DownloadError {
    match error.kind() {
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => DownloadError::Stalled,
        io::ErrorKind::UnexpectedEof => DownloadError::Truncated,
        _ => DownloadError::Io(error.to_string()),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

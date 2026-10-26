//! Кэш картинок сайтов: скачивает превью в папку данных и отдаёт ключ для localhost-протокола.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sha2::{Digest, Sha256};
use url::Url;

use bmm_domain::atomic;

use crate::net::{PageFetcher, USER_AGENT, safe_redirect_policy};

const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
/// Картинки поиска копятся с каждым запросом; больше этого — старые уходят.
pub const DEFAULT_LIMIT_BYTES: u64 = 300 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteImage {
    pub key: String,
    pub extension: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error("картинка не скачалась")]
    Network,
    #[error("картинка слишком большая")]
    TooLarge,
    #[error("сайт не отдал картинку")]
    NotImage,
    #[error("{0}")]
    Io(String),
}

pub struct RemoteImageCache {
    root: PathBuf,
    client: Option<reqwest::blocking::Client>,
    fallback: Option<Arc<dyn PageFetcher>>,
}

impl RemoteImageCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::build(root.into(), None)
    }

    /// Сайт не отдаёт картинки обычным запросом — пробуем тем же путём, что и страницы.
    pub fn with_fallback(root: impl Into<PathBuf>, fallback: Arc<dyn PageFetcher>) -> Self {
        Self::build(root.into(), Some(fallback))
    }

    fn build(root: PathBuf, fallback: Option<Arc<dyn PageFetcher>>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .redirect(safe_redirect_policy())
            .build()
            .ok();
        Self {
            root,
            client,
            fallback,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Оставляет в папке не больше `max_bytes`: первыми уходят давно не нужные
    /// (время изменения обновляется при каждом показе картинки). Возвращает, сколько удалено.
    pub fn trim(&self, max_bytes: u64) -> usize {
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return 0;
        };
        let mut files: Vec<(std::time::SystemTime, u64, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let metadata = entry.metadata().ok()?;
                metadata.is_file().then(|| {
                    (
                        metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                        metadata.len(),
                        entry.path(),
                    )
                })
            })
            .collect();
        let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
        if total <= max_bytes {
            return 0;
        }
        files.sort_by_key(|(modified, _, _)| *modified);
        let mut removed = 0;
        for (_, size, path) in files {
            if total <= max_bytes {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total = total.saturating_sub(size);
                removed += 1;
            }
        }
        removed
    }

    pub fn cached(&self, url: &Url) -> Option<RemoteImage> {
        let key = key_of(url);
        let mut found: Vec<(std::time::SystemTime, &'static str, PathBuf)> = Vec::new();
        for extension in ["jpg", "jpeg", "png", "webp"] {
            let path = self.root.join(format!("{key}.{extension}"));
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            if metadata.is_file() {
                found.push((
                    metadata.modified().unwrap_or(std::time::UNIX_EPOCH),
                    extension,
                    path,
                ));
            }
        }
        // Старые версии могли сохранить один и тот же адрес в другом формате. Берём
        // самый свежий файл, иначе картинка навсегда остаётся прежней.
        found.sort_by_key(|(modified, _, _)| *modified);
        let (_, extension, path) = found.pop()?;
        let _ = std::fs::File::options()
            .append(true)
            .open(&path)
            .and_then(|file| file.set_modified(std::time::SystemTime::now()));
        for (_, _, older) in found {
            let _ = std::fs::remove_file(older);
        }
        Some(RemoteImage {
            key,
            extension: extension.to_string(),
        })
    }

    pub fn fetch(&self, url: &Url) -> Result<Option<RemoteImage>, ImageError> {
        if let Some(cached) = self.cached(url) {
            return Ok(Some(cached));
        }
        match self.download(url) {
            Ok(Some((content_type, bytes))) => self.store(url, &content_type, bytes),
            Ok(None) | Err(ImageError::Network) | Err(ImageError::NotImage) => {
                match self.fallback_bytes(url) {
                    Some((content_type, bytes)) => self.store(url, &content_type, bytes),
                    None => Ok(None),
                }
            }
            Err(error) => Err(error),
        }
    }

    fn download(&self, url: &Url) -> Result<Option<(String, Vec<u8>)>, ImageError> {
        let client = self.client.as_ref().ok_or(ImageError::Network)?;
        let response = client
            .get(url.clone())
            .send()
            .map_err(|_| ImageError::Network)?;
        if !response.status().is_success() {
            return Ok(None);
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_lowercase();
        if extension_of(&content_type, url).is_none() {
            return Err(ImageError::NotImage);
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_IMAGE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| ImageError::Network)?;
        if bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err(ImageError::TooLarge);
        }
        if bytes.is_empty() {
            return Ok(None);
        }
        Ok(Some((content_type, bytes)))
    }

    fn fallback_bytes(&self, url: &Url) -> Option<(String, Vec<u8>)> {
        let (content_type, bytes) = self.fallback.as_ref()?.get_bytes(url).ok()??;
        if bytes.is_empty() || bytes.len() as u64 > MAX_IMAGE_BYTES {
            return None;
        }
        Some((content_type, bytes))
    }

    fn store(
        &self,
        url: &Url,
        content_type: &str,
        bytes: Vec<u8>,
    ) -> Result<Option<RemoteImage>, ImageError> {
        if bytes.is_empty() {
            return Ok(None);
        }
        if bytes.len() as u64 > MAX_IMAGE_BYTES {
            return Err(ImageError::TooLarge);
        }
        let Some(extension) = extension_of(content_type, url) else {
            return Err(ImageError::NotImage);
        };
        let key = key_of(url);
        std::fs::create_dir_all(&self.root).map_err(|error| ImageError::Io(error.to_string()))?;
        atomic::write_atomic(&self.root.join(format!("{key}.{extension}")), &bytes)
            .map_err(|error| ImageError::Io(error.to_string()))?;
        // Сайт мог отдать тот же адрес уже другим форматом. Старый файл иначе
        // находится первым и картинка навсегда остаётся прежней.
        for other in ["jpg", "jpeg", "png", "webp"] {
            if other != extension {
                let _ = std::fs::remove_file(self.root.join(format!("{key}.{other}")));
            }
        }
        Ok(Some(RemoteImage { key, extension }))
    }

    pub fn path(&self, image: &RemoteImage) -> PathBuf {
        self.root.join(format!("{}.{}", image.key, image.extension))
    }
}

fn key_of(url: &Url) -> String {
    let hash = Sha256::digest(url.as_str().as_bytes());
    hash[..12]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn extension_of(content_type: &str, url: &Url) -> Option<String> {
    let content_type = content_type.to_lowercase();
    if content_type.starts_with("image/jpeg") || content_type.starts_with("image/jpg") {
        return Some("jpg".to_string());
    }
    if content_type.starts_with("image/png") {
        return Some("png".to_string());
    }
    if content_type.starts_with("image/webp") {
        return Some("webp".to_string());
    }
    if !content_type.is_empty() {
        // gif, svg, avif и прочее не выдаём за jpg/png по расширению адреса.
        return None;
    }
    url.path()
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_lowercase())
        .filter(|extension| matches!(extension.as_str(), "jpg" | "jpeg" | "png" | "webp"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::{FetchError, RawResponse};
    use std::io::Write;
    use std::net::TcpListener;

    fn serve(response: &'static [u8]) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buffer = [0u8; 1024];
                let _ = stream.read(&mut buffer);
                let _ = stream.write_all(response);
            }
        });
        (format!("http://{address}/preview.jpg"), thread)
    }

    fn response(body: &'static [u8], content_type: &str) -> &'static [u8] {
        response_status("200 OK", body, content_type)
    }

    fn response_status(status: &str, body: &'static [u8], content_type: &str) -> &'static [u8] {
        let mut bytes = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        bytes.extend_from_slice(body);
        Box::leak(bytes.into_boxed_slice())
    }

    struct BrowserBytes(Option<(String, Vec<u8>)>);

    impl PageFetcher for BrowserBytes {
        fn get(&self, _url: &Url) -> Result<RawResponse, FetchError> {
            Err(FetchError::Unsupported)
        }

        fn get_bytes(&self, _url: &Url) -> Result<Option<(String, Vec<u8>)>, FetchError> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn trim_drops_least_recently_used_first() {
        let temp = tempfile::tempdir().unwrap();
        let cache = RemoteImageCache::new(temp.path());
        let old = std::time::SystemTime::now() - Duration::from_secs(3600);
        for (index, name) in ["a.jpg", "b.jpg", "c.jpg"].iter().enumerate() {
            let path = temp.path().join(name);
            std::fs::write(&path, vec![0u8; 100]).unwrap();
            let file = std::fs::File::options().append(true).open(&path).unwrap();
            file.set_modified(old + Duration::from_secs(index as u64 * 60))
                .unwrap();
        }
        assert_eq!(cache.trim(1000), 0, "под пределом ничего не трогаем");
        assert_eq!(cache.trim(200), 1);
        assert!(!temp.path().join("a.jpg").exists(), "ушла самая старая");
        assert!(temp.path().join("c.jpg").exists());
        assert_eq!(RemoteImageCache::new(temp.path().join("нет")).trim(0), 0);
    }

    #[test]
    fn caches_image_and_reuses_it() {
        let (url, server) = serve(response(b"\x89PNG\r\n\x1a\nx", "image/png"));
        let temp = tempfile::tempdir().unwrap();
        let cache = RemoteImageCache::new(temp.path());
        let url = Url::parse(&url).unwrap();
        let first = cache.fetch(&url).unwrap().unwrap();
        assert_eq!(first.extension, "png");
        assert!(cache.path(&first).is_file());
        server.join().unwrap();
        let second = cache.fetch(&url).unwrap().unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn browser_fallback_saves_image_when_site_forbids_direct_request() {
        let (url, server) = serve(response_status("403 Forbidden", b"forbidden", "text/html"));
        let temp = tempfile::tempdir().unwrap();
        let fallback = BrowserBytes(Some((
            "image/png".to_string(),
            b"\x89PNG\r\n\x1a\nx".to_vec(),
        )));
        let cache = RemoteImageCache::with_fallback(temp.path(), Arc::new(fallback));
        let url = Url::parse(&url).unwrap();
        let image = cache.fetch(&url).unwrap().unwrap();
        assert_eq!(image.extension, "png");
        assert!(cache.path(&image).is_file());
        server.join().unwrap();
    }

    #[test]
    fn browser_fallback_without_bytes_leaves_placeholder() {
        let (url, server) = serve(response_status("403 Forbidden", b"no", "text/html"));
        let temp = tempfile::tempdir().unwrap();
        let cache = RemoteImageCache::with_fallback(temp.path(), Arc::new(BrowserBytes(None)));
        assert!(cache.fetch(&Url::parse(&url).unwrap()).unwrap().is_none());
        server.join().unwrap();
    }

    #[test]
    fn rejects_non_image() {
        let (url, server) = serve(response(b"hello", "text/html"));
        let temp = tempfile::tempdir().unwrap();
        let cache = RemoteImageCache::new(temp.path());
        let result = cache.fetch(&Url::parse(&url).unwrap());
        assert!(matches!(result, Ok(None)));
        server.join().unwrap();
    }

    #[test]
    fn extension_falls_back_to_url() {
        let url = Url::parse("https://example.org/a/b.webp").unwrap();
        assert_eq!(extension_of("", &url).as_deref(), Some("webp"));
        assert_eq!(extension_of("image/jpeg", &url).as_deref(), Some("jpg"));
        assert_eq!(extension_of("text/html", &url), None);
        assert_eq!(extension_of("image/svg+xml", &url), None);
        assert_eq!(
            extension_of("application/octet-stream", &url),
            None,
            "неверный тип не подменяем расширением"
        );
    }

    #[test]
    fn cached_prefers_the_newest_file_and_drops_old_formats() {
        let temp = tempfile::tempdir().unwrap();
        let cache = RemoteImageCache::new(temp.path());
        let url = Url::parse("https://example.org/changed.jpg").unwrap();
        let key = key_of(&url);
        let old = temp.path().join(format!("{key}.jpg"));
        let new = temp.path().join(format!("{key}.png"));
        std::fs::write(&old, b"old").unwrap();
        std::fs::write(&new, b"new").unwrap();
        let past = std::time::SystemTime::now() - Duration::from_secs(60);
        std::fs::File::options()
            .append(true)
            .open(&old)
            .unwrap()
            .set_modified(past)
            .unwrap();

        let image = cache.cached(&url).unwrap();
        assert_eq!(image.extension, "png");
        assert!(new.is_file());
        assert!(!old.exists(), "старый формат убран");
    }

    #[test]
    fn new_image_type_replaces_the_old_file() {
        let temp = tempfile::tempdir().unwrap();
        let cache = RemoteImageCache::new(temp.path());
        let url = Url::parse("https://example.org/changed.jpg").unwrap();
        let key = key_of(&url);

        // Раньше сайт отдавал jpeg, теперь — png по тому же адресу.
        std::fs::write(temp.path().join(format!("{key}.jpg")), b"old").unwrap();
        let image = cache
            .store(&url, "image/png", b"\x89PNG\r\n\x1a\nnew".to_vec())
            .unwrap()
            .unwrap();
        assert_eq!(image.extension, "png");
        assert!(!temp.path().join(format!("{key}.jpg")).exists());
        assert_eq!(
            cache.cached(&url).map(|image| image.extension),
            Some("png".to_string())
        );
    }
}

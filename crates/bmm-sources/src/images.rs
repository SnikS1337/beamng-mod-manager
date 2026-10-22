//! Кэш картинок сайтов: скачивает превью в папку данных и отдаёт ключ для localhost-протокола.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};
use url::Url;

use bmm_domain::atomic;

const MAX_IMAGE_BYTES: u64 = 8 * 1024 * 1024;
const USER_AGENT: &str = "BeamModManager/0.4 (personal use; update checks)";

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
}

impl RemoteImageCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(10))
            .build()
            .ok();
        Self {
            root: root.into(),
            client,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn cached(&self, url: &Url) -> Option<RemoteImage> {
        let key = key_of(url);
        for extension in ["jpg", "jpeg", "png", "webp"] {
            if self.root.join(format!("{key}.{extension}")).is_file() {
                return Some(RemoteImage {
                    key,
                    extension: extension.to_string(),
                });
            }
        }
        None
    }

    pub fn fetch(&self, url: &Url) -> Result<Option<RemoteImage>, ImageError> {
        if let Some(cached) = self.cached(url) {
            return Ok(Some(cached));
        }
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
        let Some(extension) = extension_of(&content_type, url) else {
            return Err(ImageError::NotImage);
        };
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
        let key = key_of(url);
        std::fs::create_dir_all(&self.root).map_err(|error| ImageError::Io(error.to_string()))?;
        atomic::write_atomic(&self.root.join(format!("{key}.{extension}")), &bytes)
            .map_err(|error| ImageError::Io(error.to_string()))?;
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
    if !content_type.starts_with("image/") && !content_type.is_empty() {
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
        let mut bytes = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        bytes.extend_from_slice(body);
        Box::leak(bytes.into_boxed_slice())
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
    fn rejects_non_image() {
        let (url, server) = serve(response(b"hello", "text/html"));
        let temp = tempfile::tempdir().unwrap();
        let cache = RemoteImageCache::new(temp.path());
        let result = cache.fetch(&Url::parse(&url).unwrap());
        assert!(matches!(result, Err(ImageError::NotImage)));
        server.join().unwrap();
    }

    #[test]
    fn extension_falls_back_to_url() {
        let url = Url::parse("https://example.org/a/b.webp").unwrap();
        assert_eq!(extension_of("", &url).as_deref(), Some("webp"));
        assert_eq!(extension_of("image/jpeg", &url).as_deref(), Some("jpg"));
        assert_eq!(extension_of("text/html", &url), None);
    }
}

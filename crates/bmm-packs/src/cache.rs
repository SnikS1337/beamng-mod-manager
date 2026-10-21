use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use bmm_domain::atomic;

const TITLES_MAX_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CachedPreview {
    Image { path: PathBuf, extension: String },
    NoImage,
}

pub struct PreviewCache {
    root: PathBuf,
}

impl PreviewCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn key(file_name: &str, size_bytes: i64, modified_utc: Option<DateTime<Utc>>) -> String {
        let text = format!(
            "{}|{}|{}",
            file_name.to_lowercase(),
            size_bytes,
            modified_utc
                .map(|date| date.timestamp_millis())
                .unwrap_or(0)
        );
        let hash = Sha256::digest(text.as_bytes());
        hash[..12]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    pub fn get(&self, key: &str) -> Option<CachedPreview> {
        if self.root.join(format!("{key}.none")).is_file() {
            return Some(CachedPreview::NoImage);
        }
        for extension in ["jpg", "jpeg", "png"] {
            let path = self.root.join(format!("{key}.{extension}"));
            if path.is_file() {
                return Some(CachedPreview::Image {
                    path,
                    extension: extension.to_string(),
                });
            }
        }
        None
    }

    pub fn put_image(&self, key: &str, extension: &str, bytes: &[u8]) -> std::io::Result<PathBuf> {
        fs::create_dir_all(&self.root)?;
        let extension = extension.trim_start_matches('.').to_lowercase();
        let path = self.root.join(format!("{key}.{extension}"));
        atomic::write_atomic(&path, bytes)?;
        let _ = fs::remove_file(self.root.join(format!("{key}.none")));
        for other in ["jpg", "jpeg", "png"] {
            if other != extension {
                let _ = fs::remove_file(self.root.join(format!("{key}.{other}")));
            }
        }
        Ok(path)
    }

    pub fn put_no_image(&self, key: &str, file_name: &str) -> std::io::Result<()> {
        fs::create_dir_all(&self.root)?;
        atomic::write_atomic(&self.root.join(format!("{key}.none")), file_name.as_bytes())?;
        for extension in ["jpg", "jpeg", "png"] {
            let _ = fs::remove_file(self.root.join(format!("{key}.{extension}")));
        }
        Ok(())
    }

    pub fn prune(&self, live_keys: &HashSet<String>) {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let Some(name) = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            if name.ends_with(".tmp") {
                if let Ok(metadata) = entry.metadata()
                    && let Ok(modified) = metadata.modified()
                    && SystemTime::now()
                        .duration_since(modified)
                        .map(|age| age > Duration::from_secs(5 * 60))
                        .unwrap_or(false)
                {
                    let _ = fs::remove_file(path);
                }
                continue;
            }
            let Some((key, extension)) = name.rsplit_once('.') else {
                continue;
            };
            if matches!(extension, "jpg" | "jpeg" | "png" | "none") && !live_keys.contains(key) {
                let _ = fs::remove_file(path);
            }
        }
    }

    pub fn titles(&self) -> BTreeMap<String, String> {
        let path = self.root.join("titles.json");
        let Ok(metadata) = fs::metadata(&path) else {
            return BTreeMap::new();
        };
        if metadata.len() > TITLES_MAX_BYTES {
            return BTreeMap::new();
        }
        let Ok(text) = fs::read_to_string(path) else {
            return BTreeMap::new();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    pub fn title(&self, key: &str) -> Option<Option<String>> {
        let titles = self.titles();
        titles.get(key).map(|title| {
            if title.is_empty() {
                None
            } else {
                Some(title.clone())
            }
        })
    }

    pub fn put_title(&self, key: &str, title: Option<&str>) -> std::io::Result<()> {
        fs::create_dir_all(&self.root)?;
        let mut titles = self.titles();
        let value = title.unwrap_or("");
        if value.chars().count() > 200 {
            return Ok(());
        }
        titles.insert(key.to_string(), value.to_string());
        let json = serde_json::to_string(&titles).map_err(std::io::Error::other)?;
        atomic::write_atomic(&self.root.join("titles.json"), json.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_ignores_case_but_not_size_or_time() {
        let time = DateTime::<Utc>::from_timestamp(1_700_000_000, 0);
        assert_eq!(
            PreviewCache::key("Car.zip", 10, time),
            PreviewCache::key("car.ZIP", 10, time)
        );
        assert_ne!(
            PreviewCache::key("car.zip", 10, time),
            PreviewCache::key("car.zip", 11, time)
        );
    }

    #[test]
    fn image_and_none_replace_each_other() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(dir.path());
        let key = "abc";
        cache.put_image(key, "jpg", b"jpeg").unwrap();
        assert!(matches!(cache.get(key), Some(CachedPreview::Image { .. })));
        cache.put_no_image(key, "car.zip").unwrap();
        assert_eq!(cache.get(key), Some(CachedPreview::NoImage));
        cache.put_image(key, "png", b"png").unwrap();
        assert!(matches!(cache.get(key), Some(CachedPreview::Image { .. })));
        assert!(!dir.path().join("abc.none").exists());
    }

    #[test]
    fn prune_removes_only_orphans() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(dir.path());
        cache.put_image("live", "jpg", b"x").unwrap();
        cache.put_image("dead", "jpg", b"x").unwrap();
        fs::write(dir.path().join("readme.txt"), b"keep").unwrap();
        cache.prune(&HashSet::from(["live".to_string()]));
        assert!(dir.path().join("live.jpg").exists());
        assert!(!dir.path().join("dead.jpg").exists());
        assert!(dir.path().join("readme.txt").exists());
    }

    #[test]
    fn titles_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(dir.path());
        cache.put_title("a", Some("Toyota")).unwrap();
        cache.put_title("b", None).unwrap();
        assert_eq!(cache.title("a"), Some(Some("Toyota".to_string())));
        assert_eq!(cache.title("b"), Some(None));
        assert_eq!(cache.title("c"), None);
    }
}

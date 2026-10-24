use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use bmm_domain::atomic;

const TITLES_MAX_BYTES: u64 = 4 * 1024 * 1024;
const CONTENTS_MAX_BYTES: u64 = 32 * 1024 * 1024;
/// Мод-сборник может нести сотни папок машин; для сравнения хватит первых.
const CONTENT_KEYS_PER_MOD: usize = 500;

/// Картинки модов бывают 4K и по несколько мегабайт. Карточке и панели мода хватает
/// 720 точек по большей стороне: декодирование при прокрутке становится в разы дешевле.
const PREVIEW_MAX_SIDE: u32 = 720;
/// Файл в кэше больше этого — старая полноразмерная картинка, её уменьшаем при чтении.
const PREVIEW_STALE_BYTES: u64 = 350 * 1024;

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
            let Ok(meta) = fs::metadata(&path) else {
                continue;
            };
            if !meta.is_file() {
                continue;
            }
            if meta.len() > PREVIEW_STALE_BYTES
                && let Some(small) = fs::read(&path).ok().and_then(|bytes| shrink(&bytes))
                && let Ok((path, extension)) = self.store(key, "jpg", &small)
            {
                return Some(CachedPreview::Image { path, extension });
            }
            return Some(CachedPreview::Image {
                path,
                extension: extension.to_string(),
            });
        }
        None
    }

    /// Сохраняет картинку мода, уменьшив её до размера карточки. Возвращает путь и расширение
    /// того, что легло в кэш (большая картинка становится jpg).
    pub fn put_image(
        &self,
        key: &str,
        extension: &str,
        bytes: &[u8],
    ) -> std::io::Result<(PathBuf, String)> {
        match shrink(bytes) {
            Some(small) => self.store(key, "jpg", &small),
            None => self.store(key, extension, bytes),
        }
    }

    fn store(
        &self,
        key: &str,
        extension: &str,
        bytes: &[u8],
    ) -> std::io::Result<(PathBuf, String)> {
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
        Ok((path, extension))
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

    /// Что не так с упаковкой архива (`PackagingProblem::code`), по ключу превью;
    /// пустая строка — проверено, всё в порядке.
    pub fn problems(&self) -> BTreeMap<String, String> {
        let path = self.root.join("problems.json");
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() <= TITLES_MAX_BYTES => {}
            _ => return BTreeMap::new(),
        }
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn put_problems(&self, entries: &[(String, Option<String>)]) -> std::io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.root)?;
        let mut problems = self.problems();
        for (key, problem) in entries {
            problems.insert(key.clone(), problem.clone().unwrap_or_default());
        }
        let json = serde_json::to_string(&problems).map_err(std::io::Error::other)?;
        atomic::write_atomic(&self.root.join("problems.json"), json.as_bytes())
    }

    /// Состав архивов (`incoming::content_keys`) по ключу превью — для поиска повторов
    /// без повторного чтения сотен архивов при каждом показе списка.
    pub fn contents(&self) -> BTreeMap<String, Vec<String>> {
        let path = self.root.join("contents.json");
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() <= CONTENTS_MAX_BYTES => {}
            _ => return BTreeMap::new(),
        }
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn put_contents(&self, entries: &[(String, Vec<String>)]) -> std::io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.root)?;
        let mut contents = self.contents();
        for (key, keys) in entries {
            let mut keys = keys.clone();
            keys.sort();
            keys.truncate(CONTENT_KEYS_PER_MOD);
            contents.insert(key.clone(), keys);
        }
        let json = serde_json::to_string(&contents).map_err(std::io::Error::other)?;
        atomic::write_atomic(&self.root.join("contents.json"), json.as_bytes())
    }

    /// `mod_info` из архивов по ключу превью: `{"id": …, "data": {…}}`, `null` — нет.
    pub fn mod_infos(&self) -> BTreeMap<String, serde_json::Value> {
        let path = self.root.join("mod_info.json");
        match fs::metadata(&path) {
            Ok(metadata) if metadata.len() <= CONTENTS_MAX_BYTES => {}
            _ => return BTreeMap::new(),
        }
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn put_mod_infos(&self, entries: &[(String, serde_json::Value)]) -> std::io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.root)?;
        let mut infos = self.mod_infos();
        for (key, value) in entries {
            infos.insert(key.clone(), value.clone());
        }
        let json = serde_json::to_string(&infos).map_err(std::io::Error::other)?;
        atomic::write_atomic(&self.root.join("mod_info.json"), json.as_bytes())
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
        let titles: BTreeMap<String, String> = serde_json::from_str(&text).unwrap_or_default();
        // Записанное раньше могло сохранить лишние пробелы авторов («Toyota  Mark II»).
        titles
            .into_iter()
            .map(|(key, title)| (key, title.split_whitespace().collect::<Vec<_>>().join(" ")))
            .collect()
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

    /// Несколько названий одной записью файла: первый проход по сотням модов.
    pub fn put_titles(&self, entries: &[(String, Option<String>)]) -> std::io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        fs::create_dir_all(&self.root)?;
        let mut titles = self.titles();
        for (key, title) in entries {
            let value = title.as_deref().unwrap_or("");
            if value.chars().count() <= 200 {
                titles.insert(key.clone(), value.to_string());
            }
        }
        let json = serde_json::to_string(&titles).map_err(std::io::Error::other)?;
        atomic::write_atomic(&self.root.join("titles.json"), json.as_bytes())
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

/// Уменьшает картинку до [`PREVIEW_MAX_SIDE`] и пережимает в jpg. `None` — картинка и так
/// маленькая или не читается (тогда кладём как есть).
fn shrink(bytes: &[u8]) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let big = image.width() > PREVIEW_MAX_SIDE || image.height() > PREVIEW_MAX_SIDE;
    if !big && bytes.len() as u64 <= PREVIEW_STALE_BYTES {
        return None;
    }
    let image = if big {
        image.resize(
            PREVIEW_MAX_SIDE,
            PREVIEW_MAX_SIDE,
            image::imageops::FilterType::Triangle,
        )
    } else {
        image
    };
    let rgb = image.to_rgb8();
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 84)
        .encode_image(&rgb)
        .ok()?;
    Some(out)
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
    fn big_pictures_become_small_jpegs() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(dir.path());
        let mut png = Vec::new();
        image::RgbImage::from_pixel(1920, 1080, image::Rgb([200, 40, 40]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let (path, extension) = cache.put_image("big", "png", &png).unwrap();
        assert_eq!(extension, "jpg");
        let small = image::open(&path).unwrap();
        assert_eq!((small.width(), small.height()), (720, 405));
        assert!(!dir.path().join("big.png").exists());
    }

    #[test]
    fn small_pictures_stay_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(dir.path());
        let mut png = Vec::new();
        image::RgbImage::from_pixel(300, 200, image::Rgb([1, 2, 3]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        let (_, extension) = cache.put_image("small", "png", &png).unwrap();
        assert_eq!(extension, "png");
        assert_eq!(fs::read(dir.path().join("small.png")).unwrap(), png);
    }

    #[test]
    fn old_full_size_picture_is_shrunk_on_read() {
        let dir = tempfile::tempdir().unwrap();
        let mut jpg = Vec::new();
        let noisy = image::RgbImage::from_fn(2400, 1600, |x, y| {
            image::Rgb([
                (x * 7 % 251) as u8,
                (y * 13 % 241) as u8,
                ((x ^ y) % 239) as u8,
            ])
        });
        noisy
            .write_to(
                &mut std::io::Cursor::new(&mut jpg),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        assert!(jpg.len() as u64 > PREVIEW_STALE_BYTES);
        fs::write(dir.path().join("old.jpg"), &jpg).unwrap();
        let cache = PreviewCache::new(dir.path());
        let Some(CachedPreview::Image { path, .. }) = cache.get("old") else {
            panic!("картинка пропала");
        };
        assert!(fs::metadata(&path).unwrap().len() <= PREVIEW_STALE_BYTES);
        assert_eq!(image::open(&path).unwrap().width(), 720);
    }

    #[test]
    fn titles_are_written_in_one_batch() {
        let temp = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(temp.path());
        cache
            .put_titles(&[
                ("a".to_string(), Some("Dacia Sandero".to_string())),
                ("b".to_string(), None),
            ])
            .unwrap();
        assert_eq!(cache.title("a"), Some(Some("Dacia Sandero".to_string())));
        assert_eq!(cache.title("b"), Some(None), "известно, что названия нет");
        assert_eq!(cache.title("c"), None, "ещё не читали");
    }

    #[test]
    fn contents_round_trip_sorted() {
        let temp = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(temp.path());
        assert!(cache.contents().is_empty());
        cache
            .put_contents(&[
                (
                    "a".to_string(),
                    vec!["vehicles/b".to_string(), "lua/ge".to_string()],
                ),
                ("b".to_string(), Vec::new()),
            ])
            .unwrap();
        let contents = cache.contents();
        assert_eq!(contents["a"], vec!["lua/ge", "vehicles/b"]);
        assert!(contents["b"].is_empty(), "проверено, состава нет");
    }

    #[test]
    fn stored_titles_lose_extra_spaces() {
        let temp = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(temp.path());
        fs::create_dir_all(temp.path()).unwrap();
        fs::write(
            temp.path().join("titles.json"),
            r#"{"a":"Toyota  Mark II (X90)","b":"LADA 2104/2105/2107  ","c":""}"#,
        )
        .unwrap();
        let titles = cache.titles();
        assert_eq!(titles["a"], "Toyota Mark II (X90)");
        assert_eq!(titles["b"], "LADA 2104/2105/2107");
        assert_eq!(cache.title("c"), Some(None), "пустое — «названия нет»");
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

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

use bmm_domain::atomic;

const TITLES_FILE: &str = "titles.json";
const PROBLEMS_FILE: &str = "problems.json";
const CONTENTS_FILE: &str = "contents.json";
const MOD_INFO_FILE: &str = "mod_info.json";
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

/// Кэш картинок и сведений об архивах модов.
///
/// Сведения (названия, состав, ошибки упаковки, mod_info) — по одному json на вид.
/// Их читают на каждом показе списка, поэтому разобранная копия держится в памяти и
/// перечитывается, только если файл изменился; запись идёт под одной блокировкой,
/// чтобы фоновое заполнение и показ списка не затирали друг друга.
pub struct PreviewCache {
    root: PathBuf,
    titles: MapFile<String>,
    problems: MapFile<String>,
    contents: MapFile<Vec<String>>,
    mod_infos: MapFile<serde_json::Value>,
    write: Mutex<()>,
}

impl PreviewCache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            titles: MapFile::new(Some(clean_title)),
            problems: MapFile::new(None),
            contents: MapFile::new(None),
            mod_infos: MapFile::new(None),
            write: Mutex::new(()),
        }
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
        self.prune_maps(live_keys);
    }

    /// Что не так с упаковкой архива (`PackagingProblem::code`), по ключу превью;
    /// пустая строка — проверено, всё в порядке.
    pub fn problems(&self) -> BTreeMap<String, String> {
        self.problems
            .load(&self.root.join(PROBLEMS_FILE), TITLES_MAX_BYTES)
    }

    pub fn put_problems(&self, entries: &[(String, Option<String>)]) -> std::io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let _write = self.write_gate();
        let mut problems = self.problems();
        for (key, problem) in entries {
            problems.insert(key.clone(), problem.clone().unwrap_or_default());
        }
        self.problems.save(&self.root, PROBLEMS_FILE, problems)
    }

    /// Состав архивов (`incoming::content_keys`) по ключу превью — для поиска повторов
    /// без повторного чтения сотен архивов при каждом показе списка.
    pub fn contents(&self) -> BTreeMap<String, Vec<String>> {
        self.contents
            .load(&self.root.join(CONTENTS_FILE), CONTENTS_MAX_BYTES)
    }

    pub fn put_contents(&self, entries: &[(String, Vec<String>)]) -> std::io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let _write = self.write_gate();
        let mut contents = self.contents();
        for (key, keys) in entries {
            let mut keys = keys.clone();
            keys.sort();
            keys.truncate(CONTENT_KEYS_PER_MOD);
            contents.insert(key.clone(), keys);
        }
        self.contents.save(&self.root, CONTENTS_FILE, contents)
    }

    /// `mod_info` из архивов по ключу превью: `{"id": …, "data": {…}}`, `null` — нет.
    pub fn mod_infos(&self) -> BTreeMap<String, serde_json::Value> {
        self.mod_infos
            .load(&self.root.join(MOD_INFO_FILE), CONTENTS_MAX_BYTES)
    }

    pub fn put_mod_infos(&self, entries: &[(String, serde_json::Value)]) -> std::io::Result<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let _write = self.write_gate();
        let mut infos = self.mod_infos();
        for (key, value) in entries {
            infos.insert(key.clone(), value.clone());
        }
        self.mod_infos.save(&self.root, MOD_INFO_FILE, infos)
    }

    pub fn titles(&self) -> BTreeMap<String, String> {
        self.titles
            .load(&self.root.join(TITLES_FILE), TITLES_MAX_BYTES)
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
        let _write = self.write_gate();
        let mut titles = self.titles();
        for (key, title) in entries {
            let value = title.as_deref().unwrap_or("");
            if value.chars().count() <= 200 {
                titles.insert(key.clone(), clean_title(value));
            }
        }
        self.titles.save(&self.root, TITLES_FILE, titles)
    }

    pub fn put_title(&self, key: &str, title: Option<&str>) -> std::io::Result<()> {
        self.put_titles(&[(key.to_string(), title.map(str::to_string))])
    }

    /// Убирает из сведений об архивах (названия, состав, ошибки упаковки, mod_info)
    /// записи модов, которых больше нет. Без этого файлы растут с каждым обновлением
    /// мода (новый размер — новый ключ), а состав архивов упирается в предел и
    /// перестаёт читаться целиком.
    fn prune_maps(&self, live_keys: &HashSet<String>) {
        let _write = self.write_gate();
        let keep = |key: &String| live_keys.contains(key);
        let mut titles = self.titles();
        if retain_changed(&mut titles, keep) {
            let _ = self.titles.save(&self.root, TITLES_FILE, titles);
        }
        let mut problems = self.problems();
        if retain_changed(&mut problems, keep) {
            let _ = self.problems.save(&self.root, PROBLEMS_FILE, problems);
        }
        let mut contents = self.contents();
        if retain_changed(&mut contents, keep) {
            let _ = self.contents.save(&self.root, CONTENTS_FILE, contents);
        }
        let mut infos = self.mod_infos();
        if retain_changed(&mut infos, keep) {
            let _ = self.mod_infos.save(&self.root, MOD_INFO_FILE, infos);
        }
    }

    fn write_gate(&self) -> std::sync::MutexGuard<'_, ()> {
        self.write.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// Названия авторов бывают с лишними пробелами («Toyota  Mark II»).
fn clean_title(title: &str) -> String {
    title.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn retain_changed<T>(map: &mut BTreeMap<String, T>, keep: impl Fn(&String) -> bool) -> bool {
    let before = map.len();
    map.retain(|key, _| keep(key));
    map.len() != before
}

/// Отметка файла: размер и время изменения. Совпала — разобранная копия свежая.
type Stamp = (u64, Option<SystemTime>);

fn stamp_of(path: &Path) -> Option<Stamp> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()))
}

type Cached<T> = Option<(Stamp, Arc<BTreeMap<String, T>>)>;

/// Один json-файл сведений `ключ → значение` с разобранной копией в памяти.
struct MapFile<T> {
    cached: Mutex<Cached<T>>,
    clean: Option<fn(&str) -> String>,
}

impl<T> MapFile<T>
where
    T: Clone + serde::Serialize + serde::de::DeserializeOwned + CleanValue,
{
    fn new(clean: Option<fn(&str) -> String>) -> Self {
        Self {
            cached: Mutex::new(None),
            clean,
        }
    }

    fn load(&self, path: &Path, max_bytes: u64) -> BTreeMap<String, T> {
        let Some(stamp) = stamp_of(path) else {
            *self.slot() = None;
            return BTreeMap::new();
        };
        if stamp.0 > max_bytes {
            return BTreeMap::new();
        }
        if let Some((cached, map)) = self.slot().as_ref()
            && *cached == stamp
        {
            return (**map).clone();
        }
        let mut map: BTreeMap<String, T> = fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        if let Some(clean) = self.clean {
            for value in map.values_mut() {
                value.clean_with(clean);
            }
        }
        *self.slot() = Some((stamp, Arc::new(map.clone())));
        map
    }

    fn save(&self, root: &Path, name: &str, map: BTreeMap<String, T>) -> std::io::Result<()> {
        fs::create_dir_all(root)?;
        let path = root.join(name);
        let json = serde_json::to_string(&map).map_err(std::io::Error::other)?;
        atomic::write_atomic(&path, json.as_bytes())?;
        *self.slot() = stamp_of(&path).map(|stamp| (stamp, Arc::new(map)));
        Ok(())
    }

    fn slot(&self) -> std::sync::MutexGuard<'_, Cached<T>> {
        self.cached
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

/// Чистка строковых значений при чтении (названия); остальные виды не трогаются.
trait CleanValue {
    fn clean_with(&mut self, _clean: fn(&str) -> String) {}
}

impl CleanValue for String {
    fn clean_with(&mut self, clean: fn(&str) -> String) {
        *self = clean(self);
    }
}

impl CleanValue for Vec<String> {}
impl CleanValue for serde_json::Value {}

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
    fn prune_also_forgets_facts_of_removed_mods() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(dir.path());
        cache
            .put_titles(&[
                ("live".to_string(), Some("Live".to_string())),
                ("dead".to_string(), Some("Dead".to_string())),
            ])
            .unwrap();
        cache
            .put_contents(&[
                ("live".to_string(), vec!["vehicles/a".to_string()]),
                ("dead".to_string(), vec!["vehicles/b".to_string()]),
            ])
            .unwrap();
        cache
            .put_problems(&[("dead".to_string(), Some("nested".to_string()))])
            .unwrap();
        cache
            .put_mod_infos(&[("dead".to_string(), serde_json::Value::Null)])
            .unwrap();
        cache.prune(&HashSet::from(["live".to_string()]));
        assert_eq!(cache.titles().keys().collect::<Vec<_>>(), vec!["live"]);
        assert_eq!(cache.contents().keys().collect::<Vec<_>>(), vec!["live"]);
        assert!(cache.problems().is_empty());
        assert!(cache.mod_infos().is_empty());
        // И на диске, а не только в памяти.
        let fresh = PreviewCache::new(dir.path());
        assert_eq!(fresh.titles().len(), 1);
        assert!(fresh.problems().is_empty());
    }

    #[test]
    fn copy_in_memory_follows_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let cache = PreviewCache::new(dir.path());
        cache
            .put_titles(&[("a".to_string(), Some("A".to_string()))])
            .unwrap();
        assert_eq!(cache.titles()["a"], "A");
        // Файл поменяли снаружи (другая копия программы, ручная правка): читаем заново.
        fs::write(
            dir.path().join(TITLES_FILE),
            r#"{"a":"Changed name","b":"B"}"#,
        )
        .unwrap();
        let titles = cache.titles();
        assert_eq!(titles["a"], "Changed name");
        assert_eq!(titles.len(), 2);
        // Файл удалили — пусто, а не старая копия.
        fs::remove_file(dir.path().join(TITLES_FILE)).unwrap();
        assert!(cache.titles().is_empty());
    }

    #[test]
    fn writes_from_two_threads_are_not_lost() {
        let dir = tempfile::tempdir().unwrap();
        let cache = std::sync::Arc::new(PreviewCache::new(dir.path()));
        let handles: Vec<_> = (0..8)
            .map(|thread| {
                let cache = cache.clone();
                std::thread::spawn(move || {
                    for index in 0..25 {
                        cache
                            .put_title(&format!("{thread}-{index}"), Some("x"))
                            .unwrap();
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(cache.titles().len(), 200);
        assert_eq!(PreviewCache::new(dir.path()).titles().len(), 200);
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

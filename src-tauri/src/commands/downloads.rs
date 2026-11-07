//! Наблюдение за «Загрузками»: готовый архив предлагается к установке.

use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use ts_rs::TS;

use bmm_domain::settings::AppSettings;

use crate::error::AppError;
use crate::state::AppState;

const STABLE_AFTER: Duration = Duration::from_millis(1500);
/// Сколько «Загрузки» смотрим после «Обновить», даже если наблюдение выключено в настройках:
/// человек сам пошёл за новой версией, и подхватить её — то, чего он ждёт.
const EXPECT_FOR: Duration = Duration::from_secs(30 * 60);

/// До какого момента (мс от начала эпохи) ждём скачанное обновление.
static EXPECT_UNTIL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Папка, в которую окно загрузки положит файл: сторож должен смотреть и её.
static EXPECT_FOLDER: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Файлы, которые ведёт окно загрузки: путь → загрузка закончилась успешно.
/// Пока окно открыто, сторож такие файлы не предлагает, сколько бы раз они ни
/// менялись: файл растёт, пока качается, и каждый новый размер — «новый» файл.
static CAUGHT: LazyLock<Mutex<HashMap<PathBuf, bool>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// Скачанные окном файлы после его закрытия: панель уже предложила их установить,
/// сторожу остаётся только запомнить их как виденные.
static HANDLED: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
pub(crate) fn note_caught(path: &Path) {
    let mut caught = CAUGHT.lock().unwrap_or_else(|error| error.into_inner());
    if caught.len() >= 128 {
        caught.clear();
    }
    caught.insert(path.to_path_buf(), false);
}

/// Размер и время докачанных окном файлов: если человек переименует или перенесёт
/// такой файл в папке, сторож узнает его и не предложит второй раз.
static FINISHED_STAMPS: Mutex<Vec<(u64, SystemTime)>> = Mutex::new(Vec::new());

fn finished_stamp(len: u64, modified: Option<SystemTime>) -> bool {
    let Some(modified) = modified else {
        return false;
    };
    FINISHED_STAMPS
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .contains(&(len, modified))
}

/// Окно загрузки докачало файл и передало его своей панели.
pub(crate) fn note_finished(path: &Path) {
    if let Ok(metadata) = std::fs::metadata(path)
        && let Ok(modified) = metadata.modified()
    {
        let mut stamps = FINISHED_STAMPS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if stamps.len() >= 128 {
            stamps.remove(0);
        }
        stamps.push((metadata.len(), modified));
    }
    if let Some(finished) = CAUGHT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .get_mut(path)
    {
        *finished = true;
    }
}

fn is_caught(path: &Path) -> bool {
    CAUGHT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .contains_key(path)
}

fn take_handled(path: &Path) -> bool {
    HANDLED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(path)
}

/// Окно загрузки закрыли. Недокачанный файл снова виден сторожу, иначе он навсегда
/// пропустил бы его как «уже забранный». Докачанный панель уже предлагала — второй
/// раз его не показываем.
pub(crate) fn forget_caught() {
    let caught = std::mem::take(&mut *CAUGHT.lock().unwrap_or_else(|error| error.into_inner()));
    let mut handled = HANDLED.lock().unwrap_or_else(|error| error.into_inner());
    if handled.len() >= 128 {
        handled.clear();
    }
    handled.extend(
        caught
            .into_iter()
            .filter_map(|(path, finished)| finished.then_some(path)),
    );
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|time| time.as_millis() as u64)
        .unwrap_or(0)
}

fn expecting() -> bool {
    now_millis() < EXPECT_UNTIL.load(std::sync::atomic::Ordering::Relaxed)
}

/// «Обновить» открыл страницу мода: следующие полчаса смотрим «Загрузки».
#[tauri::command]
pub fn downloads_expect() {
    *EXPECT_FOLDER
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = None;
    EXPECT_UNTIL.store(
        now_millis() + EXPECT_FOR.as_millis() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
}

/// «Обновить» у мода из Telegram-канала: архив отдаёт только приложение Telegram.
/// Открываем пост в нём (нет приложения — в браузере) и полчаса смотрим «Загрузки»
/// и папку Telegram Desktop: скачанный архив программа предложит установить сама.
#[tauri::command]
pub fn telegram_open(app: AppHandle, url: String) -> Result<(), AppError> {
    use bmm_sources::parsers::telegram as channel;
    use tauri_plugin_opener::OpenerExt;
    let post = url::Url::parse(url.trim())
        .ok()
        .and_then(|url| channel::post_id(&url))
        .ok_or_else(|| AppError::new("url", "Это не пост Telegram-канала с модами."))?;
    downloads_expect();
    if app
        .opener()
        .open_url(channel::app_link(post), None::<&str>)
        .is_err()
    {
        app.opener()
            .open_url(channel::post_url(post).as_str(), None::<&str>)
            .map_err(|error| AppError::new("open", error.to_string()))?;
    }
    Ok(())
}

/// Встроенное окно загрузки знает точную папку: даже если перехват не сработает,
/// сторож предложит готовый архив из неё.
pub(crate) fn expect_folder(folder: &Path) {
    *EXPECT_FOLDER
        .lock()
        .unwrap_or_else(|error| error.into_inner()) = Some(folder.to_path_buf());
    EXPECT_UNTIL.store(
        now_millis() + EXPECT_FOR.as_millis() as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
}

#[derive(Debug, Serialize, Clone, TS)]
pub(crate) struct DownloadReady {
    path: String,
}

struct Scanner {
    seen: HashSet<SeenFile>,
    pending: HashMap<PathBuf, (u64, Instant)>,
    watched: HashSet<PathBuf>,
    stable_after: Duration,
}

/// Уже показанный файл. Размер и время нужны, чтобы архив, перезаписанный
/// на месте, снова считался новым.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SeenFile {
    path: PathBuf,
    len: u64,
    modified: Option<SystemTime>,
}

impl SeenFile {
    fn of(path: PathBuf, metadata: &std::fs::Metadata) -> Self {
        Self {
            path,
            len: metadata.len(),
            modified: metadata.modified().ok(),
        }
    }
}

impl Scanner {
    fn new(stable_after: Duration) -> Self {
        Self {
            seen: HashSet::new(),
            pending: HashMap::new(),
            watched: HashSet::new(),
            stable_after,
        }
    }

    fn scan(&mut self, folders: &[PathBuf], now: Instant) -> Vec<PathBuf> {
        let mut ready = Vec::new();
        for folder in folders {
            if self.watched.insert(folder.clone()) {
                self.remember_existing(folder);
            }
            let Ok(entries) = std::fs::read_dir(folder) else {
                continue;
            };
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if !is_candidate(&path) {
                    continue;
                }
                let Ok(metadata) = entry.metadata() else {
                    continue;
                };
                if !metadata.is_file() {
                    continue;
                }
                let signature = SeenFile::of(path.clone(), &metadata);
                if self.seen.contains(&signature) {
                    continue;
                }
                if is_caught(&path) || take_handled(&path) {
                    // Файл ведёт окно загрузки: его панель сама предложит установку.
                    // Второй раз через `incoming://download` — это двойной диалог.
                    self.pending.remove(&path);
                    self.seen.insert(signature);
                    continue;
                }
                // После меток окна: иначе метка его файла осталась бы висеть.
                if self.known_elsewhere(&signature) {
                    // Тот же файл под другим именем: человек переименовал, перенёс или
                    // скопировал уже предложенный архив. Второй раз не предлагаем.
                    self.pending.remove(&path);
                    self.seen.insert(signature);
                    continue;
                }
                let size = metadata.len();
                match self.pending.get(&path) {
                    Some((previous, since)) if *previous == size => {
                        if now.duration_since(*since) >= self.stable_after {
                            self.pending.remove(&path);
                            self.seen.insert(signature);
                            ready.push(path);
                        }
                    }
                    _ => {
                        self.pending.insert(path, (size, now));
                    }
                }
            }
        }
        self.watched.retain(|folder| folders.contains(folder));
        self.pending.retain(|path, _| path.is_file());
        self.seen.retain(|file| file.path.is_file());
        ready
    }

    /// Уже виденный или докачанный окном файл с тем же размером и временем, но под
    /// другим путём. Время изменения при переименовании и переносе не меняется.
    fn known_elsewhere(&self, file: &SeenFile) -> bool {
        let Some(modified) = file.modified else {
            return false;
        };
        self.seen.iter().any(|known| {
            known.path != file.path && known.len == file.len && known.modified == Some(modified)
        }) || finished_stamp(file.len, file.modified)
    }

    fn remember_existing(&mut self, folder: &Path) {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if !is_candidate(&path) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_file() {
                self.seen.insert(SeenFile::of(path, &metadata));
            }
        }
    }
}

pub fn spawn(app: AppHandle, state: Arc<AppState>) {
    std::thread::spawn(move || {
        let mut scanner = Scanner::new(STABLE_AFTER);
        let mut cached = None;
        loop {
            std::thread::sleep(Duration::from_millis(600));
            let settings = settings_cached(&state, &mut cached);
            let folders = watch_folders(&settings, expecting());
            for path in scanner.scan(&folders, Instant::now()) {
                let _ = app.emit(
                    "incoming://download",
                    DownloadReady {
                        path: path.display().to_string(),
                    },
                );
            }
        }
    });
}

/// Настройки читаем только когда файл изменился: сторож просыпается каждые 600 мс,
/// а полный разбор JSON десять раз в минуту конкурирует с сохранением.
fn settings_cached(
    state: &AppState,
    cached: &mut Option<((SystemTime, u64), bmm_domain::settings::AppSettings)>,
) -> bmm_domain::settings::AppSettings {
    let stamp = std::fs::metadata(state.paths.settings())
        .ok()
        .and_then(|meta| meta.modified().ok().map(|modified| (modified, meta.len())));
    if let (Some((known, settings)), Some(stamp)) = (cached.as_ref(), stamp)
        && *known == stamp
    {
        return settings.clone();
    }
    let settings = state.settings.load().settings;
    *cached = stamp.map(|stamp| (stamp, settings.clone()));
    settings
}

fn watch_folders(settings: &bmm_domain::settings::AppSettings, expecting: bool) -> Vec<PathBuf> {
    watch_folders_in(settings, expecting, downloads_folder())
}

/// Куда Telegram Desktop сохраняет файлы по умолчанию.
pub(crate) const TELEGRAM_FOLDER: &str = "Telegram Desktop";

fn watch_folders_in(
    settings: &bmm_domain::settings::AppSettings,
    expecting: bool,
    downloads: Option<PathBuf>,
) -> Vec<PathBuf> {
    let expected = expecting.then(|| {
        EXPECT_FOLDER
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    });
    let expected = expected.flatten();
    let mut folders: Vec<PathBuf> = Vec::new();
    if let Some(folder) = &expected {
        folders.push(folder.clone());
    }
    if (settings.allow_downloads || (expecting && expected.is_none()))
        && let Some(downloads) = downloads
    {
        // Архивы из Telegram-канала Telegram Desktop кладёт в свою папку внутри «Загрузок».
        if !settings.source_off(bmm_sources::parsers::telegram::SOURCE_ID) {
            folders.push(downloads.join(TELEGRAM_FOLDER));
        }
        folders.push(downloads);
    }
    if settings.allow_downloads
        && let Some(folder) = settings
            .downloads_folder
            .as_deref()
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
            .map(PathBuf::from)
            .filter(|folder| folder.is_absolute())
    {
        folders.push(folder);
    }
    for folder in &settings.watch_folders {
        folders.push(PathBuf::from(folder));
    }
    let mut seen = HashSet::new();
    folders
        .retain(|folder| folder.is_dir() && seen.insert(folder.to_string_lossy().to_lowercase()));
    folders
}

fn is_candidate(path: &Path) -> bool {
    let Some(name) = path.file_name().map(|name| name.to_string_lossy()) else {
        return false;
    };
    if name.starts_with('.') {
        return false;
    }
    let lower = name.to_lowercase();
    [".zip", ".rar", ".7z"]
        .iter()
        .any(|extension| lower.ends_with(extension))
}

/// Папка, куда окно загрузки кладёт архивы: выбранная, иначе системные «Загрузки»,
/// иначе папка данных программы. Проверяем запись, а не только существование.
/// Папку внутри папки модов пропускаем: иначе игра увидит скачанный архив как мод,
/// а установщик откажется заменять файл, который уже лежит на месте.
pub(crate) fn download_folder(
    settings: &AppSettings,
    app_root: &Path,
    mods_dir: Option<&Path>,
) -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(folder) = settings
        .downloads_folder
        .as_deref()
        .map(str::trim)
        .filter(|folder| !folder.is_empty())
        .map(PathBuf::from)
        .filter(|folder| folder.is_absolute())
        .filter(|folder| mods_dir.is_none_or(|mods| !is_inside(folder, mods)))
    {
        candidates.push(folder);
    }
    if let Some(system) = downloads_folder() {
        candidates.push(system);
    }
    candidates.push(app_root.join("downloads"));
    pick_writable(candidates)
}

fn is_inside(path: &Path, folder: &Path) -> bool {
    let path = std::fs::canonicalize(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    let folder = std::fs::canonicalize(folder)
        .or_else(|_| std::path::absolute(folder))
        .unwrap_or_else(|_| folder.to_path_buf());
    #[cfg(windows)]
    {
        let path = path.to_string_lossy().to_lowercase();
        let folder = folder.to_string_lossy().to_lowercase();
        let folder = folder.trim_end_matches(['\\', '/']);
        path == folder || path.starts_with(&format!("{folder}\\"))
    }
    #[cfg(not(windows))]
    {
        path.starts_with(&folder)
    }
}

fn pick_writable(candidates: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    candidates.into_iter().find(|path| writable_dir(path))
}

fn writable_dir(path: &Path) -> bool {
    if std::fs::create_dir_all(path).is_err() {
        return false;
    }
    // Счётчик, а не только миллисекунды: параллельные проверки в одном процессе
    // иначе выбирают одно имя и мешают друг другу.
    static PROBE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let probe = path.join(format!(
        ".bmm-write-{}-{}-{}",
        std::process::id(),
        now_millis(),
        PROBE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    match OpenOptions::new().write(true).create_new(true).open(&probe) {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(windows)]
pub(crate) fn downloads_folder() -> Option<PathBuf> {
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath};

    let mut raw: *mut u16 = std::ptr::null_mut();
    let result =
        unsafe { SHGetKnownFolderPath(&FOLDERID_Downloads, 0, std::ptr::null_mut(), &mut raw) };
    if result < 0 || raw.is_null() {
        return fallback_downloads();
    }
    let length = unsafe {
        let mut length = 0usize;
        while *raw.add(length) != 0 {
            length += 1;
        }
        length
    };
    let value = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(raw, length) });
    unsafe { CoTaskMemFree(raw as *const core::ffi::c_void) };
    let path = PathBuf::from(value);
    path.is_dir().then_some(path).or_else(fallback_downloads)
}

#[cfg(not(windows))]
pub(crate) fn downloads_folder() -> Option<PathBuf> {
    fallback_downloads()
}

fn fallback_downloads() -> Option<PathBuf> {
    let profile = std::env::var_os("USERPROFILE").map(PathBuf::from)?;
    let downloads = profile.join("Downloads");
    downloads.is_dir().then_some(downloads)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Метки перехвата — общие на процесс; тесты, которые их трогают, идут по очереди.
    static CAUGHT_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Своё время изменения для файла теста. На Windows время идёт шагами около 15 мс:
    /// файлы параллельных тестов одного размера получали одно время, и сторож
    /// принимал файл одного теста за переименованный файл другого.
    fn unique_stamp(path: &Path) -> SystemTime {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let step = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let stamp = SystemTime::UNIX_EPOCH + Duration::from_secs(1_500_000_000 + step * 97);
        std::fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(stamp)
            .unwrap();
        stamp
    }

    #[test]
    fn renamed_archive_is_not_offered_again() {
        // Было: человек переименовал предложенный архив в «Загрузках» — сторож видел
        // «новый» файл и предлагал установить его второй раз.
        let temp = tempfile::tempdir().unwrap();
        let folder = [temp.path().to_path_buf()];
        let path = temp.path().join("car.zip");
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        scanner.scan(&folder, start);
        std::fs::write(&path, b"archive").unwrap();
        unique_stamp(&path);
        scanner.scan(&folder, at(10));
        assert_eq!(scanner.scan(&folder, at(20)), vec![path.clone()]);

        let renamed = temp.path().join("car_v2.zip");
        std::fs::rename(&path, &renamed).unwrap();
        for ms in [30, 40, 1000] {
            assert!(
                scanner.scan(&folder, at(ms)).is_empty(),
                "переименованный — тот же"
            );
        }
        // Перенесли в другую наблюдаемую папку — тоже тот же.
        let other = tempfile::tempdir().unwrap();
        let both = [temp.path().to_path_buf(), other.path().to_path_buf()];
        scanner.scan(&both, at(1100));
        std::fs::rename(&renamed, other.path().join("car_v2.zip")).unwrap();
        for ms in [1200, 1300, 2000] {
            assert!(
                scanner.scan(&both, at(ms)).is_empty(),
                "перенесённый — тот же"
            );
        }
    }

    #[test]
    fn copy_of_an_offered_archive_is_not_offered_again() {
        let temp = tempfile::tempdir().unwrap();
        let folder = [temp.path().to_path_buf()];
        let path = temp.path().join("car.zip");
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        scanner.scan(&folder, start);
        std::fs::write(&path, b"archive").unwrap();
        unique_stamp(&path);
        scanner.scan(&folder, at(10));
        assert_eq!(scanner.scan(&folder, at(20)), vec![path.clone()]);
        // Копия в Проводнике сохраняет время изменения.
        let copy = temp.path().join("car - copy.zip");
        std::fs::copy(&path, &copy).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::File::options()
            .write(true)
            .open(&copy)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        for ms in [30, 40, 1000] {
            assert!(scanner.scan(&folder, at(ms)).is_empty());
        }
    }

    #[test]
    fn other_archive_of_the_same_size_is_still_offered() {
        // Совпал только размер, время другое — это другой архив.
        let temp = tempfile::tempdir().unwrap();
        let folder = [temp.path().to_path_buf()];
        let first = temp.path().join("a.zip");
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        scanner.scan(&folder, start);
        std::fs::write(&first, b"same size 1").unwrap();
        unique_stamp(&first);
        scanner.scan(&folder, at(10));
        assert_eq!(scanner.scan(&folder, at(20)), vec![first.clone()]);
        let second = temp.path().join("b.zip");
        std::fs::write(&second, b"same size 2").unwrap();
        unique_stamp(&second);
        scanner.scan(&folder, at(30));
        assert_eq!(scanner.scan(&folder, at(40)), vec![second]);
    }

    #[test]
    fn window_archive_renamed_before_the_watcher_saw_it_is_not_offered() {
        // Окно докачало файл, а человек сразу его переименовал — раньше, чем сторож
        // успел заглянуть в папку. Панель окна архив уже взяла.
        let _guard = CAUGHT_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let temp = tempfile::tempdir().unwrap();
        let folder = [temp.path().to_path_buf()];
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        scanner.scan(&folder, start);
        let path = temp.path().join("from_window.zip");
        note_caught(&path);
        std::fs::write(&path, b"downloaded by the window, unique body").unwrap();
        unique_stamp(&path);
        note_finished(&path);
        std::fs::rename(&path, temp.path().join("renamed.zip")).unwrap();
        for ms in [10, 20, 1000] {
            assert!(scanner.scan(&folder, at(ms)).is_empty());
        }
    }

    #[test]
    fn reports_only_stable_new_archives() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("car.zip");
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        assert!(scanner.scan(&[temp.path().to_path_buf()], start).is_empty());
        std::fs::write(&path, b"one").unwrap();
        assert!(
            scanner
                .scan(
                    &[temp.path().to_path_buf()],
                    start + Duration::from_millis(10)
                )
                .is_empty(),
            "новый файл: ждём стабильности"
        );
        std::fs::write(&path, b"one plus").unwrap();
        assert!(
            scanner
                .scan(
                    &[temp.path().to_path_buf()],
                    start + Duration::from_millis(20)
                )
                .is_empty(),
            "файл изменился — ждём ещё"
        );
        assert_eq!(
            scanner.scan(
                &[temp.path().to_path_buf()],
                start + Duration::from_millis(30)
            ),
            vec![path.clone()]
        );
        assert!(
            scanner
                .scan(&[temp.path().to_path_buf()], start + Duration::from_secs(1))
                .is_empty(),
            "второй раз не предлагаем"
        );
    }

    #[test]
    fn closed_window_returns_unfinished_download_to_the_watcher() {
        let _guard = CAUGHT_TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("car.zip");
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        assert!(scanner.scan(&[temp.path().to_path_buf()], start).is_empty());

        std::fs::write(&path, b"one").unwrap();
        note_caught(&path);
        forget_caught();

        assert!(
            scanner
                .scan(
                    &[temp.path().to_path_buf()],
                    start + Duration::from_millis(10)
                )
                .is_empty(),
            "новый файл: ждём стабильности"
        );
        assert_eq!(
            scanner.scan(
                &[temp.path().to_path_buf()],
                start + Duration::from_millis(20)
            ),
            vec![path],
            "после закрытия окна архив снова предлагается"
        );
    }

    #[test]
    fn rewritten_archive_is_offered_again() {
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().to_path_buf();
        let path = folder.join("car.zip");
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), start)
                .is_empty()
        );

        std::fs::write(&path, b"one").unwrap();
        assert!(
            scanner
                .scan(
                    std::slice::from_ref(&folder),
                    start + Duration::from_millis(10)
                )
                .is_empty()
        );
        assert_eq!(
            scanner.scan(
                std::slice::from_ref(&folder),
                start + Duration::from_millis(20)
            ),
            vec![path.clone()]
        );

        // Тот же путь, другой файл: новый размер — новый архив.
        std::fs::write(&path, b"two is longer").unwrap();
        assert!(
            scanner
                .scan(
                    std::slice::from_ref(&folder),
                    start + Duration::from_millis(30)
                )
                .is_empty(),
            "снова ждём стабильности"
        );
        assert_eq!(
            scanner.scan(
                std::slice::from_ref(&folder),
                start + Duration::from_millis(40)
            ),
            vec![path]
        );
    }

    #[test]
    fn download_folder_prefers_the_configured_writable_folder() {
        let temp = tempfile::tempdir().unwrap();
        let custom = temp.path().join("BeamNG downloads");
        std::fs::create_dir_all(&custom).unwrap();
        let settings = AppSettings {
            downloads_folder: Some(custom.display().to_string()),
            ..AppSettings::default()
        };
        assert_eq!(
            download_folder(&settings, temp.path(), None),
            Some(custom.clone())
        );
    }

    #[test]
    fn relative_download_folder_is_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let settings = AppSettings {
            downloads_folder: Some("relative/Downloads".to_string()),
            ..AppSettings::default()
        };
        let chosen = download_folder(&settings, temp.path(), None).expect("запасная папка");
        assert_ne!(chosen, PathBuf::from("relative/Downloads"));
        assert!(chosen.is_absolute());
    }

    #[test]
    fn broken_download_folder_falls_back_to_a_writable_one() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("not-a-folder");
        std::fs::write(&file, b"x").unwrap();
        let settings = AppSettings {
            downloads_folder: Some(file.display().to_string()),
            ..AppSettings::default()
        };
        let chosen = download_folder(&settings, temp.path(), None).expect("запасная папка");
        assert_ne!(chosen, file);
        assert!(writable_dir(&chosen));
    }

    #[test]
    fn downloads_folder_inside_mods_is_ignored() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        let inside = mods.join("downloads");
        std::fs::create_dir_all(&inside).unwrap();
        let settings = AppSettings {
            downloads_folder: Some(inside.display().to_string()),
            ..AppSettings::default()
        };
        let chosen = download_folder(&settings, temp.path(), Some(&mods)).expect("запасная папка");
        assert_ne!(chosen, inside);
        assert!(!is_inside(&chosen, &mods));
    }

    #[test]
    fn caught_download_is_not_offered_twice() {
        let _guard = CAUGHT_TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().to_path_buf();
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), start)
                .is_empty()
        );
        let path = folder.join("caught.zip");
        std::fs::write(&path, b"archive").unwrap();
        note_caught(&path);
        assert!(
            scanner
                .scan(
                    std::slice::from_ref(&folder),
                    start + Duration::from_millis(20)
                )
                .is_empty(),
            "окно загрузки уже забрало файл"
        );
        assert!(
            scanner
                .scan(&[folder], start + Duration::from_secs(1))
                .is_empty()
        );
        assert!(
            scanner
                .scan(&[temp.path().to_path_buf()], start + Duration::from_secs(3))
                .is_empty(),
            "после того как окно загрузки забрало файл, сторож не должен предлагать его снова"
        );
    }

    #[test]
    fn growing_download_of_the_window_is_not_offered_twice() {
        // Было: метка снималась при первом взгляде, а файл ещё качался. Следующий
        // размер считался новым файлом, и после загрузки главное окно предлагало
        // «Обновить» тот же архив, который уже установила панель окна загрузки.
        let _guard = CAUGHT_TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().to_path_buf();
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), start)
                .is_empty()
        );
        let path = folder.join("car_v1.6.zip");
        note_caught(&path);
        std::fs::write(&path, b"part").unwrap();
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), at(10))
                .is_empty()
        );
        std::fs::write(&path, b"part and more").unwrap();
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), at(20))
                .is_empty()
        );
        std::fs::write(&path, b"part and more, whole archive").unwrap();
        unique_stamp(&path);
        note_finished(&path);
        for ms in [30, 40, 1000] {
            assert!(
                scanner
                    .scan(std::slice::from_ref(&folder), at(ms))
                    .is_empty(),
                "файл ведёт окно загрузки — сторож молчит"
            );
        }
        forget_caught();
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), at(2000))
                .is_empty()
        );
        assert!(
            scanner.scan(&[folder], at(3000)).is_empty(),
            "окно закрыли после загрузки — архив уже предлагала его панель"
        );
    }

    #[test]
    fn handled_download_is_offered_again_when_rewritten() {
        let _guard = CAUGHT_TEST_LOCK.lock().unwrap();
        let temp = tempfile::tempdir().unwrap();
        let folder = temp.path().to_path_buf();
        let mut scanner = Scanner::new(Duration::from_millis(5));
        let start = Instant::now();
        let at = |ms: u64| start + Duration::from_millis(ms);
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), start)
                .is_empty()
        );
        let path = folder.join("car.zip");
        note_caught(&path);
        std::fs::write(&path, b"from window").unwrap();
        unique_stamp(&path);
        note_finished(&path);
        forget_caught();
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), at(10))
                .is_empty()
        );
        // Потом тот же файл скачали ещё раз браузером — это уже новый архив.
        std::fs::write(&path, b"downloaded again by browser").unwrap();
        assert!(
            scanner
                .scan(std::slice::from_ref(&folder), at(20))
                .is_empty()
        );
        assert_eq!(scanner.scan(&[folder], at(30)), vec![path]);
    }

    #[test]
    fn existing_archives_are_not_offered_on_start() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("old.zip"), b"old").unwrap();
        let mut scanner = Scanner::new(Duration::ZERO);
        assert!(
            scanner
                .scan(&[temp.path().to_path_buf()], Instant::now())
                .is_empty()
        );
        assert!(
            scanner
                .scan(
                    &[temp.path().to_path_buf()],
                    Instant::now() + Duration::from_secs(5)
                )
                .is_empty()
        );
    }

    #[test]
    fn ignores_other_files_and_missing_folders() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("photo.jpg"), b"x").unwrap();
        std::fs::write(temp.path().join(".hidden.zip"), b"x").unwrap();
        std::fs::write(temp.path().join("archive.zip.crdownload"), b"x").unwrap();
        let mut scanner = Scanner::new(Duration::ZERO);
        let missing = temp.path().join("missing");
        assert!(
            scanner
                .scan(&[temp.path().to_path_buf(), missing], Instant::now())
                .is_empty()
        );
    }

    #[test]
    fn downloads_are_watched_when_allowed_or_after_update_click() {
        let off = bmm_domain::settings::AppSettings::default();
        assert!(!off.allow_downloads);
        let Some(downloads) = downloads_folder() else {
            return; // нет папки «Загрузки» в этом окружении
        };
        assert!(!watch_folders(&off, false).contains(&downloads));
        assert!(watch_folders(&off, true).contains(&downloads));
        let on = bmm_domain::settings::AppSettings {
            allow_downloads: true,
            ..off
        };
        assert!(watch_folders(&on, false).contains(&downloads));
    }

    #[test]
    fn telegram_folder_is_watched_with_downloads_when_the_channel_is_on() {
        let temp = tempfile::tempdir().unwrap();
        let downloads = temp.path().join("Downloads");
        let telegram = downloads.join(TELEGRAM_FOLDER);
        std::fs::create_dir_all(&telegram).unwrap();
        let mut settings = bmm_domain::settings::AppSettings::default();
        // Канал выключен (по умолчанию) — его папку не смотрим.
        let off = watch_folders_in(&settings, true, Some(downloads.clone()));
        assert!(off.contains(&downloads));
        assert!(!off.contains(&telegram));
        settings.set_off_sources(&[]);
        let on = watch_folders_in(&settings, true, Some(downloads.clone()));
        assert!(on.contains(&telegram) && on.contains(&downloads));
        // Без «Обновить» и без разрешения смотреть «Загрузки» — ни той, ни другой.
        assert!(watch_folders_in(&settings, false, Some(downloads.clone())).is_empty());
        // Папки Telegram нет — смотреть нечего, но «Загрузки» остаются.
        std::fs::remove_dir(&telegram).unwrap();
        assert_eq!(
            watch_folders_in(&settings, true, Some(downloads.clone())),
            [downloads]
        );
    }

    #[test]
    fn update_click_opens_a_half_hour_window() {
        EXPECT_UNTIL.store(0, std::sync::atomic::Ordering::Relaxed);
        assert!(!expecting());
        downloads_expect();
        assert!(expecting());
        let until = EXPECT_UNTIL.load(std::sync::atomic::Ordering::Relaxed);
        assert!(until >= now_millis() + 29 * 60 * 1000);
    }

    #[cfg(windows)]
    #[test]
    fn downloads_folder_exists_or_falls_back() {
        if let Some(path) = downloads_folder() {
            assert!(path.is_dir());
        }
    }
}

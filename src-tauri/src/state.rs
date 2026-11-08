use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use tokio::sync::Semaphore;

use bmm_app_release::{CancelToken, PreparedUpdate, UpdateDownloader, UpdateState};
use bmm_checks::coordinator::UpdateCoordinator;
use bmm_domain::library::JsonLibraryStore;
use bmm_domain::matches::MatchDecisionStore;
use bmm_domain::models::Listing;
use bmm_domain::paths::AppPaths;
use bmm_domain::settings_store::SettingsStore;
use bmm_domain::text::ModKind;
use bmm_packs::cache::PreviewCache;
use bmm_sources::images::RemoteImageCache;
use bmm_sources::net::PoliteFetcher;
use bmm_sources::search::{SearchResponse, SearchService, SourceReport};
use bmm_sources::sources::ModSource;

#[derive(Debug, Clone)]
pub struct PreviewTarget {
    pub path: PathBuf,
    pub key: String,
    pub file_name: String,
    pub sub_folder: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DeletedTicket {
    pub mod_id: String,
    pub file_name: String,
    pub backup_id: String,
    pub was_in_disabled: bool,
    pub was_enabled: bool,
}

#[derive(Debug, Clone)]
pub struct CachedContent {
    pub len: u64,
    pub modified: Option<SystemTime>,
    pub content: HashSet<String>,
}

pub struct SearchRuntime {
    pub fetcher: Arc<PoliteFetcher>,
    pub service: Arc<SearchService>,
    pub matches: Arc<MatchDecisionStore>,
    pub previews: Arc<RemoteImageCache>,
    pub sources: Arc<Vec<Arc<dyn ModSource>>>,
}

pub struct UpdateRuntime {
    pub coordinator: Arc<UpdateCoordinator>,
    pub enabled: Arc<Mutex<HashSet<String>>>,
    pub sources: Arc<Vec<Arc<dyn ModSource>>>,
    pub page_cancel: Arc<AtomicBool>,
    pub page_busy: Arc<AtomicBool>,
}

pub struct AppUpdateRuntime {
    pub downloader: UpdateDownloader,
    pub state: UpdateState,
    pub prepared: Option<PreparedUpdate>,
    pub cancel: CancelToken,
    /// Номер попытки скачивания: старый поток не должен затирать новое состояние.
    pub attempt: u64,
    pub update_dir: PathBuf,
    pub skipped_path: PathBuf,
    pub manifest_url: url::Url,
}

/// Накопленная выдача раздела: страницы сайтов и последние отчёты.
pub struct BrowseSession {
    pub kind: ModKind,
    /// Сколько страниц раздела уже загружено с каждого сайта.
    pub pages: u32,
    /// Порядок сайтов в панели и в выдаче.
    pub order: Vec<String>,
    pub per_source: BTreeMap<String, Vec<Listing>>,
    pub reports: BTreeMap<String, SourceReport>,
    pub response: SearchResponse,
}

pub struct AppState {
    pub paths: AppPaths,
    pub settings: SettingsStore,
    pub library: Arc<JsonLibraryStore>,
    pub previews: PreviewCache,
    pub load_lock: Mutex<()>,
    pub previews_by_id: Mutex<HashMap<String, PreviewTarget>>,
    pub pending: Mutex<HashMap<String, bool>>,
    pub preview_gate: Arc<Semaphore>,
    pub deleted: Mutex<HashMap<String, DeletedTicket>>,
    pub content_cache: Mutex<HashMap<PathBuf, CachedContent>>,
    pub search: Option<Arc<SearchRuntime>>,
    pub update: Option<Arc<UpdateRuntime>>,
    pub app_update: Option<Arc<Mutex<AppUpdateRuntime>>>,
    pub search_cancel: Mutex<Option<(u64, Arc<AtomicBool>)>>,
    pub last_search: Mutex<Option<SearchResponse>>,
    pub browse: Mutex<Option<BrowseSession>>,
}

/// Замок, переживший панику держателя. Паника в одной команде (битый архив и т.п.)
/// отравляла замок, и все следующие команды библиотеки отвечали «не удалось
/// заблокировать» до перезапуска. Данные под нашими замками целостны и после паники:
/// их меняют одной записью.
pub fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl AppState {
    /// Очередь к библиотеке: скан, установка, удаление, переключение.
    pub fn load_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        lock(&self.load_lock)
    }

    pub fn save_pending(&self) -> std::io::Result<()> {
        let path = self.paths.root().join("pending.json");
        let pending = crate::state::lock(&self.pending).clone();
        let json = serde_json::to_string_pretty(&pending).map_err(std::io::Error::other)?;
        bmm_domain::atomic::write_atomic(&path, json.as_bytes())
    }

    pub fn load_pending(&self) {
        let path = self.paths.root().join("pending.json");
        let Ok(text) = std::fs::read_to_string(path) else {
            return;
        };
        let Ok(map) = serde_json::from_str::<HashMap<String, bool>>(&text) else {
            return;
        };
        *crate::state::lock(&self.pending) = map;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_survives_a_panicked_holder() {
        let mutex = std::sync::Arc::new(Mutex::new(1));
        let held = mutex.clone();
        let _ = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("упали, держа замок");
        })
        .join();
        assert!(mutex.is_poisoned());
        *lock(&mutex) += 1;
        assert_eq!(*lock(&mutex), 2);
    }

    /// Упавший поток не должен ронять всю программу при следующем обращении к
    /// общему состоянию: в рабочем коде — только `crate::state::lock`.
    #[test]
    fn production_code_does_not_unwrap_locks() {
        let unwraps_lock = |body: &str| {
            body.match_indices(".lock()").any(|(at, found)| {
                body[at + found.len()..]
                    .trim_start()
                    .starts_with(".unwrap()")
            })
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![root];
        let mut found = Vec::new();
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|ext| ext != "rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&path).unwrap();
                let body = text.split("#[cfg(test)]\nmod tests").next().unwrap_or("");
                if unwraps_lock(body) {
                    found.push(path.display().to_string());
                }
            }
        }
        assert!(found.is_empty(), "{found:?}");
    }
}

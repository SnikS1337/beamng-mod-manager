use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use tokio::sync::Semaphore;

use bmm_app_release::{CancelToken, PreparedUpdate, UpdateDownloader, UpdateState};
use bmm_checks::coordinator::UpdateCoordinator;
use bmm_domain::library::JsonLibraryStore;
use bmm_domain::matches::MatchDecisionStore;
use bmm_domain::paths::AppPaths;
use bmm_domain::settings_store::SettingsStore;
use bmm_packs::cache::PreviewCache;
use bmm_sources::images::RemoteImageCache;
use bmm_sources::net::PoliteFetcher;
use bmm_sources::search::{SearchResponse, SearchService};
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
}

pub struct UpdateRuntime {
    pub coordinator: Arc<UpdateCoordinator>,
    pub enabled: Arc<Mutex<HashSet<String>>>,
    pub sources: Arc<Vec<Arc<dyn ModSource>>>,
}

pub struct AppUpdateRuntime {
    pub downloader: UpdateDownloader,
    pub state: UpdateState,
    pub prepared: Option<PreparedUpdate>,
    pub cancel: CancelToken,
    pub update_dir: PathBuf,
    pub skipped_path: PathBuf,
    pub manifest_url: url::Url,
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
}

impl AppState {
    pub fn save_pending(&self) -> std::io::Result<()> {
        let path = self.paths.root().join("pending.json");
        let pending = self.pending.lock().unwrap().clone();
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
        *self.pending.lock().unwrap() = map;
    }
}

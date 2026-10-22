mod commands;
mod error;
mod named_mutex;
mod state;

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use bmm_checks::{UpdateCheckOptions, UpdateCoordinator, UpdateCoordinatorOptions};
use bmm_domain::clock::system_clock;
use bmm_domain::library::JsonLibraryStore;
use bmm_domain::matches::{MatchDecisionStore, MatchDecisions};
use bmm_domain::paths::AppPaths;
use bmm_domain::settings_store::SettingsStore;
use bmm_packs::cache::PreviewCache;
use bmm_sources::net::{PoliteFetcher, PoliteOptions, default_fetcher};
use bmm_sources::search::{SearchOptions, SearchService};
use bmm_sources::sources::{ModSource, all_sources};
use state::{AppState, SearchRuntime, UpdateRuntime};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _instance = match named_mutex::acquire() {
        Some(guard) => guard,
        None => {
            named_mutex::show_already_running();
            return;
        }
    };

    let paths = match AppPaths::resolve() {
        Ok(paths) => paths,
        Err(error) => {
            eprintln!("[bmm] не удалось определить каталог данных: {error}");
            return;
        }
    };

    if let Err(error) = paths.ensure_dirs() {
        eprintln!("[bmm] не удалось создать каталоги данных: {error}");
    }

    let matches = Arc::new(MatchDecisionStore::new(
        Some(paths.matches()),
        system_clock(),
    ));
    let settings_store = SettingsStore::with_system_clock(paths.settings());
    let settings_snapshot = settings_store.load().settings;
    let library = Arc::new(JsonLibraryStore::new(paths.library(), system_clock()));
    let fetcher = Arc::new(PoliteFetcher::new(
        default_fetcher(),
        PoliteOptions::default(),
    ));
    let source_list: Arc<Vec<Arc<dyn ModSource>>> = Arc::new(all_sources(fetcher.clone()));
    let status_fetcher = fetcher.clone();
    let decisions: Arc<dyn MatchDecisions + Send + Sync> = matches.clone();
    let search_service = Arc::new(SearchService::new(
        source_list.as_ref().to_vec(),
        Arc::new(move |source| status_fetcher.status_of(source)),
        SearchOptions::default(),
        Some(decisions),
    ));
    let enabled_sources = Arc::new(Mutex::new(HashSet::new()));
    let factory_sources = source_list.clone();
    let factory_enabled = enabled_sources.clone();
    let clear_fetcher = fetcher.clone();
    let reset_fetcher = fetcher.clone();
    let homes: HashMap<String, url::Url> = source_list
        .iter()
        .map(|source| (source.id().to_string(), source.home()))
        .collect();
    let coordinator = UpdateCoordinator::new(
        library.clone(),
        Arc::new(move || {
            let enabled = factory_enabled.lock().unwrap();
            factory_sources
                .iter()
                .filter(|source| enabled.contains(source.id()))
                .cloned()
                .collect()
        }),
        system_clock(),
    )
    .with_options(UpdateCoordinatorOptions {
        check: UpdateCheckOptions::from_settings(&settings_snapshot),
        ..UpdateCoordinatorOptions::default()
    })
    .with_clear_cache(Arc::new(move || clear_fetcher.clear_cache()))
    .with_reset_source(Arc::new(move |source| {
        if let Some(url) = homes.get(source) {
            reset_fetcher.reset_source(source, url);
        }
    }));
    let search = Some(Arc::new(SearchRuntime {
        fetcher,
        service: search_service,
        matches,
        previews: Arc::new(bmm_sources::images::RemoteImageCache::new(
            paths.search_previews(),
        )),
    }));
    let update = Some(Arc::new(UpdateRuntime {
        coordinator: Arc::new(coordinator),
        enabled: enabled_sources,
        sources: source_list,
    }));

    let state = Arc::new(AppState {
        settings: settings_store,
        library: Arc::clone(&library),
        previews: PreviewCache::new(paths.previews()),
        paths,
        load_lock: Mutex::new(()),
        previews_by_id: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashMap::new()),
        preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
        deleted: Mutex::new(HashMap::new()),
        content_cache: Mutex::new(HashMap::new()),
        search,
        update,
        search_cancel: Mutex::new(None),
        last_search: Mutex::new(None),
    });
    state.load_pending();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(state)
        .register_uri_scheme_protocol("bmm", |context, request| {
            preview_protocol(context.app_handle(), request)
        })
        .setup(|app| {
            if std::env::args().any(|arg| arg == "--probe") {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(error) = commands::dev::open_modland_probe(handle).await {
                        eprintln!("[bmm] modland probe failed: {error}");
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app::app_bootstrap,
            commands::dev::dev_library,
            commands::dev::dev_environment,
            commands::dev::open_modland_probe,
            commands::library::library_load,
            commands::library::library_refresh,
            commands::library::mod_toggle,
            commands::library::mod_details,
            commands::library::mod_preview,
            commands::versions::mod_delete,
            commands::versions::mod_undelete,
            commands::versions::mod_forget_deleted,
            commands::versions::mod_restore_deleted,
            commands::versions::mod_versions,
            commands::versions::mod_restore_version,
            commands::incoming::incoming_inspect,
            commands::incoming::incoming_install,
            commands::incoming::incoming_undo,
            commands::incoming::incoming_discard,
            commands::search::search_start,
            commands::search::search_cancel,
            commands::search::search_decide,
            commands::search::search_preview,
            commands::updates::updates_snapshot,
            commands::updates::updates_start,
            commands::updates::updates_cancel,
            commands::updates::updates_dismiss,
            commands::updates::updates_restore,
            commands::updates::updates_find_pages,
            commands::updates::updates_confirm_page,
            commands::updates::updates_reject_page,
            commands::updates::updates_undo_page,
            commands::updates::updates_resolve_human
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn preview_protocol(
    app: &tauri::AppHandle,
    request: tauri::http::Request<Vec<u8>>,
) -> tauri::http::Response<Cow<'static, [u8]>> {
    let not_found = || {
        tauri::http::Response::builder()
            .status(404)
            .header("Content-Type", "text/plain")
            .body(Cow::Borrowed(&b"not found"[..]))
            .expect("static response")
    };

    let Some(name) = request.uri().path().strip_prefix("/preview/") else {
        if let Some(name) = request.uri().path().strip_prefix("/search-preview/") {
            if name.is_empty() || name.contains('/') || name.contains("..") {
                return not_found();
            }
            let state = app.state::<Arc<AppState>>();
            let path = state.paths.search_previews().join(name);
            let Ok(bytes) = std::fs::read(path) else {
                return not_found();
            };
            let mime = if name.ends_with(".png") {
                "image/png"
            } else if name.ends_with(".webp") {
                "image/webp"
            } else {
                "image/jpeg"
            };
            return tauri::http::Response::builder()
                .status(200)
                .header("Content-Type", mime)
                .header("Cache-Control", "no-cache")
                .body(Cow::Owned(bytes))
                .expect("preview response");
        }
        return not_found();
    };
    if name.is_empty() || name.contains('/') || name.contains("..") {
        return not_found();
    }
    let state = app.state::<Arc<AppState>>();
    let path = state.previews.root().join(name);
    let Ok(bytes) = std::fs::read(path) else {
        return not_found();
    };
    let mime = if name.ends_with(".png") {
        "image/png"
    } else {
        "image/jpeg"
    };
    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Cache-Control", "no-cache")
        .body(Cow::Owned(bytes))
        .expect("preview response")
}

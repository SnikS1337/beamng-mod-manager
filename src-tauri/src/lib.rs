mod commands;
mod error;
mod named_mutex;
mod state;

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use bmm_domain::clock::system_clock;
use bmm_domain::library::JsonLibraryStore;
use bmm_domain::paths::AppPaths;
use bmm_domain::settings_store::SettingsStore;
use bmm_packs::cache::PreviewCache;
use state::AppState;
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

    let state = Arc::new(AppState {
        settings: SettingsStore::with_system_clock(paths.settings()),
        library: JsonLibraryStore::new(paths.library(), system_clock()),
        previews: PreviewCache::new(paths.previews()),
        paths,
        load_lock: Mutex::new(()),
        previews_by_id: Mutex::new(HashMap::new()),
        pending: Mutex::new(HashMap::new()),
        preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
        deleted: Mutex::new(HashMap::new()),
    });
    state.load_pending();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
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
            commands::versions::mod_restore_version
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

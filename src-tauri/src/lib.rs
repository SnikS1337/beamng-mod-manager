mod browser_pages;
mod commands;
mod error;
mod named_mutex;
mod state;
#[cfg(test)]
mod test_support;

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
use bmm_sources::net::{
    PageFetcher, PoliteFetcher, PoliteOptions, RoutingPageFetcher, default_fetcher,
};
use bmm_sources::search::{SearchOptions, SearchService};
use bmm_sources::sources::{ModSource, all_sources};
use state::{AppState, AppUpdateRuntime, SearchRuntime, UpdateRuntime};
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    if let Some(arguments) = commands::app_update::installer_args() {
        commands::app_update::run_installer(arguments);
        return;
    }
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
    commands::app_update::remember_rolled_back(&paths.root().join("update-skip.txt"));

    let matches = Arc::new(MatchDecisionStore::new(
        Some(paths.matches()),
        system_clock(),
    ));
    let settings_store = SettingsStore::with_system_clock(paths.settings());
    let settings_snapshot = settings_store.load().settings;
    let library = Arc::new(JsonLibraryStore::new(paths.library(), system_clock()));
    let browser_pages = Arc::new(browser_pages::BrowserPages::new());
    let pages: Arc<dyn PageFetcher> = Arc::new(RoutingPageFetcher::new(
        default_fetcher(),
        browser_pages.clone(),
        &["modland.net"],
    ));
    let fetcher = Arc::new(PoliteFetcher::new(pages.clone(), PoliteOptions::default()));
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
        previews: {
            let cache = Arc::new(bmm_sources::images::RemoteImageCache::with_fallback(
                paths.search_previews(),
                pages,
            ));
            let trimmer = cache.clone();
            // Кеш картинок пополняется во время работы: подчищаем его не только при запуске,
            // иначе за долгую сессию папка растёт без предела.
            std::thread::spawn(move || {
                loop {
                    trimmer.trim(bmm_sources::images::DEFAULT_LIMIT_BYTES);
                    std::thread::sleep(std::time::Duration::from_secs(30 * 60));
                }
            });
            cache
        },
        sources: source_list.clone(),
    }));
    let update = Some(Arc::new(UpdateRuntime {
        coordinator: Arc::new(coordinator),
        enabled: enabled_sources,
        sources: source_list,
        page_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        page_busy: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }));
    let app_update = bmm_app_release::UpdateDownloader::reqwest(concat!(
        "ModManagerBeamNG/",
        env!("CARGO_PKG_VERSION")
    ))
        .ok()
        .map(|downloader| {
            Arc::new(Mutex::new(AppUpdateRuntime {
                downloader,
                state: bmm_app_release::UpdateState::idle(),
                prepared: None,
                cancel: bmm_app_release::CancelToken::new(),
                update_dir: paths.update_dir(),
                // Там же, где у 0.3.x: папку обновления чистят после каждого обновления.
                skipped_path: paths.root().join("update-skip.txt"),
                manifest_url: "https://github.com/SnikS1337/beamng-mod-manager/releases/latest/download/latest.json"
                    .parse()
                    .expect("статический адрес обновлений"),
            }))
        });

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
        app_update,
        search_cancel: Mutex::new(None),
        last_search: Mutex::new(None),
    });
    state.load_pending();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                // Служебное окно браузера не должно всплывать на экране при следующем запуске.
                .with_denylist(&["modland-pages"])
                .build(),
        )
        .manage(state)
        .on_page_load(|webview, payload| {
            if webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Finished
            {
                let update_dir = webview.state::<Arc<AppState>>().paths.update_dir();
                static HEALTHY: std::sync::Once = std::sync::Once::new();
                HEALTHY.call_once(|| commands::app_update::on_healthy(update_dir));
            }
        })
        .register_uri_scheme_protocol("bmm", |context, request| {
            preview_protocol(context.app_handle(), request)
        })
        .setup(move |app| {
            browser_pages.attach(app.handle().clone());
            let state = app.state::<Arc<AppState>>().inner().clone();
            commands::downloads::spawn(app.handle().clone(), state.clone());
            commands::updates::spawn_auto_check(app.handle().clone(), state);
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
            commands::library::library_load,
            commands::library::mod_toggle,
            commands::library::mod_details,
            commands::library::mod_preview,
            commands::library::mod_fix_packaging,
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
            commands::search::search_page_preview,
            commands::search::search_open_modland,
            commands::updates::updates_snapshot,
            commands::updates::updates_start,
            commands::updates::updates_retry_source,
            commands::updates::updates_check_mod,
            commands::updates::updates_decline_mod,
            commands::updates::updates_undo_decline,
            commands::updates::updates_resume_mod,
            commands::updates::updates_cancel,
            commands::updates::updates_dismiss,
            commands::updates::updates_restore,
            commands::updates::updates_find_pages,
            commands::updates::updates_confirm_page,
            commands::updates::updates_reject_page,
            commands::updates::updates_undo_page,
            commands::updates::updates_unfollow,
            commands::updates::updates_forget_lost,
            commands::updates::updates_refollow,
            commands::updates::mod_bind_page,
            commands::downloads::downloads_expect,
            commands::updates::updates_open_modland,
            commands::settings::settings_load,
            commands::settings::settings_save,
            commands::app_update::app_update_state,
            commands::app_update::app_update_check,
            commands::app_update::app_update_download,
            commands::app_update::app_update_cancel,
            commands::app_update::app_update_restart
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| match event {
            // Скрытые окна (страницы Modland, проверка сайта) держат процесс живым:
            // без этого после закрытия главного окна программа остаётся висеть,
            // а новый запуск упирается в «уже запущен».
            tauri::RunEvent::WindowEvent {
                label,
                event: tauri::WindowEvent::Destroyed,
                ..
            } => {
                let open: Vec<String> = app.webview_windows().into_keys().collect();
                for other in windows_to_close(&label, &open) {
                    if let Some(window) = app.get_webview_window(&other) {
                        let _ = window.destroy();
                    }
                }
            }
            // Фоновая проверка пишет библиотеку пакетами раз в 20 секунд: при закрытии
            // программы дописываем то, что накопилось, иначе найденное пропадёт.
            tauri::RunEvent::Exit => {
                app.state::<Arc<AppState>>().library.flush();
            }
            _ => {}
        });
}

/// Главное окно закрыли — закрываем и все служебные, чтобы процесс завершился.
fn windows_to_close(closed: &str, open: &[String]) -> Vec<String> {
    if closed != MAIN_WINDOW {
        return Vec::new();
    }
    open.iter()
        .filter(|label| label.as_str() != MAIN_WINDOW)
        .cloned()
        .collect()
}

const MAIN_WINDOW: &str = "main";

/// Имя файла кеша приходит из адреса `bmm.localhost`. Пропускаем только ключи,
/// которые выдаёт программа: без разделителей, двоеточий и переходов вверх.
fn safe_cache_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_'))
        && !name.contains("..")
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
            if !safe_cache_name(name) {
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
    if !safe_cache_name(name) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closing_main_window_closes_hidden_ones() {
        let open = vec!["modland-pages".to_string(), "modland-check".to_string()];
        assert_eq!(windows_to_close("main", &open), open);
        let with_main = vec!["main".to_string(), "modland-pages".to_string()];
        assert_eq!(
            windows_to_close("main", &with_main),
            vec!["modland-pages".to_string()]
        );
    }

    #[test]
    fn closing_a_service_window_keeps_the_rest() {
        let open = vec!["main".to_string(), "modland-pages".to_string()];
        assert!(windows_to_close("modland-check", &open).is_empty());
    }

    #[test]
    fn preview_names_cannot_escape_the_cache_folder() {
        assert!(safe_cache_name("0123456789ab.jpg"));
        assert!(safe_cache_name("0123456789ab.webp"));
        for name in [
            "",
            "..",
            "../library.json",
            "..\\library.json",
            "C:library.json",
            "/library.json",
            "a/b.jpg",
            "a\\b.jpg",
            "a%2fb.jpg",
            "a\0.jpg",
        ] {
            assert!(!safe_cache_name(name), "пропущено: {name:?}");
        }
    }
}

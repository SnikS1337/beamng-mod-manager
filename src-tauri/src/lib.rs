mod commands;
mod error;
mod state;

use bmm_domain::paths::AppPaths;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
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

    let state = AppState { paths };

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(state)
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
            commands::dev::open_modland_probe
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

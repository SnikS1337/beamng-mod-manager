use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Окно Modland для ручной проверки Cloudflare: запуск с `--probe`.
pub async fn open_modland_probe(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("modland-probe") {
        window.set_focus().map_err(|error| error.to_string())?;
        return Ok(());
    }

    let url = "https://www.modland.net/beamng-drive-mods.html"
        .parse()
        .map_err(|error| format!("{error:?}"))?;

    WebviewWindowBuilder::new(&app, "modland-probe", WebviewUrl::External(url))
        .title("Modland — проверка Cloudflare (PoC)")
        .inner_size(1100.0, 800.0)
        .build()
        .map_err(|error| error.to_string())?;

    Ok(())
}

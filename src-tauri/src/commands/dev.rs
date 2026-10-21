use serde::Serialize;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

const TITLES: [&str; 12] = [
    "Gavril D-Series",
    "Ibishu Covet",
    "ETK 800-Series",
    "Bruckell Bastion",
    "Cherrier FCV",
    "Soliad Wendover",
    "Gavril Roamer",
    "Ibishu Pessima",
    "Hirochi Sunburst",
    "Autobello Piccolina",
    "Wentward DT40L",
    "Civetta Bolide",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModEntry {
    id: usize,
    title: String,
    file: String,
    version: String,
    enabled: bool,
    update_available: bool,
    source: &'static str,
    size_mb: f64,
}

#[tauri::command]
pub fn dev_library(count: usize) -> Vec<ModEntry> {
    let count = count.clamp(1, 50_000);

    (0..count)
        .map(|i| {
            let base = TITLES[i % TITLES.len()];
            let round = i / TITLES.len();
            let title = if round == 0 {
                base.to_string()
            } else {
                format!("{base} · вариант {}", round + 1)
            };

            ModEntry {
                id: i,
                title,
                file: format!("bmm_mod_{i:05}.zip"),
                version: format!("1.{}.{}", i % 9, i % 7),
                enabled: i % 3 != 0,
                update_available: i % 11 == 0,
                source: ["repo", "modland", "stmods", "modsgaming"][i % 4],
                size_mb: ((3.0 + (i % 97) as f64 * 1.7) * 10.0).round() / 10.0,
            }
        })
        .collect()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvInfo {
    app_version: String,
    os: &'static str,
    arch: &'static str,
    backend: &'static str,
}

#[tauri::command]
pub fn dev_environment(app: tauri::AppHandle) -> EnvInfo {
    EnvInfo {
        app_version: app.package_info().version.to_string(),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        backend: "tauri",
    }
}

#[tauri::command]
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

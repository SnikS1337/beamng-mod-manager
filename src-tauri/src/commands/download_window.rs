//! Окно загрузки обновления: слева страница мода/файлообменника,
//! справа — панель с этапами. Оба вида — child webview одного нативного окна.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::webview::{DownloadEvent, NewWindowResponse};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl,
};
use url::Url;

use crate::error::AppError;

const WINDOW_LABEL: &str = "download";
const PANEL_LABEL: &str = "download-panel";
const SITE_LABEL: &str = "download-site";
const SITE_X: f64 = 24.0;
const SITE_Y: f64 = 116.0;
/// Слева и справа от страницы: отступ, правая панель и ещё один отступ.
const SITE_SIDE_SPACE: f64 = 448.0;
/// Сверху (шапка и адрес) и снизу (подпись) от страницы.
const SITE_VERTICAL_SPACE: f64 = 176.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub mod_name: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub page_url: String,
    pub preview_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadPanelEvent {
    step: u8,
    title: String,
    detail: String,
    error: Option<String>,
}

static CONTEXT: Mutex<Option<DownloadRequest>> = Mutex::new(None);

#[tauri::command]
pub fn download_panel_state() -> Option<DownloadRequest> {
    CONTEXT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

#[tauri::command]
pub fn download_close(app: AppHandle) {
    if let Some(window) = app.get_window(WINDOW_LABEL) {
        let _ = window.close();
    }
}

#[tauri::command]
pub async fn download_open(app: AppHandle, request: DownloadRequest) -> Result<(), AppError> {
    let url = Url::parse(&request.page_url)
        .map_err(|_| AppError::new("url", "Некорректный адрес страницы мода."))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AppError::new(
            "url",
            "Открывать можно только страницы http и https.",
        ));
    }
    *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = Some(request.clone());
    let settings = app
        .state::<std::sync::Arc<crate::state::AppState>>()
        .settings
        .load()
        .settings;
    let app_root = app
        .state::<std::sync::Arc<crate::state::AppState>>()
        .paths
        .root()
        .to_path_buf();
    let downloads = crate::commands::downloads::download_folder(&settings, &app_root)
        .ok_or_else(|| AppError::new("downloads", "Не нашли папку, куда сохранить архив."))?;

    if let Some(window) = app.get_window(WINDOW_LABEL) {
        if let Some(site) = app.get_webview(SITE_LABEL) {
            let _ = site.navigate(url);
        }
        let _ = window.show();
        let _ = window.set_focus();
        let _ = app.emit_to(PANEL_LABEL, "download://context", request);
        return Ok(());
    }

    let (window_width, window_height) = window_size(&app);
    let window = tauri::Window::builder(&app, WINDOW_LABEL)
        .title("Скачивание")
        .inner_size(window_width, window_height)
        .resizable(false)
        .decorations(false)
        .visible(false)
        .center()
        .build()
        .map_err(|error| AppError::new("window", error.to_string()))?;

    let panel = window
        .add_child(
            WebviewBuilder::new(PANEL_LABEL, panel_url(&app)),
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(window_width, window_height),
        )
        .map_err(|error| AppError::new("panel", error.to_string()))?;
    let download_app = app.clone();
    let site_builder = WebviewBuilder::new(SITE_LABEL, WebviewUrl::External(url))
        .on_download(move |_webview, event| {
            match event {
                DownloadEvent::Requested { url, destination } => {
                    if !is_download_url(&url) {
                        return false;
                    }
                    let name = download_name(&url, destination);
                    if !is_archive_name(Path::new(&name)) {
                        // Картинки, установщики и прочее пусть качает сам WebView2.
                        return true;
                    }
                    let path = unique_path(&downloads, &name);
                    if std::fs::create_dir_all(&downloads).is_err() {
                        emit_state(
                            &download_app,
                            2,
                            "Не удалось начать загрузку",
                            "Не получилось создать папку загрузки.",
                            Some("Проверьте доступ к папке данных программы.".to_string()),
                        );
                        return false;
                    }
                    crate::commands::downloads::note_caught(&path);
                    *destination = path;
                    emit_state(
                        &download_app,
                        2,
                        "Файл скачивается",
                        "Дождитесь окончания загрузки — менеджер заберёт файл сам.",
                        None,
                    );
                }
                DownloadEvent::Finished { path, success, .. } => {
                    if success {
                        if let Some(path) = path
                            && path.starts_with(&downloads)
                            && is_archive_name(&path)
                        {
                            emit_state(
                                &download_app,
                                3,
                                "Проверяем архив",
                                "Смотрим структуру, опасные пути и подозрительные файлы.",
                                None,
                            );
                            let app = download_app.clone();
                            std::thread::spawn(move || {
                                let payload =
                                    serde_json::json!({ "path": path.display().to_string() });
                                let _ = app.emit("incoming://download", payload);
                                emit_state(
                                    &app,
                                    4,
                                    "Установка",
                                    "Продолжите в главном окне — прошлая версия останется в копиях.",
                                    None,
                                );
                                if let Some(main) = app.get_window("main") {
                                    let _ = main.set_focus();
                                }
                            });
                        }
                    } else {
                        emit_state(
                            &download_app,
                            2,
                            "Загрузка не удалась",
                            "Файл не скачался. Попробуйте ещё раз или откройте страницу в браузере.",
                            Some("Сайт не отдал файл.".to_string()),
                        );
                    }
                }
                _ => {}
            }
            true
        })
        .on_new_window({
            let app = app.clone();
            move |url, _features| {
                if is_download_url(&url) {
                    let same_site = app
                        .get_webview(SITE_LABEL)
                        .and_then(|site| site.url().ok())
                        .and_then(|current| current.host_str().map(str::to_string))
                        .is_some_and(|host| {
                            url.host_str()
                                .is_some_and(|next| next.eq_ignore_ascii_case(&host))
                        });
                    let file_host = bmm_domain::hosts::host_matches(
                        &url,
                        bmm_domain::hosts::KNOWN_FILE_HOSTS,
                    );
                    if (same_site || file_host)
                        && let Some(site) = app.get_webview(SITE_LABEL)
                    {
                        let _ = site.navigate(url);
                    }
                }
                NewWindowResponse::Deny
            }
        });
    let site_width = (window_width - SITE_SIDE_SPACE).max(520.0);
    let site_height = (window_height - SITE_VERTICAL_SPACE).max(420.0);
    let site = window
        .add_child(
            site_builder,
            LogicalPosition::new(SITE_X, SITE_Y),
            LogicalSize::new(site_width, site_height),
        )
        .map_err(|error| AppError::new("site", error.to_string()))?;

    window
        .show()
        .map_err(|error| AppError::new("window", error.to_string()))?;
    site.set_focus()
        .map_err(|error| AppError::new("site", error.to_string()))?;
    let _ = panel;
    Ok(())
}

/// Окно не должно вылезать за рабочую область даже на маленьком экране.
fn window_size(app: &AppHandle) -> (f64, f64) {
    let (width, height) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            let scale = monitor.scale_factor();
            let work = monitor.work_area().size;
            (work.width as f64 / scale, work.height as f64 / scale)
        })
        .unwrap_or((1240.0, 820.0));
    (
        (width.min(1240.0) - 32.0).max(1000.0),
        (height.min(820.0) - 32.0).max(680.0),
    )
}

/// В dev панель — маршрут Vite (`/download`), в собранной программе — готовый asset.
fn panel_url(app: &AppHandle) -> WebviewUrl {
    if cfg!(debug_assertions)
        && let Some(dev) = &app.config().build.dev_url
        && let Ok(url) = dev.join("download")
    {
        return WebviewUrl::External(url);
    }
    WebviewUrl::App("download.html".into())
}

/// `blob:` и `data:` тоже бывают загрузками, хотя в них нет обычного адреса файла.
fn is_download_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https" | "blob" | "data")
}

fn is_archive_name(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "zip" | "rar" | "7z"
            )
        })
}

fn emit_state(app: &AppHandle, step: u8, title: &str, detail: &str, error: Option<String>) {
    let _ = app.emit_to(
        PANEL_LABEL,
        "download://state",
        DownloadPanelEvent {
            step,
            title: title.to_string(),
            detail: detail.to_string(),
            error,
        },
    );
}

fn download_name(url: &Url, destination: &Path) -> String {
    let from_destination = destination
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string);
    let from_url = url
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .map(str::to_string);
    // Иногда WebView2 предлагает имя без расширения, а настоящий ZIP виден в адресе.
    let raw = match (from_destination, from_url) {
        (Some(destination), Some(url_name))
            if !is_archive_name(Path::new(&destination))
                && is_archive_name(Path::new(&url_name)) =>
        {
            url_name
        }
        (Some(destination), _) => destination,
        (None, Some(url_name)) => url_name,
        (None, None) => "mod.zip".to_string(),
    };
    let raw = raw.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let safe: String = raw
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(character, '<' | '>' | ':' | '"' | '|' | '?' | '*')
            {
                '_'
            } else {
                character
            }
        })
        .collect();
    let safe = safe.trim().trim_matches('.').to_string();
    if safe.is_empty() {
        "mod.zip".to_string()
    } else {
        safe.chars().take(160).collect()
    }
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("mod");
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    for index in 1..1000 {
        let next = dir.join(format!("{stem} ({index}){extension}"));
        if !next.exists() {
            return next;
        }
    }
    dir.join(format!("{stem}-{}{extension}", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_name_stays_a_plain_file_name() {
        let url = Url::parse("https://example.com/files/mod.zip").unwrap();
        assert_eq!(
            download_name(&url, Path::new(r"C:\Downloads\bad:name?.zip")),
            "bad_name_.zip"
        );
        assert_eq!(download_name(&url, Path::new("")), "mod.zip");
        assert_eq!(
            download_name(&url, Path::new(r"..\..\evil.zip")),
            "evil.zip"
        );
        assert_eq!(
            download_name(&url, Path::new(r"C:\Downloads\download")),
            "mod.zip",
            "расширение берём из адреса"
        );
    }

    #[test]
    fn only_archives_are_caught() {
        assert!(is_archive_name(Path::new("mod.zip")));
        assert!(is_archive_name(Path::new("mod.RAR")));
        assert!(is_archive_name(Path::new("mod.7z")));
        assert!(!is_archive_name(Path::new("setup.exe")));
        assert!(!is_archive_name(Path::new("image.png")));
    }

    #[test]
    fn duplicate_download_gets_a_number() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("mod.zip"), b"old").unwrap();
        assert_eq!(
            unique_path(temp.path(), "mod.zip"),
            temp.path().join("mod (1).zip")
        );
    }

    #[test]
    fn browser_download_schemes_are_allowed() {
        for raw in [
            "https://example.com/mod.zip",
            "http://example.com/mod.zip",
            "blob:https://example.com/id",
            "data:application/zip;base64,AAAA",
        ] {
            assert!(is_download_url(&Url::parse(raw).unwrap()), "{raw}");
        }
        assert!(!is_download_url(
            &Url::parse("file:///C:/Windows/System32/calc.exe").unwrap()
        ));
    }
}

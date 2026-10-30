//! Окно загрузки обновления: слева страница мода/файлообменника,
//! справа — панель с этапами. Оба вида — child webview одного нативного окна.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use tauri::webview::{DownloadEvent, NewWindowResponse};
use tauri::{
    AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, WebviewBuilder, WebviewUrl,
};
use ts_rs::TS;
use url::Url;

use crate::error::AppError;

const WINDOW_LABEL: &str = "download";
const PANEL_LABEL: &str = "download-panel";
const SITE_LABEL: &str = "download-site";
const SITE_X: f64 = 24.0;
const SITE_Y: f64 = 116.0;
/// Слева и справа от страницы: отступ, правая панель и ещё один отступ.
const SITE_SIDE_SPACE: f64 = 448.0;
/// На узком окне панель уже, иначе странице не остаётся места.
const SITE_SIDE_SPACE_NARROW: f64 = 360.0;
/// Сверху (шапка и адрес) и снизу (подпись) от страницы.
const SITE_VERTICAL_SPACE: f64 = 176.0;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub mod_name: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub page_url: String,
    pub preview_url: Option<String>,
    #[serde(default)]
    #[ts(type = "ModRole | null")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadPanelEvent {
    step: u8,
    title: String,
    detail: String,
    error: Option<String>,
}

static CONTEXT: Mutex<Option<DownloadRequest>> = Mutex::new(None);
static READY: Mutex<Option<String>> = Mutex::new(None);
static INTERCEPTED: LazyLock<Mutex<HashSet<PathBuf>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn mark_intercepted(path: &Path) {
    let mut paths = INTERCEPTED
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    if paths.len() >= 128 {
        paths.clear();
    }
    paths.insert(path.to_path_buf());
}

fn take_intercepted(path: &Path) -> bool {
    INTERCEPTED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(path)
}

#[tauri::command]
pub fn download_panel_state() -> Option<DownloadRequest> {
    CONTEXT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

/// Готовый архив для панели: на случай, если событие пришло до её загрузки.
#[tauri::command]
pub fn download_ready() -> Option<String> {
    READY
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

fn clear_ready() {
    *READY.lock().unwrap_or_else(|error| error.into_inner()) = None;
}

/// Сессия окна закончилась: панель, контекст и метки перехвата больше не нужны.
/// Незаконченная загрузка после этого снова видна сторожу «Загрузок».
fn clear_session() {
    clear_ready();
    *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = None;
    INTERCEPTED
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clear();
    crate::commands::downloads::forget_caught();
}

#[tauri::command]
pub fn download_close(app: AppHandle) {
    clear_session();
    if let Some(window) = app.get_window(WINDOW_LABEL) {
        let _ = window.close();
    }
}

#[tauri::command]
pub async fn download_open(app: AppHandle, request: DownloadRequest) -> Result<(), AppError> {
    let url = Url::parse(&request.page_url)
        .map_err(|_| AppError::new("url", "Некорректный адрес страницы мода."))?;
    if !page_url_allowed(&url) {
        return Err(AppError::new(
            "url",
            "Открывать можно только внешние страницы http и https.",
        ));
    }
    *CONTEXT.lock().unwrap_or_else(|error| error.into_inner()) = Some(request.clone());
    clear_ready();
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
    let (mods_dir, _) = crate::commands::library::resolve_folder(&settings);
    let downloads =
        crate::commands::downloads::download_folder(&settings, &app_root, mods_dir.as_deref())
            .ok_or_else(|| AppError::new("downloads", "Не нашли папку, куда сохранить архив."))?;
    crate::commands::downloads::expect_folder(&downloads);

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
    // Окно могли закрыть не кнопкой панели (Alt+F4, выход из программы):
    // метки перехвата не должны пережить сессию окна.
    window.on_window_event(|event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            clear_session();
        }
    });

    let panel = match window.add_child(
        WebviewBuilder::new(PANEL_LABEL, panel_url(&app)),
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(window_width, window_height),
    ) {
        Ok(panel) => panel,
        Err(error) => {
            // Окно уже зарегистрировано: без уборки повтор откроет пустую панель.
            let _ = window.close();
            return Err(AppError::new("panel", error.to_string()));
        }
    };
    let download_app = app.clone();
    let site_builder = WebviewBuilder::new(SITE_LABEL, WebviewUrl::External(url))
        .initialization_script(SITE_HELPERS)
        // Первый адрес проверен выше; каждый следующий переход и редирект тоже
        // не должен уводить в localhost/частную сеть.
        .on_navigation(|url| {
            url.scheme() == "about" || bmm_sources::net::redirect_allowed(url)
        })
        .on_download(move |_webview, event| {
            match event {
                DownloadEvent::Requested { url, destination } => {
                    let name = download_name(&url, destination);
                    if !accepts_download(&url, &name) {
                        // Окно — только для архивов модов. Установщик или другой файл
                        // с той же страницы молча лёг бы в «Загрузки»; не качаем его.
                        emit_state(
                            &download_app,
                            1,
                            "Это не архив мода",
                            &format!("Сайт предложил скачать «{name}» — такой файл менеджер не берёт."),
                            Some("Нужна кнопка, которая скачивает zip, rar или 7z.".to_string()),
                        );
                        return false;
                    }
                    let state = download_app.state::<std::sync::Arc<crate::state::AppState>>();
                    let settings = state.settings.load().settings;
                    let app_root = state.paths.root().to_path_buf();
                    let (mods_dir, _) = crate::commands::library::resolve_folder(&settings);
                    let Some(downloads) = crate::commands::downloads::download_folder(
                        &settings,
                        &app_root,
                        mods_dir.as_deref(),
                    ) else {
                        emit_state(
                            &download_app,
                            2,
                            "Не удалось начать загрузку",
                            "Не нашли папку, куда сохранить архив.",
                            Some("Проверьте доступ к папке загрузки.".to_string()),
                        );
                        return false;
                    };
                    crate::commands::downloads::expect_folder(&downloads);
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
                    mark_intercepted(&path);
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
                    let intercepted = path.as_deref().is_some_and(take_intercepted);
                    if success {
                        if let Some(path) = path
                            && intercepted
                            && is_archive_name(&path)
                        {
                            let path_text = path.display().to_string();
                            *READY.lock().unwrap_or_else(|error| error.into_inner()) =
                                Some(path_text.clone());
                            emit_state(
                                &download_app,
                                3,
                                "Проверяем архив",
                                "Смотрим структуру, опасные пути и подозрительные файлы.",
                                None,
                            );
                            let _ = download_app.emit_to(
                                PANEL_LABEL,
                                "download://ready",
                                serde_json::json!({ "path": path_text }),
                            );
                        }
                    } else {
                        clear_ready();
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
    let site_width = (window_width - side_space(window_width)).max(320.0);
    let site_height = (window_height - SITE_VERTICAL_SPACE).max(300.0);
    let site = match window.add_child(
        site_builder,
        LogicalPosition::new(SITE_X, SITE_Y),
        LogicalSize::new(site_width, site_height),
    ) {
        Ok(site) => site,
        Err(error) => {
            let _ = window.close();
            return Err(AppError::new("site", error.to_string()));
        }
    };

    if let Err(error) = window.show() {
        let _ = window.close();
        return Err(AppError::new("window", error.to_string()));
    }
    if let Err(error) = site.set_focus() {
        let _ = window.close();
        return Err(AppError::new("site", error.to_string()));
    }
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
    let width = (width - 32.0).clamp(760.0, 1240.0).min(width - 16.0);
    let height = (height - 32.0).clamp(560.0, 820.0).min(height - 16.0);
    (width.max(480.0), height.max(420.0))
}

fn side_space(window_width: f64) -> f64 {
    if window_width < 1000.0 {
        SITE_SIDE_SPACE_NARROW
    } else {
        SITE_SIDE_SPACE
    }
}

/// В dev панель — маршрут Vite (`/download`), в собранной программе — готовый asset.
fn panel_url(app: &AppHandle) -> WebviewUrl {
    // Адрес дев-сервера — только под `tauri dev`. Отладочная сборка
    // (`tauri build --debug`) идёт без сервера: по cfg!(debug_assertions) панель
    // загрузки открывалась пустой.
    if tauri::is_dev()
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

/// Скачивать из окна можно только архив мода по обычной ссылке браузера.
fn accepts_download(url: &Url, name: &str) -> bool {
    is_download_url(url) && is_archive_name(Path::new(name))
}

/// Страницу мода открываем только во внешнем браузере WebView2: localhost,
/// частные адреса и служебные схемы не должны попадать в него из выдачи сайта.
fn page_url_allowed(url: &Url) -> bool {
    bmm_sources::net::redirect_allowed(url)
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

/// Ссылки «скачать» на страницах модов часто открываются в новой вкладке. Встроенному
/// окну нужна та же вкладка, иначе WebView2 теряет загрузку; рекламу и чужие popup не трогаем.
const SITE_HELPERS: &str = r#"
(() => {
  const downloadPath = /\/download_file\.php|\/download(?:[/?]|$)|\/d\/[a-z0-9]+/i;
  document.addEventListener("click", (event) => {
    const anchor = event.target && event.target.closest
      ? event.target.closest("a[target=_blank]")
      : null;
    if (!anchor) return;
    let next;
    try { next = new URL(anchor.href, location.href); } catch { return; }
    if (next.host !== location.host) return;
    if (!downloadPath.test(next.pathname + next.search)) return;
    event.preventDefault();
    location.href = next.href;
  }, true);
})();
"#;

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
        avoid_reserved(preserve_extension(&safe, 160))
    }
}

/// Обрезка не должна съедать расширение: иначе длинное имя перестаёт считаться
/// архивом и WebView2 кладёт файл мимо перехвата.
fn preserve_extension(name: &str, max: usize) -> String {
    if name.chars().count() <= max {
        return name.to_string();
    }
    let path = Path::new(name);
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!(".{extension}"))
        .unwrap_or_default();
    if extension.is_empty() {
        return name.chars().take(max).collect();
    }
    let stem_max = max.saturating_sub(extension.chars().count());
    let stem: String = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("mod")
        .chars()
        .take(stem_max)
        .collect();
    if stem.is_empty() {
        format!("mod{extension}")
    } else {
        format!("{stem}{extension}")
    }
}

fn avoid_reserved(name: String) -> String {
    let stem = Path::new(&name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|reserved| stem.eq_ignore_ascii_case(reserved))
    {
        format!("_{name}")
    } else {
        name
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
    fn long_download_name_keeps_archive_extension() {
        let url = Url::parse("https://example.com/files/mod.zip").unwrap();
        let long = format!("{}.zip", "a".repeat(300));
        let name = download_name(&url, Path::new(&long));
        assert!(name.ends_with(".zip"), "{name}");
        assert!(name.chars().count() <= 160, "{name}");
        assert_eq!(
            download_name(&url, Path::new("CON.zip")),
            "_CON.zip",
            "зарезервированное имя Windows"
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
    fn only_mod_archives_are_downloaded_from_the_window() {
        let page = Url::parse("https://modsfire.com/d/abc").unwrap();
        assert!(accepts_download(&page, "BMW_M2_v1.5.zip"));
        assert!(accepts_download(&page, "map.7z"));
        // Установщики, картинки, документы и архив без расширения — нет.
        for name in [
            "setup.exe",
            "update.msi",
            "preview.png",
            "readme.pdf",
            "mod",
        ] {
            assert!(!accepts_download(&page, name), "{name}");
        }
        let local = Url::parse("file:///C:/mods/mod.zip").unwrap();
        assert!(!accepts_download(&local, "mod.zip"));
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

    #[test]
    fn page_urls_must_be_public_http() {
        for raw in [
            "https://stmods.org/beamng_drive/mods/example/",
            "http://example.com/page",
        ] {
            assert!(page_url_allowed(&Url::parse(raw).unwrap()), "{raw}");
        }
        for raw in [
            "http://127.0.0.1/service",
            "http://localhost/service",
            "http://192.168.1.10/router",
            "http://[::1]/service",
            "file:///C:/Windows/System32/calc.exe",
            "javascript:alert(1)",
        ] {
            assert!(!page_url_allowed(&Url::parse(raw).unwrap()), "{raw}");
        }
    }
}

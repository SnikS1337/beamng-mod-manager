use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use bmm_domain::library::LibraryStore;
use bmm_packs::incoming::{
    self, IncomingAnalysis, IncomingItem, IncomingKind, IncomingOptions, InstalledArchive,
};
use bmm_packs::scan;
use bmm_packs::zip_index;

use crate::commands::library::resolve_folder;
use crate::error::AppError;
use crate::state::{AppState, CachedContent};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingFindingDto {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub entry: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingItemDto {
    pub zip_path: String,
    pub file_name: String,
    pub size: u64,
    pub title: String,
    pub kind: String,
    pub replaces: Option<String>,
    pub replaces_enabled: bool,
    pub replaces_sub_folder: Option<String>,
    pub locked: bool,
    pub repacked: bool,
    pub blocked: bool,
    pub warnings: bool,
    pub findings: Vec<IncomingFindingDto>,
    /// Похоже, такая машина уже стоит другим модом.
    pub similar_to: Option<SimilarModDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimilarModDto {
    pub file_name: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingArchiveDto {
    pub source: String,
    pub problem: Option<String>,
    pub work_dir: String,
    pub items: Vec<IncomingItemDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingInstallRequest {
    pub zip_path: String,
    pub file_name: String,
    pub replaces: Option<String>,
    pub allow_warnings: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingInstallResult {
    pub success: bool,
    pub message: Option<String>,
    pub file_name: String,
    pub replaces: Option<String>,
    pub backup_id: Option<String>,
    pub was_disabled: bool,
    pub same_name: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingTicketDto {
    pub file_name: String,
    pub replaces: Option<String>,
    pub backup_id: Option<String>,
    pub was_disabled: bool,
    pub same_name: bool,
}

#[tauri::command]
pub async fn incoming_inspect(
    state: tauri::State<'_, Arc<AppState>>,
    paths: Vec<String>,
) -> Result<Vec<IncomingArchiveDto>, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || inspect_paths(&state, &paths))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn incoming_install(
    state: tauri::State<'_, Arc<AppState>>,
    requests: Vec<IncomingInstallRequest>,
) -> Result<Vec<IncomingInstallResult>, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || install_items(&state, &requests))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn incoming_undo(
    state: tauri::State<'_, Arc<AppState>>,
    ticket: IncomingTicketDto,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || undo_install(&state, &ticket))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

/// Удалить скачанный архив после успешной установки. Только если это включено
/// в настройках и файл действительно лежит в одной из папок загрузок.
#[tauri::command]
pub async fn incoming_remove_source(
    state: tauri::State<'_, Arc<AppState>>,
    path: String,
) -> Result<bool, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || remove_download_source(&state, &path))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

fn remove_download_source(state: &AppState, path: &str) -> Result<bool, AppError> {
    let settings = state.settings.load().settings;
    if !settings.delete_downloads_after_install {
        return Ok(false);
    }
    let path = PathBuf::from(path);
    if !is_download_source(&path, &settings, state.paths.root()) {
        return Ok(false);
    }
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::new("io", error.to_string())),
    }
}

fn is_download_source(
    path: &Path,
    settings: &bmm_domain::settings::AppSettings,
    app_root: &Path,
) -> bool {
    if !path.is_file()
        || !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "zip" | "rar" | "7z"
                )
            })
    {
        return false;
    }
    let Ok(path) = fs::canonicalize(path) else {
        return false;
    };
    let mut dirs = Vec::new();
    if let Some(folder) = settings.downloads_folder.as_deref() {
        dirs.push(PathBuf::from(folder));
    }
    if let Some(folder) = crate::commands::downloads::downloads_folder() {
        dirs.push(folder);
    }
    dirs.push(app_root.join("downloads"));
    dirs.extend(settings.watch_folders.iter().map(PathBuf::from));
    dirs.iter()
        .filter_map(|folder| fs::canonicalize(folder).ok())
        .any(|folder| path.starts_with(folder))
}

#[tauri::command]
pub async fn incoming_discard(work_dir: String) -> Result<(), AppError> {
    let root = incoming_root();
    let path = PathBuf::from(&work_dir);
    let safe = path.starts_with(&root)
        && path != root
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir));
    if !safe {
        return Err(AppError::new("path", "Недопустимая временная папка."));
    }
    let _ = fs::remove_dir_all(path);
    Ok(())
}

fn inspect_paths(state: &AppState, paths: &[String]) -> Result<Vec<IncomingArchiveDto>, AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let settings = state.settings.load().settings;
    let (mods_dir, error) = resolve_folder(&settings);
    let Some(mods_dir) = mods_dir else {
        return Err(AppError::new(
            "folder",
            error.unwrap_or_else(|| "Папка модов не найдена.".to_string()),
        ));
    };
    let installer = bmm_packs::install::ModInstaller::new(
        bmm_packs::scan::ModLayout::in_app_folder(&mods_dir, state.paths.root()),
        bmm_domain::clock::system_clock(),
        Arc::new(|| 3),
    );
    let installed = installed_archives(state, &installer);
    let root = incoming_root();
    let _ = fs::create_dir_all(&root);
    cleanup_incoming(&root);
    let cancel = AtomicBool::new(false);
    let mut result = Vec::new();
    for raw in paths.iter().take(8) {
        let source = PathBuf::from(raw);
        let work = root.join(format!("{}-{}", std::process::id(), unique_suffix()));
        let analysis = incoming::analyze(
            &source,
            &installed,
            &work,
            IncomingOptions::default(),
            &cancel,
        );
        result.push(to_dto(analysis));
    }
    Ok(result)
}

fn installed_archives(
    state: &AppState,
    installer: &bmm_packs::install::ModInstaller,
) -> Vec<InstalledArchive> {
    let scan = scan::scan_mods(installer.layout());
    let titles = state.previews.titles();
    let mut result = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for disk in scan.mods {
        let path = match &disk.sub_folder {
            Some(folder) => installer
                .layout()
                .mods_dir
                .join(folder)
                .join(&disk.file_name),
            None if disk.enabled => installer.layout().mods_dir.join(&disk.file_name),
            None => installer.layout().disabled_dir.join(&disk.file_name),
        };
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        seen.insert(path.clone());
        let stamp = (metadata.len(), metadata.modified().ok());
        let content = {
            let mut cache = state.content_cache.lock().unwrap();
            match cache.get(&path) {
                Some(cached) if cached.len == stamp.0 && cached.modified == stamp.1 => {
                    cached.content.clone()
                }
                _ => {
                    let content = zip_index::read_index(&path)
                        .map(|index| incoming::content_keys(&index))
                        .unwrap_or_default();
                    cache.insert(
                        path.clone(),
                        CachedContent {
                            len: stamp.0,
                            modified: stamp.1,
                            content: content.clone(),
                        },
                    );
                    content
                }
            }
        };
        let title = titles
            .get(&bmm_packs::cache::PreviewCache::key(
                &disk.file_name,
                disk.size_bytes,
                disk.modified_utc,
            ))
            .filter(|title| !title.is_empty())
            .cloned();
        result.push(InstalledArchive {
            title,
            file_name: disk.file_name,
            enabled: disk.enabled,
            size: disk.size_bytes,
            path,
            content,
            sub_folder: disk.sub_folder,
        });
    }
    state
        .content_cache
        .lock()
        .unwrap()
        .retain(|path, _| seen.contains(path));
    result
}

fn cleanup_incoming(root: &Path) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.filter_map(Result::ok) {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now
            .duration_since(modified)
            .map(|age| age.as_secs() > 24 * 60 * 60)
            .unwrap_or(false)
        {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn to_dto(analysis: IncomingAnalysis) -> IncomingArchiveDto {
    IncomingArchiveDto {
        source: analysis.source.display().to_string(),
        problem: analysis.problem,
        work_dir: analysis.work_dir.display().to_string(),
        items: analysis.items.into_iter().map(item_dto).collect(),
    }
}

fn item_dto(item: IncomingItem) -> IncomingItemDto {
    let locked = item.locked();
    IncomingItemDto {
        zip_path: item.zip_path.display().to_string(),
        file_name: item.file_name,
        size: item.size,
        title: item.title,
        kind: match item.kind {
            IncomingKind::New => "new",
            IncomingKind::Update => "update",
            IncomingKind::AlreadyInstalled => "alreadyInstalled",
        }
        .to_string(),
        replaces: item.replaces,
        replaces_enabled: item.replaces_enabled,
        replaces_sub_folder: item.replaces_sub_folder,
        locked,
        repacked: item.repacked,
        blocked: item.blocked,
        warnings: item.warnings,
        similar_to: item
            .similar_to
            .map(|(file_name, title)| SimilarModDto { file_name, title }),
        findings: item
            .findings
            .into_iter()
            .map(|finding| IncomingFindingDto {
                severity: match finding.severity {
                    bmm_packs::inspect::Severity::Info => "info",
                    bmm_packs::inspect::Severity::Warning => "warning",
                    bmm_packs::inspect::Severity::Block => "block",
                }
                .to_string(),
                code: finding.code.to_string(),
                message: finding.message,
                entry: finding.entry,
            })
            .collect(),
    }
}

fn install_items(
    state: &AppState,
    requests: &[IncomingInstallRequest],
) -> Result<Vec<IncomingInstallResult>, AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let settings = state.settings.load().settings;
    let (mods_dir, error) = resolve_folder(&settings);
    let Some(mods_dir) = mods_dir else {
        return Err(AppError::new(
            "folder",
            error.unwrap_or_else(|| "Папка модов не найдена.".to_string()),
        ));
    };
    let installer = bmm_packs::install::ModInstaller::new(
        bmm_packs::scan::ModLayout::in_app_folder(&mods_dir, state.paths.root()),
        bmm_domain::clock::system_clock(),
        Arc::new(move || settings.keep_backups.max(1) as usize),
    );
    let mut results = Vec::new();
    for request in requests {
        let source = PathBuf::from(&request.zip_path);
        let options = bmm_packs::install::InstallOptions {
            allow_warnings: request.allow_warnings,
            ..bmm_packs::install::InstallOptions::default()
        };
        if let Some(replaced) = &request.replaces {
            let update = installer.update_from_path(replaced, &source, &request.file_name, options);
            if update.install.success
                && !replaced.eq_ignore_ascii_case(&request.file_name)
                && let Some(path) = update.install.path.as_ref()
            {
                let size = fs::metadata(path)
                    .ok()
                    .and_then(|meta| i64::try_from(meta.len()).ok())
                    .unwrap_or(0);
                bmm_domain::library::rename_file(
                    state.library.as_ref(),
                    replaced,
                    &request.file_name,
                    size,
                );
            }
            let backup_id = if update.install.success {
                installer
                    .list_backups(replaced)
                    .first()
                    .map(|entry| entry.id.clone())
            } else {
                None
            };
            results.push(IncomingInstallResult {
                success: update.install.success,
                message: update.install.message,
                file_name: request.file_name.clone(),
                replaces: Some(replaced.clone()),
                backup_id,
                was_disabled: !update.was_enabled,
                same_name: replaced.eq_ignore_ascii_case(&request.file_name),
            });
        } else {
            let install = installer.install_from_path(&source, &request.file_name, options);
            results.push(IncomingInstallResult {
                success: install.success,
                message: install.message,
                file_name: request.file_name.clone(),
                replaces: None,
                backup_id: None,
                was_disabled: false,
                same_name: false,
            });
        }
    }
    Ok(results)
}

fn undo_install(state: &AppState, ticket: &IncomingTicketDto) -> Result<(), AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let settings = state.settings.load().settings;
    let (mods_dir, error) = resolve_folder(&settings);
    let Some(mods_dir) = mods_dir else {
        return Err(AppError::new(
            "folder",
            error.unwrap_or_else(|| "Папка модов не найдена.".to_string()),
        ));
    };
    let installer = bmm_packs::install::ModInstaller::new(
        bmm_packs::scan::ModLayout::in_app_folder(&mods_dir, state.paths.root()),
        bmm_domain::clock::system_clock(),
        Arc::new(|| 3),
    );
    if let Some(replaced) = &ticket.replaces {
        if !ticket.same_name {
            let _ = installer.remove(&ticket.file_name);
        }
        if let Some(backup_id) = &ticket.backup_id {
            let result = installer.restore_backup(replaced, backup_id);
            if !result.success {
                return Err(AppError::new("undo", result.message));
            }
            if ticket.was_disabled {
                let _ = installer.disable(replaced);
            }
            // Запись библиотеки возвращается на прежний файл вместе со страницей мода.
            if !ticket.same_name {
                let size = result
                    .path
                    .as_ref()
                    .and_then(|path| fs::metadata(path).ok())
                    .and_then(|meta| i64::try_from(meta.len()).ok())
                    .unwrap_or(0);
                bmm_domain::library::rename_file(
                    state.library.as_ref(),
                    &ticket.file_name,
                    replaced,
                    size,
                );
            }
        }
        Ok(())
    } else {
        let result = installer.remove(&ticket.file_name);
        if !result.success {
            return Err(AppError::new("undo", result.message));
        }
        if let Some(found) = state
            .library
            .mods()
            .into_iter()
            .find(|item| item.file_name.eq_ignore_ascii_case(&ticket.file_name))
        {
            state.library.remove_mod(&found.id);
        }
        Ok(())
    }
}

fn incoming_root() -> PathBuf {
    std::env::temp_dir().join("BeamModManager").join("incoming")
}

fn unique_suffix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::Mutex;

    use bmm_domain::clock::system_clock;
    use bmm_domain::library::JsonLibraryStore;
    use bmm_domain::paths::AppPaths;
    use bmm_domain::settings::AppSettings;
    use bmm_domain::settings_store::SettingsStore;
    use bmm_packs::cache::PreviewCache;
    use bmm_packs::install::{InstallOptions, ModInstaller};

    fn setup() -> (tempfile::TempDir, Arc<AppState>) {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let mods = temp.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        let paths = AppPaths::at(&data);
        paths.ensure_dirs().unwrap();
        let settings = SettingsStore::with_system_clock(paths.settings());
        settings
            .save(&AppSettings {
                mods_folder: Some(mods.display().to_string()),
                ..AppSettings::default()
            })
            .unwrap();
        let state = Arc::new(AppState {
            paths,
            settings,
            library: Arc::new(JsonLibraryStore::new(
                data.join("library.json"),
                system_clock(),
            )),
            previews: PreviewCache::new(data.join("previews")),
            load_lock: Mutex::new(()),
            previews_by_id: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
            deleted: Mutex::new(HashMap::new()),
            content_cache: Mutex::new(HashMap::new()),
            search: None,
            update: None,
            app_update: None,
            search_cancel: Mutex::new(None),
            last_search: Mutex::new(None),
        });
        (temp, state)
    }

    fn installer_for_test(state: &AppState) -> ModInstaller {
        let settings = state.settings.load().settings;
        let (mods_dir, error) = resolve_folder(&settings);
        let mods_dir = mods_dir.unwrap_or_else(|| panic!("{error:?}"));
        ModInstaller::new(
            scan::ModLayout::in_app_folder(&mods_dir, state.paths.root()),
            system_clock(),
            Arc::new(|| 3),
        )
    }

    fn make_zip(path: &std::path::Path, body: &[u8]) {
        let file = fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "vehicles/car/a.jbeam",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(body).unwrap();
        writer.finish().unwrap();
    }

    #[test]
    fn broken_and_missing_archives_are_reported_not_dropped() {
        let (temp, state) = setup();
        let broken = temp.path().join("broken.zip");
        fs::write(&broken, b"not a zip at all").unwrap();
        let missing = temp.path().join("missing.zip");
        let good = temp.path().join("good.zip");
        make_zip(&good, b"{}");
        let paths: Vec<String> = [&broken, &missing, &good]
            .iter()
            .map(|path| path.display().to_string())
            .collect();
        let result = inspect_paths(&state, &paths).unwrap();
        assert_eq!(result.len(), 3, "каждый архив получил ответ");
        // Битый архив — одна заблокированная строка с одной причиной, а не тишина.
        let broken_item = &result[0].items[0];
        assert!(broken_item.blocked);
        assert_eq!(broken_item.findings.len(), 1, "{:?}", broken_item.findings);
        assert!(result[1].problem.is_some(), "пропавший файл назван");
        assert!(result[2].problem.is_none(), "{:?}", result[2].problem);
        assert_eq!(result[2].items.len(), 1);
        for archive in &result {
            let _ = fs::remove_dir_all(&archive.work_dir);
        }
    }

    #[test]
    fn install_update_and_undo_keeps_disabled_state() {
        let (temp, state) = setup();
        let v1 = temp.path().join("v1.zip");
        let v2 = temp.path().join("v2.zip");
        make_zip(&v1, b"one");
        make_zip(&v2, b"two");
        let installer = installer_for_test(&state);
        assert!(
            installer
                .install_from_path(&v1, "car_v1.zip", InstallOptions::default())
                .success
        );
        assert!(installer.disable("car_v1.zip").success);

        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: v2.display().to_string(),
                file_name: "car_v2.zip".to_string(),
                replaces: Some("car_v1.zip".to_string()),
                allow_warnings: true,
            }],
        )
        .unwrap();
        assert!(results[0].success, "{:?}", results[0].message);
        assert!(results[0].was_disabled);
        let backup_id = results[0].backup_id.clone().expect("backup");
        assert!(installer.layout().disabled_dir.join("car_v2.zip").is_file());

        undo_install(
            &state,
            &IncomingTicketDto {
                file_name: "car_v2.zip".to_string(),
                replaces: Some("car_v1.zip".to_string()),
                backup_id: Some(backup_id),
                was_disabled: true,
                same_name: false,
            },
        )
        .unwrap();
        assert!(installer.layout().disabled_dir.join("car_v1.zip").is_file());
        assert!(!installer.layout().mods_dir.join("car_v1.zip").exists());
        assert!(!installer.layout().disabled_dir.join("car_v2.zip").exists());
    }

    #[test]
    fn install_window_and_rust_agree_on_fields_and_values() {
        use crate::test_support::{assert_matches_ts, ts_union};
        let (temp, state) = setup();
        let source = temp.path().join("new.zip");
        make_zip(&source, b"body");

        let archives = inspect_paths(&state, &[source.display().to_string()]).unwrap();
        let archive = serde_json::to_value(&archives[0]).unwrap();
        assert_matches_ts(&archive, "incoming.ts", "IncomingArchiveDto");
        let item = &archive["items"][0];
        assert_matches_ts(item, "incoming.ts", "IncomingItemDto");
        assert!(ts_union("incoming.ts", "IncomingKind").contains(item["kind"].as_str().unwrap()));
        for finding in item["findings"].as_array().unwrap() {
            assert_matches_ts(finding, "incoming.ts", "IncomingFindingDto");
        }
        let severities = ts_union("incoming.ts", "FindingSeverity");
        for severity in ["info", "warning", "block"] {
            assert!(severities.contains(severity));
        }
        let kinds = ts_union("incoming.ts", "IncomingKind");
        for kind in ["new", "update", "alreadyInstalled"] {
            assert!(kinds.contains(kind));
        }

        // Окно присылает запрос в своём виде — Rust его принимает.
        let request: IncomingInstallRequest = serde_json::from_value(serde_json::json!({
            "zipPath": item["zipPath"],
            "fileName": "new_mod.zip",
            "replaces": null,
            "allowWarnings": true,
        }))
        .unwrap();
        let shape = crate::test_support::ts_interface("incoming.ts", "IncomingInstallRequest");
        assert_eq!(
            shape
                .required
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["allowWarnings", "fileName", "replaces", "zipPath"]
        );
        let results = install_items(&state, &[request]).unwrap();
        let result = serde_json::to_value(&results[0]).unwrap();
        assert_matches_ts(&result, "incoming.ts", "IncomingInstallResult");

        // Отмена приходит из окна билетом — тем же набором полей.
        let ticket: IncomingTicketDto = serde_json::from_value(serde_json::json!({
            "fileName": result["fileName"],
            "replaces": result["replaces"],
            "backupId": result["backupId"],
            "wasDisabled": result["wasDisabled"],
            "sameName": result["sameName"],
        }))
        .unwrap();
        assert_matches_ts(
            &serde_json::to_value(&ticket).unwrap(),
            "incoming.ts",
            "IncomingTicket",
        );
        undo_install(&state, &ticket).unwrap();
    }

    #[test]
    fn delete_source_is_limited_to_download_folders() {
        let temp = tempfile::tempdir().unwrap();
        let downloads = temp.path().join("downloads");
        let other = temp.path().join("other");
        std::fs::create_dir_all(&downloads).unwrap();
        std::fs::create_dir_all(&other).unwrap();
        let inside = downloads.join("mod.zip");
        let outside = other.join("mod.zip");
        std::fs::write(&inside, b"x").unwrap();
        std::fs::write(&outside, b"x").unwrap();
        let settings = bmm_domain::settings::AppSettings {
            downloads_folder: Some(downloads.display().to_string()),
            ..bmm_domain::settings::AppSettings::default()
        };
        let app_root = temp.path().join("app");
        assert!(is_download_source(&inside, &settings, &app_root));
        assert!(!is_download_source(&outside, &settings, &app_root));
        std::fs::write(downloads.join("readme.txt"), b"x").unwrap();
        assert!(!is_download_source(
            &downloads.join("readme.txt"),
            &settings,
            &app_root
        ));
    }

    #[test]
    fn remove_source_respects_the_switch() {
        let temp = tempfile::tempdir().unwrap();
        let state = crate::test_support::test_state(&temp, temp.path());
        let downloads = temp.path().join("downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let path = downloads.join("mod.zip");
        std::fs::write(&path, b"x").unwrap();
        let mut settings = state.settings.load().settings;
        settings.downloads_folder = Some(downloads.display().to_string());
        settings.delete_downloads_after_install = false;
        state.settings.save(&settings).unwrap();
        assert!(!remove_download_source(&state, &path.display().to_string()).unwrap());
        assert!(path.is_file(), "выключенная настройка ничего не удаляет");

        settings.delete_downloads_after_install = true;
        state.settings.save(&settings).unwrap();
        assert!(remove_download_source(&state, &path.display().to_string()).unwrap());
        assert!(!path.exists(), "включённая настройка удаляет архив");
    }

    #[test]
    fn install_new_mod_appears_in_library() {
        let (temp, state) = setup();
        let source = temp.path().join("new.zip");
        make_zip(&source, b"body");
        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: source.display().to_string(),
                file_name: "new_mod.zip".to_string(),
                replaces: None,
                allow_warnings: true,
            }],
        )
        .unwrap();
        assert!(results[0].success, "{:?}", results[0].message);
        let snapshot = crate::commands::library::load_snapshot(&state).expect("snapshot");
        assert!(
            snapshot
                .mods
                .iter()
                .any(|item| item.file_name == "new_mod.zip")
        );

        undo_install(
            &state,
            &IncomingTicketDto {
                file_name: "new_mod.zip".to_string(),
                replaces: None,
                backup_id: None,
                was_disabled: false,
                same_name: false,
            },
        )
        .unwrap();
        assert!(
            !installer_for_test(&state)
                .layout()
                .mods_dir
                .join("new_mod.zip")
                .exists()
        );
    }

    #[test]
    fn update_under_another_name_keeps_tracked_page() {
        // Обновление узнано по содержимому, а имя файла другое: страница мода и проверки
        // должны остаться за ним, иначе программа «забудет», где следить за модом.
        let (temp, state) = setup();
        let v1 = temp.path().join("v1.zip");
        let v2 = temp.path().join("v2.zip");
        make_zip(&v1, b"one");
        make_zip(&v2, b"two, longer body");
        let installer = installer_for_test(&state);
        assert!(
            installer
                .install_from_path(&v1, "acadia.zip", InstallOptions::default())
                .success
        );
        let first = crate::commands::library::load_snapshot(&state).unwrap();
        let id = first.mods[0].id.clone();
        state
            .library
            .upsert_binding(bmm_domain::library::ModBinding {
                mod_id: id.clone(),
                source: "stmods".to_string(),
                source_id: "acadia".to_string(),
                url: url::Url::parse("https://stmods.org/acadia/").unwrap(),
                confirmed: true,
            })
            .unwrap();

        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: v2.display().to_string(),
                file_name: "GMC_Acadia_GMT968.zip".to_string(),
                replaces: Some("acadia.zip".to_string()),
                allow_warnings: true,
            }],
        )
        .unwrap();
        assert!(results[0].success, "{:?}", results[0].message);

        let after = crate::commands::library::load_snapshot(&state).unwrap();
        assert_eq!(after.mods.len(), 1);
        assert_eq!(after.mods[0].id, id, "та же запись библиотеки");
        assert_eq!(after.mods[0].file_name, "GMC_Acadia_GMT968.zip");
        assert!(!after.mods[0].unchecked, "страница мода не потерялась");
        assert_eq!(state.library.bindings().len(), 1);

        // «Отменить» возвращает и файл, и запись со страницей мода.
        undo_install(
            &state,
            &IncomingTicketDto {
                file_name: "GMC_Acadia_GMT968.zip".to_string(),
                replaces: Some("acadia.zip".to_string()),
                backup_id: results[0].backup_id.clone(),
                was_disabled: false,
                same_name: false,
            },
        )
        .unwrap();
        let undone = crate::commands::library::load_snapshot(&state).unwrap();
        assert_eq!(undone.mods.len(), 1);
        assert_eq!(undone.mods[0].id, id);
        assert_eq!(undone.mods[0].file_name, "acadia.zip");
        assert!(!undone.mods[0].unchecked);
    }
}

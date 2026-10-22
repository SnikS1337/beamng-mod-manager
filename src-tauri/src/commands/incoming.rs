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
        result.push(InstalledArchive {
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
            library: JsonLibraryStore::new(data.join("library.json"), system_clock()),
            previews: PreviewCache::new(data.join("previews")),
            load_lock: Mutex::new(()),
            previews_by_id: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
            deleted: Mutex::new(HashMap::new()),
            content_cache: Mutex::new(HashMap::new()),
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
}

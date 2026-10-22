use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;
use url::Url;

use bmm_domain::clock::system_clock;
use bmm_domain::filename;
use bmm_domain::library::{CheckState, DiskMod, LibraryStore, ModBinding};
use bmm_domain::settings::AppSettings;
use bmm_domain::text::ModTitles;
use bmm_game::db::{GameModDb, SetActiveOutcome};
use bmm_game::folders::{self, FolderProblem};
use bmm_game::{info, process};
use bmm_packs::cache::{CachedPreview, PreviewCache};
use bmm_packs::install::ModInstaller;
use bmm_packs::overview::{self, ArchiveOverview};
use bmm_packs::preview;
use bmm_packs::scan::{self, ModLayout};
use bmm_packs::versions;

use crate::error::AppError;
use crate::state::{AppState, PreviewTarget};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatDto {
    pub mod_for: Option<String>,
    pub game: Option<String>,
    pub text: Option<String>,
    pub differs: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModCardDto {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub version: Option<String>,
    pub size_bytes: i64,
    pub enabled: bool,
    pub sub_folder: Option<String>,
    pub kind: String,
    pub preview_url: Option<String>,
    pub compat: CompatDto,
    pub update: String,
    pub pending: String,
    pub from_game_repo: bool,
    pub deleted: bool,
    pub copies: usize,
    pub saved_at: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterCounts {
    pub all: usize,
    pub enabled: usize,
    pub updates: usize,
    pub unchecked: usize,
    pub deleted: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub mods: Vec<ModCardDto>,
    pub counts: FilterCounts,
    pub folder: Option<String>,
    pub folder_error: Option<String>,
    pub game_version: Option<String>,
    pub game_running: bool,
    pub warning: Option<String>,
    pub deleted: Vec<ModCardDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletedTicketDto {
    pub id: String,
    pub file_name: String,
    pub backup_id: String,
    pub was_in_disabled: bool,
    pub name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRowDto {
    pub backup_id: String,
    pub file_name: String,
    pub version: Option<String>,
    pub saved_at: Option<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleResult {
    pub outcome: String,
    pub enabled: bool,
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveDto {
    pub vehicles: Vec<String>,
    pub levels: Vec<String>,
    pub has_scripts: bool,
    pub lua_files: usize,
    pub files: usize,
    pub unpacked_bytes: u64,
    pub warnings: Vec<String>,
    pub unreadable: bool,
    pub title: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BindingDto {
    pub source: String,
    pub url: String,
    pub confirmed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModDetails {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub version: Option<String>,
    pub size_bytes: i64,
    pub enabled: bool,
    pub installed_at: String,
    pub folder: Option<String>,
    pub kind: String,
    pub compat: CompatDto,
    pub archive: Option<ArchiveDto>,
    pub preview_url: Option<String>,
    pub binding: Option<BindingDto>,
}

#[tauri::command]
pub async fn library_load(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<LibrarySnapshot, AppError> {
    let state = state.inner().clone();
    let _ = app;
    tauri::async_runtime::spawn_blocking(move || load_snapshot(&state))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn library_refresh(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<LibrarySnapshot, AppError> {
    library_load(app, state).await
}

#[tauri::command]
pub async fn mod_toggle(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    enabled: bool,
) -> Result<ToggleResult, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || toggle_mod(&state, &id, enabled))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn mod_details(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<ModDetails, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || details(&state, &id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn mod_preview(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<Option<String>, AppError> {
    let state = state.inner().clone();
    let gate = state.preview_gate.clone();
    let _permit = gate
        .acquire()
        .await
        .map_err(|error| AppError::new("busy", error.to_string()))?;
    let task_state = state.clone();
    tauri::async_runtime::spawn_blocking(move || ensure_preview(&task_state, &id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))
}

pub fn preview_url(key: &str, extension: &str) -> String {
    format!("http://bmm.localhost/preview/{key}.{extension}")
}

pub(crate) fn load_snapshot(state: &AppState) -> Result<LibrarySnapshot, AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать загрузку"))?;

    let settings_load = state.settings.load();
    let settings = settings_load.settings;
    let game_running = process::is_running();

    let (mods_dir, folder_error) = resolve_folder(&settings);
    let Some(mods_dir) = mods_dir else {
        return Ok(empty_snapshot(
            folder_error.or(Some("Папка модов не найдена.".to_string())),
            game_running,
            settings_load.warnings,
        ));
    };

    let layout = ModLayout::in_app_folder(&mods_dir, state.paths.root());
    let db = GameModDb::new(&mods_dir);
    let keep = settings.keep_backups.max(1) as usize;
    let installer = ModInstaller::new(layout.clone(), system_clock(), Arc::new(move || keep));

    if !game_running {
        apply_pending(state, &db, &layout);
    }

    let mut scan_result = scan::scan_mods(&layout);
    let db_read = db.read_stable();
    if !db_read.unreadable {
        for disk in &mut scan_result.mods {
            if let Some(entry) = db_read.find(&disk.file_name)
                && entry.is_at(disk.sub_folder.as_deref())
                && !entry.active
            {
                disk.enabled = false;
            }
        }
    }

    // Пока тост удаления жив, запись библиотеки должна пережить сверку: подставляем фиктивный файл,
    // как старый GarageBackend, но саму карточку не показываем.
    let tickets_before = state.deleted.lock().unwrap().clone();
    for ticket in tickets_before.values() {
        if scan_result
            .mods
            .iter()
            .any(|disk| disk.file_name.eq_ignore_ascii_case(&ticket.file_name))
        {
            continue;
        }
        let size = state
            .library
            .get_mod(&ticket.mod_id)
            .and_then(|installed| installed.size_bytes)
            .unwrap_or(0);
        scan_result.mods.push(DiskMod {
            file_name: ticket.file_name.clone(),
            enabled: ticket.was_enabled,
            size_bytes: size,
            sub_folder: None,
            modified_utc: None,
        });
    }

    let scan_warnings = std::mem::take(&mut scan_result.warnings);
    let disks_by_file: HashMap<String, &DiskMod> = scan_result
        .mods
        .iter()
        .map(|disk| (disk.file_name.to_lowercase(), disk))
        .collect();

    // Первый скан 5000 модов не должен переписывать library.json 5000 раз.
    let _batch = state.library.batch();
    let sync_report = bmm_domain::library::reconcile(
        &state.library,
        &scan_result.mods,
        scan_result.folder_available,
        Utc::now().fixed_offset(),
    );

    apply_repo_info(state, &db_read, &mods_dir);

    let installed_list = state.library.mods();
    let bindings = state.library.bindings();
    let bindings_by_mod: HashMap<String, &ModBinding> = bindings
        .iter()
        .map(|binding| (binding.mod_id.clone(), binding))
        .collect();
    let confirmed_ids: HashSet<String> = bindings
        .iter()
        .filter(|binding| binding.confirmed)
        .map(|binding| binding.mod_id.clone())
        .collect();
    let checks_by_key: HashMap<(String, String), CheckState> = state
        .library
        .checks()
        .into_iter()
        .map(|check| ((check.mod_id.clone(), check.source.clone()), check))
        .collect();
    let pending_snapshot = state.pending.lock().unwrap().clone();

    state.previews_by_id.lock().unwrap().clear();
    let mut targets: HashMap<String, PreviewTarget> = HashMap::new();
    let mut preview_keys: HashMap<String, String> = HashMap::new();
    for installed in &installed_list {
        let Some(disk) = disks_by_file
            .get(&installed.file_name.to_lowercase())
            .copied()
        else {
            continue;
        };
        let path = path_of(&layout, disk);
        let key = PreviewCache::key(&installed.file_name, disk.size_bytes, disk.modified_utc);
        preview_keys.insert(installed.id.clone(), key.clone());
        targets.insert(
            installed.id.clone(),
            PreviewTarget {
                path,
                key,
                file_name: installed.file_name.clone(),
                sub_folder: disk.sub_folder.clone(),
            },
        );
    }
    *state.previews_by_id.lock().unwrap() = targets;

    let game_version = mods_dir.parent().and_then(info::detect_version);
    let mods: Vec<ModCardDto> = installed_list
        .into_iter()
        .filter(|installed| !tickets_before.contains_key(&installed.id))
        .map(|installed| {
            let disk = disks_by_file
                .get(&installed.file_name.to_lowercase())
                .copied();
            let entry = db_read.find(&installed.file_name);
            let repo = entry.and_then(|entry| entry.repo.as_ref());
            let binding = bindings_by_mod.get(&installed.id).copied();
            let check = binding.and_then(|binding| {
                checks_by_key.get(&(installed.id.clone(), binding.source.clone()))
            });
            let site_title = check.as_ref().and_then(|check| check.last_title.as_deref());
            let repo_title = repo.and_then(|repo| repo.title.as_deref());
            let name = ModTitles::choose(repo_title, None, site_title, &installed.name);
            let version = repo
                .and_then(|repo| repo.version.clone())
                .or_else(|| installed.version.clone());
            let compat = compat_of(
                check
                    .as_ref()
                    .and_then(|check| check.last_game_version.as_deref()),
                game_version.as_deref(),
            );
            let preview_url =
                preview_keys
                    .get(&installed.id)
                    .and_then(|key| match state.previews.get(key) {
                        Some(CachedPreview::Image { extension, .. }) => {
                            Some(preview_url(key, &extension))
                        }
                        _ => None,
                    });
            let pending = pending_snapshot.contains_key(&installed.file_name);
            let kind = entry
                .map(|entry| kind_of(&entry.mod_type))
                .unwrap_or("unknown");
            ModCardDto {
                id: installed.id.clone(),
                name,
                file_name: installed.file_name.clone(),
                version,
                size_bytes: installed.size_bytes.unwrap_or(0),
                enabled: installed.enabled,
                sub_folder: disk.and_then(|disk| disk.sub_folder.clone()),
                kind: kind.to_string(),
                preview_url,
                compat,
                update: "none".to_string(),
                pending: if pending { "queued" } else { "none" }.to_string(),
                from_game_repo: disk.and_then(|disk| disk.sub_folder.as_ref()).is_some(),
                deleted: false,
                copies: 0,
                saved_at: None,
            }
        })
        .collect();

    let ticket_files: HashSet<String> = tickets_before
        .values()
        .map(|ticket| ticket.file_name.to_lowercase())
        .collect();
    let deleted_cards: Vec<ModCardDto> = versions::deleted(&installer, &state.library)
        .into_iter()
        .filter(|item| !ticket_files.contains(&item.file_name.to_lowercase()))
        .map(|item| {
            let guessed = filename::from_file_name(&item.file_name);
            let name = if guessed.name.trim().is_empty() {
                item.file_name
                    .strip_suffix(".zip")
                    .unwrap_or(&item.file_name)
                    .to_string()
            } else {
                guessed.name
            };
            ModCardDto {
                id: format!("deleted:{}", item.file_name),
                name,
                file_name: item.file_name,
                version: guessed.version,
                size_bytes: i64::try_from(item.last_size_bytes).unwrap_or(i64::MAX),
                enabled: false,
                sub_folder: None,
                kind: "unknown".to_string(),
                preview_url: None,
                compat: compat_of(None, game_version.as_deref()),
                update: "none".to_string(),
                pending: "none".to_string(),
                from_game_repo: false,
                deleted: true,
                copies: item.copies,
                saved_at: item.last_saved_at.map(|date| date.to_rfc3339()),
            }
        })
        .collect();

    let enabled = mods.iter().filter(|item| item.enabled).count();
    let unchecked = mods
        .iter()
        .filter(|item| !confirmed_ids.contains(&item.id))
        .count();
    let warning = join_warnings(
        settings_load.warnings,
        scan_warnings,
        db_read.warnings,
        sync_report.warning,
        state.library.load_warnings().to_vec(),
    );

    Ok(LibrarySnapshot {
        counts: FilterCounts {
            all: mods.len(),
            enabled,
            updates: 0,
            unchecked,
            deleted: deleted_cards.len(),
        },
        mods,
        folder: Some(mods_dir.display().to_string()),
        folder_error,
        game_version,
        game_running,
        warning,
        deleted: deleted_cards,
    })
}

pub(crate) fn resolve_folder(settings: &AppSettings) -> (Option<PathBuf>, Option<String>) {
    if settings.mods_folder.is_some() || settings.user_folder.is_some() {
        let check = folders::from_settings(settings, false);
        if let Some(path) = check.mods_dir.clone() {
            let error = if check.problem == FolderProblem::None {
                None
            } else {
                Some(check.message.clone())
            };
            return (Some(path), error);
        }
        return (None, Some(check.message));
    }

    let roots = folders::default_user_roots();
    if let Some(found) = folders::auto_detect(&[], &roots) {
        return (Some(found), None);
    }
    (None, Some("Папка игры не выбрана.".to_string()))
}

fn apply_pending(state: &AppState, db: &GameModDb, layout: &ModLayout) {
    let pending = state.pending.lock().unwrap().clone();
    if pending.is_empty() {
        return;
    }
    let installer = ModInstaller::new(layout.clone(), system_clock(), Arc::new(|| 3));
    let disk = scan::scan_mods(layout).mods;
    for (file_name, enabled) in &pending {
        let Some(item) = disk
            .iter()
            .find(|item| item.file_name.eq_ignore_ascii_case(file_name))
        else {
            continue;
        };
        if item.sub_folder.is_some() {
            continue;
        }
        if *enabled && !item.enabled {
            let _ = installer.enable(&item.file_name);
        } else if !*enabled && item.enabled {
            let _ = installer.disable(&item.file_name);
        }
    }
    let changes: Vec<(&str, bool)> = pending
        .iter()
        .map(|(file, enabled)| (file.as_str(), *enabled))
        .collect();
    let outcome = db.set_active(&changes, || false);
    if outcome.success() {
        state.pending.lock().unwrap().clear();
        let _ = state.save_pending();
    }
}

fn apply_repo_info(
    state: &AppState,
    db: &bmm_game::db::GameDbReadResult,
    _mods_dir: &std::path::Path,
) {
    if db.unreadable {
        return;
    }
    let _batch = state.library.batch();
    for installed in state.library.mods() {
        let Some(entry) = db.find(&installed.file_name) else {
            continue;
        };
        let Some(repo) = &entry.repo else {
            continue;
        };
        let mut updated = installed.clone();
        if let Some(version) = &repo.version {
            updated.version = Some(version.clone());
        }
        if let Some(title) = &repo.title {
            let guessed = bmm_domain::filename::from_file_name(&installed.file_name);
            if installed.name == guessed.name || installed.name == installed.file_name {
                updated.name = title.clone();
            }
        }
        state.library.upsert_mod(updated);
        if let Some(url) = repo.page_url().and_then(|url| Url::parse(&url).ok()) {
            let _ = state
                .library
                .upsert_binding(bmm_domain::library::ModBinding {
                    mod_id: installed.id.clone(),
                    source: "beamng".to_string(),
                    source_id: repo.unique_id.clone(),
                    url,
                    confirmed: true,
                });
        }
    }
}

fn path_of(layout: &ModLayout, disk: &DiskMod) -> PathBuf {
    match &disk.sub_folder {
        Some(sub_folder) => layout.mods_dir.join(sub_folder).join(&disk.file_name),
        None if disk.enabled => layout.mods_dir.join(&disk.file_name),
        None => layout.disabled_dir.join(&disk.file_name),
    }
}

fn toggle_mod(state: &AppState, id: &str, enabled: bool) -> Result<ToggleResult, AppError> {
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
    let Some(installed) = state.library.get_mod(id) else {
        return Err(AppError::new("not_found", "Мод не найден."));
    };
    let layout = ModLayout::in_app_folder(&mods_dir, state.paths.root());
    let disk = scan::scan_mods(&layout)
        .mods
        .into_iter()
        .find(|item| item.file_name.eq_ignore_ascii_case(&installed.file_name))
        .ok_or_else(|| AppError::new("missing", "Файл мода не найден."))?;

    if process::is_running() {
        return Ok(queue_toggle(state, &installed.file_name, enabled));
    }

    let keep = settings.keep_backups.max(1) as usize;
    let installer = ModInstaller::new(layout, system_clock(), Arc::new(move || keep));

    // Выключенный раньше переносом файл возвращаем на место; db.json правим, только если игра его помнит.
    if enabled && !disk.enabled && disk.sub_folder.is_none() {
        let moved = installer.enable(&disk.file_name);
        if !moved.success {
            return Err(AppError::new("move", moved.message));
        }
        let db = GameModDb::new(&mods_dir);
        let _ = db.set_active(&[(&disk.file_name, true)], || false);
        let mut updated = installed;
        updated.enabled = true;
        state.library.upsert_mod(updated);
        return Ok(ToggleResult {
            outcome: "done".to_string(),
            enabled: true,
            message: Some("Включён.".to_string()),
        });
    }

    let db = GameModDb::new(&mods_dir);
    let result = db.set_active(&[(&disk.file_name, enabled)], || false);
    match result.outcome {
        SetActiveOutcome::Done | SetActiveOutcome::Unchanged => {
            let mut updated = installed;
            updated.enabled = enabled;
            state.library.upsert_mod(updated);
            Ok(ToggleResult {
                outcome: if result.outcome == SetActiveOutcome::Done {
                    "done"
                } else {
                    "unchanged"
                }
                .to_string(),
                enabled,
                message: None,
            })
        }
        SetActiveOutcome::GameRunning => Ok(queue_toggle(state, &installed.file_name, enabled)),
        SetActiveOutcome::NotInDb | SetActiveOutcome::NoDb if enabled => {
            let mut updated = installed;
            updated.enabled = true;
            state.library.upsert_mod(updated);
            Ok(ToggleResult {
                outcome: "done".to_string(),
                enabled: true,
                message: Some("Включён: игра подхватит его при запуске.".to_string()),
            })
        }
        SetActiveOutcome::NotInDb | SetActiveOutcome::NoDb => {
            if disk.from_game_repo() {
                return Err(AppError::new(
                    "game_repo",
                    "Игра ещё не знает этот мод. Запустите игру один раз, потом выключите.",
                ));
            }
            if disk.enabled {
                let moved = installer.disable(&disk.file_name);
                if !moved.success {
                    return Err(AppError::new("move", moved.message));
                }
            }
            let mut updated = installed;
            updated.enabled = false;
            state.library.upsert_mod(updated);
            Ok(ToggleResult {
                outcome: "done".to_string(),
                enabled: false,
                message: Some(
                    "Выключен. Игра ещё не видела этот мод, поэтому файл убран в папку программы."
                        .to_string(),
                ),
            })
        }
        other => Err(AppError::new(
            "game_db",
            format!("Не удалось изменить db.json: {other:?}"),
        )),
    }
}

fn queue_toggle(state: &AppState, file_name: &str, enabled: bool) -> ToggleResult {
    state
        .pending
        .lock()
        .unwrap()
        .insert(file_name.to_string(), enabled);
    let _ = state.save_pending();
    ToggleResult {
        outcome: "queued".to_string(),
        enabled,
        message: Some("Изменение применится, когда закроете игру.".to_string()),
    }
}

fn details(state: &AppState, id: &str) -> Result<ModDetails, AppError> {
    let installed = state
        .library
        .get_mod(id)
        .ok_or_else(|| AppError::new("not_found", "Мод не найден."))?;
    let target = state.previews_by_id.lock().unwrap().get(id).cloned();
    let settings = state.settings.load().settings;
    let (mods_dir, _) = resolve_folder(&settings);
    let game_version = mods_dir
        .as_ref()
        .and_then(|folder| folder.parent().map(|parent| parent.to_path_buf()))
        .and_then(|folder| info::detect_version(&folder));
    let db = mods_dir.map(GameModDb::new);
    let db_read = db.as_ref().map(|db| db.read_stable());
    let entry = db_read
        .as_ref()
        .and_then(|read| read.find(&installed.file_name));
    let repo = entry.and_then(|entry| entry.repo.as_ref());
    let binding = state
        .library
        .bindings()
        .into_iter()
        .find(|binding| binding.mod_id == installed.id);
    let check = binding
        .as_ref()
        .and_then(|binding| state.library.get_check(&installed.id, &binding.source));
    let compat = compat_of(
        check
            .as_ref()
            .and_then(|check| check.last_game_version.as_deref()),
        game_version.as_deref(),
    );
    let archive = target
        .as_ref()
        .map(|target| overview::read_overview(&target.path))
        .map(archive_dto);
    let preview_url = target
        .as_ref()
        .and_then(|target| match state.previews.get(&target.key) {
            Some(CachedPreview::Image { extension, .. }) => {
                Some(preview_url(&target.key, &extension))
            }
            _ => None,
        });
    let site_title = check.as_ref().and_then(|check| check.last_title.as_deref());
    let name = ModTitles::choose(
        repo.and_then(|repo| repo.title.as_deref()),
        None,
        site_title,
        &installed.name,
    );
    let version = repo
        .and_then(|repo| repo.version.clone())
        .or_else(|| installed.version.clone());
    let kind = entry
        .map(|entry| kind_of(&entry.mod_type))
        .unwrap_or("unknown");

    Ok(ModDetails {
        id: installed.id.clone(),
        name,
        file_name: installed.file_name.clone(),
        version,
        size_bytes: installed.size_bytes.unwrap_or(0),
        enabled: installed.enabled,
        installed_at: installed.installed_at_utc.to_rfc3339(),
        folder: target
            .as_ref()
            .map(|target| target.path.display().to_string()),
        kind: kind.to_string(),
        compat,
        archive,
        preview_url,
        binding: binding.map(|binding| BindingDto {
            source: binding.source,
            url: binding.url.to_string(),
            confirmed: binding.confirmed,
        }),
    })
}

fn ensure_preview(state: &AppState, id: &str) -> Option<String> {
    let target = state.previews_by_id.lock().unwrap().get(id).cloned()?;
    if let Some(cached) = state.previews.get(&target.key) {
        return match cached {
            CachedPreview::Image { extension, .. } => Some(preview_url(&target.key, &extension)),
            CachedPreview::NoImage => None,
        };
    }
    let pick = preview::pick(&target.path).ok().flatten();
    let Some(pick) = pick else {
        let _ = state.previews.put_no_image(&target.key, &target.file_name);
        return None;
    };
    let bytes = preview::extract(&target.path, &pick.raw_name).ok()?;
    if bytes.len() > 30 * 1024 * 1024 {
        return None;
    }
    state
        .previews
        .put_image(&target.key, &pick.extension, &bytes)
        .ok()?;
    Some(preview_url(&target.key, &pick.extension))
}

fn archive_dto(overview: ArchiveOverview) -> ArchiveDto {
    ArchiveDto {
        vehicles: overview.vehicles,
        levels: overview.levels,
        has_scripts: overview.has_scripts,
        lua_files: overview.lua_files,
        files: overview.files,
        unpacked_bytes: overview.unpacked_bytes,
        warnings: overview.warnings,
        unreadable: overview.unreadable,
        title: overview.title,
    }
}

fn compat_of(mod_for: Option<&str>, game: Option<&str>) -> CompatDto {
    let level = info::compare_compat(mod_for, game);
    CompatDto {
        mod_for: info::short_version(mod_for),
        game: info::short_version(game),
        text: info::describe_compat(mod_for, game),
        differs: !matches!(level, info::CompatLevel::Same | info::CompatLevel::Unknown),
    }
}

fn kind_of(mod_type: &str) -> &'static str {
    match mod_type.to_lowercase().as_str() {
        "vehicle" => "vehicle",
        "terrain" | "level" => "level",
        "sound" => "sound",
        "scenario" | "app" | "script" => "script",
        _ => "unknown",
    }
}

fn join_warnings(
    settings: Vec<String>,
    scan: Vec<String>,
    db: Vec<String>,
    sync: Option<String>,
    library: Vec<String>,
) -> Option<String> {
    let mut all = Vec::new();
    all.extend(settings);
    all.extend(scan);
    all.extend(db);
    all.extend(sync);
    all.extend(library);
    if all.is_empty() {
        None
    } else {
        Some(all.join(" · "))
    }
}

fn empty_snapshot(
    folder_error: Option<String>,
    game_running: bool,
    settings_warnings: Vec<String>,
) -> LibrarySnapshot {
    LibrarySnapshot {
        mods: Vec::new(),
        counts: FilterCounts {
            all: 0,
            enabled: 0,
            updates: 0,
            unchecked: 0,
            deleted: 0,
        },
        folder: None,
        folder_error,
        game_version: None,
        game_running,
        warning: if settings_warnings.is_empty() {
            None
        } else {
            Some(settings_warnings.join(" · "))
        },
        deleted: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use bmm_domain::clock::system_clock;
    use bmm_domain::library::JsonLibraryStore;
    use bmm_domain::paths::AppPaths;
    use bmm_domain::settings::AppSettings;
    use bmm_domain::settings_store::SettingsStore;
    use bmm_packs::cache::PreviewCache;

    #[test]
    fn loads_real_folder_and_toggles_active() {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("data");
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("mod.zip"), b"zip").unwrap();
        std::fs::write(
            mods.join("db.json"),
            r#"{"header":{"version":1.1},"mods":{"mod":{"filename":"mod.zip","active":true,"modType":"vehicle"}}}"#,
        )
        .unwrap();

        let paths = AppPaths::at(&data);
        paths.ensure_dirs().unwrap();
        let settings = SettingsStore::with_system_clock(paths.settings());
        settings
            .save(&AppSettings {
                mods_folder: Some(mods.display().to_string()),
                ..AppSettings::default()
            })
            .unwrap();
        let state = AppState {
            paths,
            settings,
            library: JsonLibraryStore::new(temp.path().join("data/library.json"), system_clock()),
            previews: PreviewCache::new(temp.path().join("data/previews")),
            load_lock: Mutex::new(()),
            previews_by_id: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
            deleted: Mutex::new(HashMap::new()),
            content_cache: Mutex::new(HashMap::new()),
        };

        let snapshot = load_snapshot(&state).unwrap();
        assert_eq!(snapshot.mods.len(), 1);
        assert_eq!(snapshot.mods[0].name, "mod");
        assert!(snapshot.mods[0].enabled);

        let result = toggle_mod(&state, &snapshot.mods[0].id, false).unwrap();
        assert_eq!(result.outcome, "done");
        assert!(!state.library.get_mod(&snapshot.mods[0].id).unwrap().enabled);

        let db = GameModDb::new(&mods);
        let read = db.read();
        assert!(!read.find("mod.zip").unwrap().active);
        assert!(db.backup_path().exists());
    }

    fn test_state(temp: &tempfile::TempDir, mods: &std::path::Path) -> AppState {
        let data = temp.path().join("data");
        let paths = AppPaths::at(&data);
        paths.ensure_dirs().unwrap();
        let settings = SettingsStore::with_system_clock(paths.settings());
        settings
            .save(&AppSettings {
                mods_folder: Some(mods.display().to_string()),
                ..AppSettings::default()
            })
            .unwrap();
        AppState {
            paths,
            settings,
            library: JsonLibraryStore::new(temp.path().join("data/library.json"), system_clock()),
            previews: PreviewCache::new(temp.path().join("data/previews")),
            load_lock: Mutex::new(()),
            previews_by_id: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
            deleted: Mutex::new(HashMap::new()),
            content_cache: Mutex::new(HashMap::new()),
        }
    }

    #[test]
    fn toggle_moves_files_the_game_has_not_seen() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("fresh.zip"), b"zip").unwrap();
        let state = test_state(&temp, &mods);
        let layout = ModLayout::in_app_folder(&mods, state.paths.root());

        let snapshot = load_snapshot(&state).unwrap();
        let id = snapshot.mods[0].id.clone();
        let off = toggle_mod(&state, &id, false).unwrap();
        assert_eq!(off.outcome, "done");
        assert!(layout.disabled_dir.join("fresh.zip").is_file());
        assert!(!mods.join("fresh.zip").exists());
        assert!(!state.library.get_mod(&id).unwrap().enabled);

        let on = toggle_mod(&state, &id, true).unwrap();
        assert_eq!(on.outcome, "done");
        assert!(mods.join("fresh.zip").is_file());
        assert!(state.library.get_mod(&id).unwrap().enabled);
    }

    #[test]
    fn apply_pending_moves_file_and_clears_queue() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(
            mods.join("db.json"),
            r#"{"header":{"version":1.1},"mods":{"mod":{"filename":"mod.zip","active":false,"modType":"vehicle"}}}"#,
        )
        .unwrap();
        let state = test_state(&temp, &mods);
        let layout = ModLayout::in_app_folder(&mods, state.paths.root());
        std::fs::create_dir_all(&layout.disabled_dir).unwrap();
        std::fs::write(layout.disabled_dir.join("mod.zip"), b"zip").unwrap();
        state
            .pending
            .lock()
            .unwrap()
            .insert("mod.zip".to_string(), true);

        apply_pending(&state, &GameModDb::new(&mods), &layout);

        assert!(mods.join("mod.zip").is_file());
        assert!(!layout.disabled_dir.join("mod.zip").exists());
        assert!(state.pending.lock().unwrap().is_empty());
    }
}

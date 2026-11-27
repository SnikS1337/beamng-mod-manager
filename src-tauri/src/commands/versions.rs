use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::Serialize;
use ts_rs::TS;

use bmm_domain::clock::system_clock;
use bmm_domain::library::LibraryStore;
use bmm_packs::install::ModInstaller;
use bmm_packs::scan::ModLayout;
use bmm_packs::versions;

use crate::commands::library::{DeletedTicketDto, VersionRowDto, resolve_folder};
use crate::error::AppError;
use crate::state::{AppState, DeletedTicket};

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "RestoreUndoTicket")]
pub struct RestoreUndoTicketDto {
    pub backup_id: String,
    pub installed_token: String,
}

#[tauri::command]
pub async fn mod_delete(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<DeletedTicketDto, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || delete_mod(&state, &id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn mod_undelete(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || undelete_mod(&state, &id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn mod_forget_deleted(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || forget_deleted(&state, &id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn mod_restore_deleted(
    state: tauri::State<'_, Arc<AppState>>,
    file_name: String,
    backup_id: Option<String>,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        restore_deleted(&state, &file_name, backup_id.as_deref())
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn mod_versions(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<Vec<VersionRowDto>, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || versions_of(&state, &id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn mod_restore_version(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    backup_id: String,
    expected_token: Option<String>,
) -> Result<Option<RestoreUndoTicketDto>, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        restore_version(&state, &id, &backup_id, expected_token.as_deref())
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn mod_commit_restore_undo(
    state: tauri::State<'_, Arc<AppState>>,
    backup_id: String,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    commit_restore_undo(&state, &backup_id);
    Ok(())
}

fn commit_restore_undo(state: &AppState, backup_id: &str) {
    state.release_backup(backup_id);
}

/// «Удалить» у прошлой версии: копия стирается насовсем.
#[tauri::command]
pub async fn mod_delete_version(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
    backup_id: String,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || delete_version(&state, &id, &backup_id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

fn delete_version(state: &AppState, id: &str, backup_id: &str) -> Result<(), AppError> {
    let _guard = state.load_guard();
    let installed = state
        .library
        .get_mod(id)
        .ok_or_else(|| AppError::new("not_found", "Мод не найден."))?;
    let installer = installer_for(state)?;
    // Удаляем только то, что показано у этого мода в «Прошлых версиях».
    let version = versions::versions_for(&installer, state.library.as_ref(), &installed)
        .into_iter()
        .find(|version| version.backup.id == backup_id)
        .ok_or_else(|| AppError::new("not_found", "Такой копии нет."))?;
    undo_needs(state, backup_id)?;
    let result = installer.delete_backup(&version.backup.file_name, &version.backup.id);
    if result.success {
        Ok(())
    } else {
        Err(AppError::new("delete", result.message))
    }
}

/// Копия только что удалённого мода нужна кнопке «Отменить»: пока тост не ушёл, её
/// нельзя ни стереть, ни вернуть под другим именем — иначе «Отменить» сломается.
fn undo_needs(state: &AppState, backup_id: &str) -> Result<(), AppError> {
    let held = crate::state::lock(&state.deleted)
        .values()
        .any(|ticket| ticket.backup_id == backup_id);
    if held {
        return Err(AppError::new(
            "busy",
            "Эта копия ещё нужна для «Отменить». Попробуйте через несколько секунд.",
        ));
    }
    Ok(())
}

pub(crate) fn installer_for(state: &AppState) -> Result<ModInstaller, AppError> {
    let settings = state.settings.load().settings;
    let (mods_dir, error) = resolve_folder(&settings);
    let Some(mods_dir) = mods_dir else {
        return Err(AppError::new(
            "folder",
            error.unwrap_or_else(|| "Папка модов не найдена.".to_string()),
        ));
    };
    let layout = ModLayout::in_app_folder(&mods_dir, state.paths.root());
    let keep = settings.keep_backups.max(1) as usize;
    Ok(
        ModInstaller::new(layout, system_clock(), Arc::new(move || keep))
            .with_backup_guard(state.backup_guard()),
    )
}

fn delete_mod(state: &AppState, id: &str) -> Result<DeletedTicketDto, AppError> {
    let _guard = state.load_guard();
    let installer = installer_for(state)?;
    let installed = state
        .library
        .get_mod(id)
        .ok_or_else(|| AppError::new("not_found", "Мод не найден."))?;
    let target = crate::state::lock(&state.previews_by_id).get(id).cloned();
    if target
        .as_ref()
        .and_then(|target| target.sub_folder.as_ref())
        .is_some()
    {
        return Err(AppError::new(
            "repo",
            "Этот мод скачан в самой игре. Удалите его там, в «Репозитории».",
        ));
    }

    let in_mods = installer
        .layout()
        .mods_dir
        .join(&installed.file_name)
        .is_file();
    let in_disabled = installer
        .layout()
        .disabled_dir
        .join(&installed.file_name)
        .is_file();
    let was_in_disabled = !in_mods && in_disabled;
    let removed = installer.remove(&installed.file_name);
    if !removed.success {
        return Err(AppError::new("delete", removed.message));
    }
    let backup_id = removed
        .path
        .as_ref()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or_else(|| AppError::new("delete", "Копия удалённого мода не найдена."))?;

    let ticket = DeletedTicket {
        mod_id: installed.id.clone(),
        file_name: installed.file_name.clone(),
        backup_id: backup_id.clone(),
        was_in_disabled,
        was_enabled: installed.enabled,
    };
    let replaced = crate::state::lock(&state.deleted).insert(installed.id.clone(), ticket);
    if let Some(previous) = replaced {
        state.release_backup(&previous.backup_id);
    }
    state.hold_backup(&backup_id);

    Ok(DeletedTicketDto {
        id: installed.id,
        file_name: installed.file_name,
        backup_id,
        was_in_disabled,
        name: installed.name,
    })
}

fn undelete_mod(state: &AppState, id: &str) -> Result<(), AppError> {
    let _guard = state.load_guard();
    let ticket = crate::state::lock(&state.deleted)
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::new("not_found", "Удаление уже подтверждено."))?;
    let installer = installer_for(state)?;
    let result = installer.unremove(&ticket.file_name, &ticket.backup_id, ticket.was_in_disabled);
    if !result.success {
        return Err(AppError::new("undelete", result.message));
    }
    crate::state::lock(&state.deleted).remove(id);
    state.release_backup(&ticket.backup_id);
    if let Some(mut installed) = state.library.get_mod(id) {
        installed.enabled = !ticket.was_in_disabled;
        state.library.upsert_mod(installed);
    }
    Ok(())
}

fn forget_deleted(state: &AppState, id: &str) -> Result<(), AppError> {
    let _guard = state.load_guard();
    let Some(ticket) = crate::state::lock(&state.deleted).remove(id) else {
        return Ok(());
    };
    state.release_backup(&ticket.backup_id);
    let installer = installer_for(state)?;
    let exists = installer.layout().mods_dir.join(&ticket.file_name).exists()
        || installer
            .layout()
            .disabled_dir
            .join(&ticket.file_name)
            .exists();
    if !exists {
        state.library.remove_mod(id);
    }
    Ok(())
}

fn restore_deleted(
    state: &AppState,
    file_name: &str,
    backup_id: Option<&str>,
) -> Result<(), AppError> {
    let _guard = state.load_guard();
    let installer = installer_for(state)?;
    let item = installer
        .deleted_mods()
        .into_iter()
        .find(|item| item.file_name.eq_ignore_ascii_case(file_name))
        .ok_or_else(|| AppError::new("not_found", "Копий нет."))?;
    let result = versions::restore_deleted(
        &installer,
        state.library.as_ref(),
        &item,
        backup_id,
        Utc::now().fixed_offset(),
    );
    if result.success {
        Ok(())
    } else {
        Err(AppError::new("restore", result.message))
    }
}

fn versions_of(state: &AppState, id: &str) -> Result<Vec<VersionRowDto>, AppError> {
    let _guard = state.load_guard();
    let installed = state
        .library
        .get_mod(id)
        .ok_or_else(|| AppError::new("not_found", "Мод не найден."))?;
    if crate::state::lock(&state.previews_by_id)
        .get(id)
        .and_then(|target| target.sub_folder.as_ref())
        .is_some()
    {
        return Ok(Vec::new());
    }
    let installer = installer_for(state)?;
    Ok(
        versions::versions_for(&installer, state.library.as_ref(), &installed)
            .into_iter()
            .map(|version| VersionRowDto {
                backup_id: version.backup.id,
                file_name: version.backup.file_name,
                version: version.version,
                saved_at: version.backup.saved_at.map(|date| date.to_rfc3339()),
                size_bytes: version.backup.size_bytes,
            })
            .collect(),
    )
}

/// Undo уже использован: тост скрыт до ответа команды. Hold снимается при любом
/// завершении попытки, включая ошибку и панику, а не только после успешного restore.
struct ConsumedRestoreUndo<'a> {
    state: &'a AppState,
    backup_id: &'a str,
}

impl Drop for ConsumedRestoreUndo<'_> {
    fn drop(&mut self) {
        self.state.release_backup(self.backup_id);
    }
}

fn installed_path(installer: &ModInstaller, file_name: &str) -> Option<PathBuf> {
    [
        installer.layout().mods_dir.join(file_name),
        installer.layout().disabled_dir.join(file_name),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn sha256(path: &Path) -> Result<String, AppError> {
    bmm_packs::incoming::sha256_file(path)
        .map_err(|error| AppError::new("undo", format!("Не удалось проверить архив: {error}")))
}

fn restore_version(
    state: &AppState,
    id: &str,
    backup_id: &str,
    expected_token: Option<&str>,
) -> Result<Option<RestoreUndoTicketDto>, AppError> {
    let _guard = state.load_guard();
    // Hold удаления принадлежит отдельному тикету и не является Undo restore.
    undo_needs(state, backup_id)?;
    let undo = state
        .backup_is_held(backup_id)
        .then(|| ConsumedRestoreUndo { state, backup_id });
    let installed = state
        .library
        .get_mod(id)
        .ok_or_else(|| AppError::new("not_found", "Мод не найден."))?;
    let installer = installer_for(state)?;
    let versions = versions::versions_for(&installer, state.library.as_ref(), &installed);
    let version = versions
        .into_iter()
        .find(|version| version.backup.id == backup_id)
        .ok_or_else(|| AppError::new("not_found", "Такой копии нет."))?;
    if let Some(expected_token) = expected_token {
        let path = installed_path(&installer, &installed.file_name).ok_or_else(|| {
            AppError::new(
                "stale_undo",
                "Мод уже удалён — эта отмена больше не действует.",
            )
        })?;
        let current_token = sha256(&path)?;
        if current_token != expected_token {
            return Err(AppError::new(
                "stale_undo",
                "Мод уже обновлён или изменён — отмените последнее действие.",
            ));
        }
    }
    let backup_path = installer
        .backups(&version.backup.file_name)
        .into_iter()
        .find(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == backup_id)
        })
        .ok_or_else(|| AppError::new("not_found", "Такой копии нет."))?;
    let installed_token = sha256(&backup_path)?;
    let result =
        versions::restore_version(&installer, state.library.as_ref(), &installed, &version);
    if result.success {
        // Обратный restore создаёт копию текущей версии, но нового Undo у него нет.
        if undo.is_some() {
            return Ok(None);
        }
        let ticket = installer
            .list_backups(&installed.file_name)
            .into_iter()
            .next()
            .map(|entry| {
                state.hold_backup(&entry.id);
                RestoreUndoTicketDto {
                    backup_id: entry.id,
                    installed_token,
                }
            });
        Ok(ticket)
    } else {
        Err(AppError::new("restore", result.message))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use std::sync::Mutex;

    use bmm_domain::library::JsonLibraryStore;
    use bmm_domain::paths::AppPaths;
    use bmm_domain::settings::AppSettings;
    use bmm_domain::settings_store::SettingsStore;
    use bmm_packs::cache::PreviewCache;
    use bmm_packs::install::InstallOptions;

    use crate::commands::library::load_snapshot;

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
            live_lock: Mutex::new(()),
            previews_by_id: Mutex::new(HashMap::new()),
            pending: Mutex::new(HashMap::new()),
            bridge_sent: Mutex::new(None),
            held_backups: Arc::new(Mutex::new(std::collections::HashSet::new())),
            preview_gate: Arc::new(tokio::sync::Semaphore::new(4)),
            deleted: Mutex::new(HashMap::new()),
            content_cache: Mutex::new(HashMap::new()),
            search: None,
            update: None,
            app_update: None,
            search_cancel: Mutex::new(None),
            last_search: Mutex::new(None),
            browse: Mutex::new(None),
        });
        (temp, state)
    }

    fn make_zip(path: &std::path::Path) {
        make_zip_with(path, b"x");
    }

    fn make_zip_with(path: &std::path::Path, contents: &[u8]) {
        let file = fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "vehicles/car/a.jbeam",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(contents).unwrap();
        writer.finish().unwrap();
    }

    fn install_direct(state: &AppState, source: &std::path::Path, name: &str) {
        let installer = installer_for(state).unwrap();
        let result = installer.install_from_path(source, name, InstallOptions::default());
        assert!(result.success, "{:?}", result.message);
    }

    #[test]
    fn undone_deletion_puts_cards_back_in_their_places() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        for name in ["a.zip", "b.zip", "c.zip", "d.zip", "e.zip"] {
            install_direct(&state, &source, name);
        }
        let order = |state: &AppState| -> Vec<String> {
            load_snapshot(state)
                .unwrap()
                .mods
                .into_iter()
                .map(|card| card.file_name)
                .collect()
        };
        let before = order(&state);
        assert_eq!(before.len(), 5);
        let snapshot = load_snapshot(&state).unwrap();
        let id_of = |file: &str| {
            snapshot
                .mods
                .iter()
                .find(|card| card.file_name == file)
                .unwrap()
                .id
                .clone()
        };
        // Пачкой — две соседние и одна поодаль; отмена в обратном порядке.
        let ids: Vec<String> = [&before[1], &before[2], &before[4]]
            .into_iter()
            .map(|file| id_of(file))
            .collect();
        for id in &ids {
            delete_mod(&state, id).unwrap();
        }
        assert_eq!(order(&state).len(), 2);
        for id in ids.iter().rev() {
            undelete_mod(&state, id).unwrap();
        }
        assert_eq!(order(&state), before);
    }

    #[test]
    fn delete_undo_forget_and_restore_deleted() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "mod.zip");
        let snapshot = load_snapshot(&state).unwrap();
        assert_eq!(snapshot.mods.len(), 1);
        let id = snapshot.mods[0].id.clone();

        let _ticket = delete_mod(&state, &id).unwrap();
        assert!(state.library.get_mod(&id).is_some());
        let snapshot = load_snapshot(&state).unwrap();
        assert!(snapshot.mods.is_empty());
        assert!(snapshot.deleted.is_empty());

        undelete_mod(&state, &id).unwrap();
        assert!(state.deleted.lock().unwrap().is_empty());
        assert!(
            installer_for(&state)
                .unwrap()
                .layout()
                .mods_dir
                .join("mod.zip")
                .is_file()
        );
        let snapshot = load_snapshot(&state).unwrap();
        assert_eq!(snapshot.mods.len(), 1);

        let ticket = delete_mod(&state, &id).unwrap();
        assert!(ticket.backup_id.ends_with("mod.zip"));
        forget_deleted(&state, &id).unwrap();
        assert!(state.library.get_mod(&id).is_none());
        let snapshot = load_snapshot(&state).unwrap();
        assert_eq!(snapshot.deleted.len(), 1);
        assert_eq!(snapshot.deleted[0].file_name, "mod.zip");

        restore_deleted(&state, "mod.zip", None).unwrap();
        let snapshot = load_snapshot(&state).unwrap();
        assert_eq!(snapshot.mods.len(), 1);
        assert!(snapshot.deleted.is_empty());
    }

    #[test]
    fn deleting_a_disabled_mod_and_undoing_keeps_it_disabled() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "off.zip");
        let id = load_snapshot(&state).unwrap().mods[0].id.clone();
        crate::commands::library::toggle_mod(&state, &id, false).unwrap();
        assert!(!load_snapshot(&state).unwrap().mods[0].enabled);

        let ticket = delete_mod(&state, &id).unwrap();
        assert!(ticket.was_in_disabled);
        assert!(state.backup_is_held(&ticket.backup_id));
        undelete_mod(&state, &id).unwrap();
        assert!(!state.backup_is_held(&ticket.backup_id));
        let snapshot = load_snapshot(&state).unwrap();
        assert_eq!(snapshot.mods.len(), 1);
        assert!(!snapshot.mods[0].enabled, "отмена не включает мод сама");
        assert!(delete_mod(&state, "нет-такого").is_err());
        assert!(
            undelete_mod(&state, &id).is_err(),
            "второй раз отменять нечего"
        );
    }

    #[test]
    fn failed_undelete_keeps_hold_only_while_its_ticket_exists() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "mod.zip");
        let id = load_snapshot(&state).unwrap().mods[0].id.clone();
        let ticket = delete_mod(&state, &id).unwrap();

        install_direct(&state, &source, "mod.zip");
        assert!(undelete_mod(&state, &id).is_err());
        assert!(crate::state::lock(&state.deleted).contains_key(&id));
        assert!(state.backup_is_held(&ticket.backup_id));
        assert!(delete_version(&state, &id, &ticket.backup_id).is_err());

        forget_deleted(&state, &id).unwrap();
        assert!(!state.backup_is_held(&ticket.backup_id));
        assert!(crate::state::lock(&state.held_backups).is_empty());
        delete_version(&state, &id, &ticket.backup_id).unwrap();
    }

    #[test]
    fn replacing_delete_ticket_releases_previous_backup_hold() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "mod.zip");
        let id = load_snapshot(&state).unwrap().mods[0].id.clone();

        let first = delete_mod(&state, &id).unwrap();
        assert!(delete_mod(&state, &id).is_err());
        assert!(state.backup_is_held(&first.backup_id));
        assert_eq!(
            crate::state::lock(&state.deleted)
                .get(&id)
                .unwrap()
                .backup_id,
            first.backup_id
        );

        // Файл появился снова, пока старое удаление ещё не подтверждено.
        install_direct(&state, &source, "mod.zip");
        let second = delete_mod(&state, &id).unwrap();
        assert_ne!(first.backup_id, second.backup_id);
        assert_eq!(
            crate::state::lock(&state.deleted)
                .get(&id)
                .unwrap()
                .backup_id,
            second.backup_id
        );
        assert!(!state.backup_is_held(&first.backup_id));
        assert!(state.backup_is_held(&second.backup_id));

        undelete_mod(&state, &id).unwrap();
        assert!(crate::state::lock(&state.held_backups).is_empty());
        delete_version(&state, &id, &first.backup_id).unwrap();
    }

    #[test]
    fn versions_and_restore_version_command() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "mod_v1.zip");
        let installer = installer_for(&state).unwrap();
        let update = installer.update_from_path(
            "mod_v1.zip",
            &source,
            "mod_v2.zip",
            InstallOptions::default(),
        );
        assert!(update.install.success, "{:?}", update.install.message);

        let snapshot = load_snapshot(&state).unwrap();
        let id = snapshot
            .mods
            .iter()
            .find(|item| item.file_name == "mod_v2.zip")
            .unwrap()
            .id
            .clone();
        let rows = versions_of(&state, &id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].file_name, "mod_v1.zip");

        restore_version(&state, &id, &rows[0].backup_id, None).unwrap();
        let snapshot = load_snapshot(&state).unwrap();
        assert!(
            snapshot
                .mods
                .iter()
                .any(|item| item.file_name == "mod_v1.zip")
        );
        let undo_rows = versions_of(&state, &id).unwrap();
        let undo = undo_rows
            .iter()
            .find(|row| row.file_name == "mod_v2.zip")
            .unwrap();
        assert!(state.backup_is_held(&undo.backup_id));
        assert!(delete_version(&state, &id, &undo.backup_id).is_err());
        let next = installer.update_from_path(
            "mod_v1.zip",
            &source,
            "mod_v3.zip",
            InstallOptions::default(),
        );
        assert!(next.install.success, "{:?}", next.install.message);
        assert!(
            versions_of(&state, &id)
                .unwrap()
                .iter()
                .any(|row| row.backup_id == undo.backup_id),
            "копия для Undo переживает следующую ротацию"
        );
    }

    #[test]
    fn restore_returns_a_ticket_that_commit_releases() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "mod_v1.zip");
        let installer = installer_for(&state).unwrap();
        let update = installer.update_from_path(
            "mod_v1.zip",
            &source,
            "mod_v2.zip",
            InstallOptions::default(),
        );
        assert!(update.install.success, "{:?}", update.install.message);
        let id = load_snapshot(&state).unwrap().mods[0].id.clone();
        let previous = versions_of(&state, &id).unwrap().remove(0);

        let ticket = restore_version(&state, &id, &previous.backup_id, None)
            .unwrap()
            .expect("обычный restore создаёт Undo");
        assert!(state.backup_is_held(&ticket.backup_id));
        assert!(delete_version(&state, &id, &ticket.backup_id).is_err());

        commit_restore_undo(&state, &ticket.backup_id);

        assert!(!state.backup_is_held(&ticket.backup_id));
        assert!(delete_version(&state, &id, &ticket.backup_id).is_ok());
    }

    #[test]
    fn stale_restore_undo_cannot_overwrite_a_newer_update() {
        let (temp, state) = setup();
        let v1 = temp.path().join("v1.zip");
        let v2 = temp.path().join("v2.zip");
        let v3 = temp.path().join("v3.zip");
        make_zip_with(&v1, b"version-one");
        make_zip_with(&v2, b"version-two");
        make_zip_with(&v3, b"version-three");
        install_direct(&state, &v1, "mod_v1.zip");
        let installer = installer_for(&state).unwrap();
        let update =
            installer.update_from_path("mod_v1.zip", &v2, "mod_v2.zip", InstallOptions::default());
        assert!(update.install.success, "{:?}", update.install.message);
        let id = load_snapshot(&state).unwrap().mods[0].id.clone();
        let previous = versions_of(&state, &id).unwrap().remove(0);
        let ticket = restore_version(&state, &id, &previous.backup_id, None)
            .unwrap()
            .expect("обычный restore создаёт Undo");
        assert!(!ticket.installed_token.is_empty());

        let update =
            installer.update_from_path("mod_v1.zip", &v3, "mod_v3.zip", InstallOptions::default());
        assert!(update.install.success, "{:?}", update.install.message);
        let error = restore_version(
            &state,
            &id,
            &ticket.backup_id,
            Some(&ticket.installed_token),
        )
        .unwrap_err();

        assert_eq!(error.code, "stale_undo");
        assert_eq!(
            load_snapshot(&state).unwrap().mods[0].file_name,
            "mod_v3.zip"
        );
        assert!(!state.backup_is_held(&ticket.backup_id));
    }

    #[test]
    fn restore_undo_cycles_release_backup_holds() {
        let (temp, state) = setup();
        let mut settings = state.settings.load().settings;
        settings.keep_backups = 1;
        state.settings.save(&settings).unwrap();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "mod_v1.zip");
        let installer = installer_for(&state).unwrap();
        let update = installer.update_from_path(
            "mod_v1.zip",
            &source,
            "mod_v2.zip",
            InstallOptions::default(),
        );
        assert!(update.install.success, "{:?}", update.install.message);
        let id = load_snapshot(&state).unwrap().mods[0].id.clone();

        for _ in 0..3 {
            let rows = versions_of(&state, &id).unwrap();
            assert_eq!(rows.len(), 1, "ротация оставляет только рабочую копию");
            let previous = &rows[0];
            assert_eq!(previous.file_name, "mod_v1.zip");
            assert!(!state.backup_is_held(&previous.backup_id));

            let ticket = restore_version(&state, &id, &previous.backup_id, None)
                .unwrap()
                .expect("создан Undo");
            assert!(state.backup_is_held(&ticket.backup_id));
            let undo_rows = versions_of(&state, &id).unwrap();
            let undo = undo_rows
                .iter()
                .find(|row| row.file_name == "mod_v2.zip")
                .unwrap();
            assert!(state.backup_is_held(&undo.backup_id));
            assert!(delete_version(&state, &id, &undo.backup_id).is_err());

            let undone =
                restore_version(&state, &id, &undo.backup_id, Some(&ticket.installed_token))
                    .unwrap();
            assert!(undone.is_none(), "обратный restore не создаёт новый Undo");
            assert!(!state.backup_is_held(&undo.backup_id));
            assert!(crate::state::lock(&state.held_backups).is_empty());
            assert_eq!(
                load_snapshot(&state).unwrap().mods[0].file_name,
                "mod_v2.zip"
            );
        }
    }

    #[test]
    fn restore_undo_hold_survives_rotation_until_consumed() {
        let (temp, state) = setup();
        let mut settings = state.settings.load().settings;
        settings.keep_backups = 1;
        state.settings.save(&settings).unwrap();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "mod.zip");
        let installer = installer_for(&state).unwrap();
        let update =
            installer.update_from_path("mod.zip", &source, "mod.zip", InstallOptions::default());
        assert!(update.install.success, "{:?}", update.install.message);
        let id = load_snapshot(&state).unwrap().mods[0].id.clone();
        let previous = versions_of(&state, &id).unwrap().remove(0);
        let ticket = restore_version(&state, &id, &previous.backup_id, None)
            .unwrap()
            .unwrap();
        let undo = versions_of(&state, &id).unwrap().remove(0);
        assert!(state.backup_is_held(&undo.backup_id));

        let update =
            installer.update_from_path("mod.zip", &source, "mod.zip", InstallOptions::default());
        assert!(update.install.success, "{:?}", update.install.message);
        let during = installer.list_backups("mod.zip");
        assert_eq!(during.len(), 2, "held-копия исключена из ротации");
        assert!(during.iter().any(|entry| entry.id == undo.backup_id));

        restore_version(&state, &id, &undo.backup_id, Some(&ticket.installed_token)).unwrap();
        assert!(!state.backup_is_held(&undo.backup_id));
        let after = installer.list_backups("mod.zip");
        assert_eq!(after.len(), 1, "после Undo лишняя копия снова удаляется");
        assert!(after.iter().all(|entry| entry.id != undo.backup_id));
    }

    #[test]
    fn failed_restore_undo_releases_backup_hold() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "mod_v1.zip");
        let installer = installer_for(&state).unwrap();
        let update = installer.update_from_path(
            "mod_v1.zip",
            &source,
            "mod_v2.zip",
            InstallOptions::default(),
        );
        assert!(update.install.success, "{:?}", update.install.message);
        let id = load_snapshot(&state).unwrap().mods[0].id.clone();
        let previous = versions_of(&state, &id).unwrap().remove(0);
        let ticket = restore_version(&state, &id, &previous.backup_id, None)
            .unwrap()
            .unwrap();
        let undo = versions_of(&state, &id)
            .unwrap()
            .into_iter()
            .find(|row| row.file_name == "mod_v2.zip")
            .unwrap();
        assert!(state.backup_is_held(&undo.backup_id));

        let conflict = installer.layout().mods_dir.join("mod_v2.zip");
        fs::write(&conflict, b"conflict").unwrap();
        let error = restore_version(&state, &id, &undo.backup_id, Some(&ticket.installed_token))
            .unwrap_err();
        assert_eq!(error.code, "restore");
        assert!(!state.backup_is_held(&undo.backup_id));
        assert!(crate::state::lock(&state.held_backups).is_empty());

        fs::remove_file(conflict).unwrap();
        delete_version(&state, &id, &undo.backup_id).unwrap();
    }

    #[test]
    fn deleted_copy_is_not_a_past_version_of_the_other_copy() {
        // Две копии одного мода; копия «(1)» помнит имя основного файла. Удалили
        // основной — у «(1)» в «Прошлых версиях» появлялась «Версия 1.0», и «Вернуть»
        // возвращало удалённый файл, убирая в копии саму «(1)».
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "acm_v1-0.zip");
        install_direct(&state, &source, "acm_v1-0 (1).zip");
        let snapshot = load_snapshot(&state).unwrap();
        let id_of = |file: &str| {
            snapshot
                .mods
                .iter()
                .find(|item| item.file_name == file)
                .unwrap()
                .id
                .clone()
        };
        let (first, copy) = (id_of("acm_v1-0.zip"), id_of("acm_v1-0 (1).zip"));
        state.library.add_file_history(&copy, "acm_v1-0.zip");
        delete_mod(&state, &first).unwrap();
        load_snapshot(&state).unwrap();
        assert!(versions_of(&state, &copy).unwrap().is_empty());
        // Удаление остаётся отменяемым тостом.
        undelete_mod(&state, &first).unwrap();
        let mods_dir = installer_for(&state).unwrap().layout().mods_dir.clone();
        assert!(mods_dir.join("acm_v1-0.zip").is_file());
        assert!(mods_dir.join("acm_v1-0 (1).zip").is_file());
    }

    #[test]
    fn past_copy_is_deleted_for_good() {
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "fx.zip");
        install_direct(&state, &source, "fx (1).zip");
        let snapshot = load_snapshot(&state).unwrap();
        let id_of = |file: &str| {
            state
                .library
                .mods()
                .into_iter()
                .find(|item| item.file_name == file)
                .unwrap()
                .id
        };
        let _ = snapshot;
        let copy = id_of("fx (1).zip");
        delete_mod(&state, &copy).unwrap();
        forget_deleted(&state, &copy).unwrap();
        load_snapshot(&state).unwrap();
        let main = id_of("fx.zip");
        let rows = versions_of(&state, &main).unwrap();
        assert_eq!(rows.len(), 1, "удалённая копия видна как прошлая");
        delete_version(&state, &main, &rows[0].backup_id).unwrap();
        assert!(versions_of(&state, &main).unwrap().is_empty());
        assert!(
            delete_version(&state, &main, "чужая").is_err(),
            "удаляем только показанное у мода"
        );
    }

    #[test]
    fn copy_held_by_undo_is_neither_deleted_nor_returned() {
        // Пока висит «Отменить», копия удалённого мода видна в его же «Прошлых версиях»:
        // стёртая или возвращённая до конца тоста, она ломала бы «Отменить».
        let (temp, state) = setup();
        let source = temp.path().join("source.zip");
        make_zip(&source);
        install_direct(&state, &source, "fx.zip");
        install_direct(&state, &source, "fx (1).zip");
        load_snapshot(&state).unwrap();
        let id_of = |file: &str| {
            state
                .library
                .mods()
                .into_iter()
                .find(|item| item.file_name == file)
                .unwrap()
                .id
        };
        let copy = id_of("fx (1).zip");
        let ticket = delete_mod(&state, &copy).unwrap();
        let rows = versions_of(&state, &copy).unwrap();
        assert!(rows.iter().any(|row| row.backup_id == ticket.backup_id));
        assert!(delete_version(&state, &copy, &ticket.backup_id).is_err());
        assert!(restore_version(&state, &copy, &ticket.backup_id, None).is_err());
        undelete_mod(&state, &copy).unwrap();
        assert!(
            state
                .library
                .mods()
                .iter()
                .any(|item| item.file_name == "fx (1).zip")
        );
    }
}

use std::sync::Arc;

use chrono::Utc;

use bmm_domain::clock::system_clock;
use bmm_domain::library::LibraryStore;
use bmm_packs::install::ModInstaller;
use bmm_packs::scan::ModLayout;
use bmm_packs::versions;

use crate::commands::library::{DeletedTicketDto, VersionRowDto, resolve_folder};
use crate::error::AppError;
use crate::state::{AppState, DeletedTicket};

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
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || restore_version(&state, &id, &backup_id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
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
    Ok(ModInstaller::new(
        layout,
        system_clock(),
        Arc::new(move || keep),
    ))
}

fn delete_mod(state: &AppState, id: &str) -> Result<DeletedTicketDto, AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let installer = installer_for(state)?;
    let installed = state
        .library
        .get_mod(id)
        .ok_or_else(|| AppError::new("not_found", "Мод не найден."))?;
    let target = state.previews_by_id.lock().unwrap().get(id).cloned();
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
    state
        .deleted
        .lock()
        .unwrap()
        .insert(installed.id.clone(), ticket);

    Ok(DeletedTicketDto {
        id: installed.id,
        file_name: installed.file_name,
        backup_id,
        was_in_disabled,
        name: installed.name,
    })
}

fn undelete_mod(state: &AppState, id: &str) -> Result<(), AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let ticket = state
        .deleted
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::new("not_found", "Удаление уже подтверждено."))?;
    let installer = installer_for(state)?;
    let result = installer.unremove(&ticket.file_name, &ticket.backup_id, ticket.was_in_disabled);
    if !result.success {
        return Err(AppError::new("undelete", result.message));
    }
    state.deleted.lock().unwrap().remove(id);
    if let Some(mut installed) = state.library.get_mod(id) {
        installed.enabled = !ticket.was_in_disabled;
        state.library.upsert_mod(installed);
    }
    Ok(())
}

fn forget_deleted(state: &AppState, id: &str) -> Result<(), AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let Some(ticket) = state.deleted.lock().unwrap().remove(id) else {
        return Ok(());
    };
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
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
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
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let installed = state
        .library
        .get_mod(id)
        .ok_or_else(|| AppError::new("not_found", "Мод не найден."))?;
    if state
        .previews_by_id
        .lock()
        .unwrap()
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

fn restore_version(state: &AppState, id: &str, backup_id: &str) -> Result<(), AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
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
    let result = versions::restore_version(
        &installer,
        state.library.as_ref(),
        &installed,
        &version,
        Utc::now().fixed_offset(),
    );
    if result.success {
        Ok(())
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
            browse: Mutex::new(None),
        });
        (temp, state)
    }

    fn make_zip(path: &std::path::Path) {
        let file = fs::File::create(path).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        writer
            .start_file(
                "vehicles/car/a.jbeam",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer.write_all(b"x").unwrap();
        writer.finish().unwrap();
    }

    fn install_direct(state: &AppState, source: &std::path::Path, name: &str) {
        let installer = installer_for(state).unwrap();
        let result = installer.install_from_path(source, name, InstallOptions::default());
        assert!(result.success, "{:?}", result.message);
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
        undelete_mod(&state, &id).unwrap();
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

        restore_version(&state, &id, &rows[0].backup_id).unwrap();
        let snapshot = load_snapshot(&state).unwrap();
        assert!(
            snapshot
                .mods
                .iter()
                .any(|item| item.file_name == "mod_v1.zip")
        );
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
}

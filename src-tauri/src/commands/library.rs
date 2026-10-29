use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use chrono::Utc;
use tauri::Emitter;
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
use bmm_packs::incoming;
use bmm_packs::install::{InstallOptions, ModInstaller};
use bmm_packs::overview::{self, ArchiveOverview};
use bmm_packs::preview;
use bmm_packs::scan::{self, ModLayout};
use bmm_packs::versions;

use crate::error::AppError;
use crate::state::{AppState, PreviewTarget};

mod types;
pub use types::*;

#[tauri::command]
pub async fn library_load(
    app: tauri::AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<LibrarySnapshot, AppError> {
    let state = state.inner().clone();
    let task_state = state.clone();
    let snapshot = tauri::async_runtime::spawn_blocking(move || load_snapshot(&task_state))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))??;
    spawn_title_fill(app, state);
    Ok(snapshot)
}

/// «Исправить»: переупаковать архив, который игра не загрузит (лишняя папка или папка
/// машины без `vehicles/`). Прежний файл уходит в копии — его можно вернуть.
#[tauri::command]
pub async fn mod_fix_packaging(
    state: tauri::State<'_, Arc<AppState>>,
    id: String,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || fix_packaging(&state, &id))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

fn fix_packaging(state: &AppState, id: &str) -> Result<(), AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let target = state
        .previews_by_id
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or_else(|| AppError::new("fix", "Мод не найден. Обновите список."))?;
    if target.sub_folder.is_some() {
        return Err(AppError::new(
            "fix",
            "Моды из репозитория игры не переупаковываем.",
        ));
    }
    let work = state.paths.root().join("work").join("fix");
    let fixed = incoming::fix_packaging(&target.path, &work)
        .map_err(|message| AppError::new("fix", message))?;
    let installer = crate::commands::versions::installer_for(state)?;
    let result = installer.update_from_path(
        &target.file_name,
        &fixed,
        &target.file_name,
        InstallOptions {
            allow_warnings: true,
            ..InstallOptions::default()
        },
    );
    let _ = std::fs::remove_file(&fixed);
    if !result.install.success {
        return Err(AppError::new(
            "fix",
            result
                .install
                .message
                .unwrap_or_else(|| "Не удалось переупаковать архив.".to_string()),
        ));
    }
    // Тот же мод, только упакован иначе: размер файла сменился, а версия — нет. Без этого
    // сверка с папкой приняла бы новый размер за чужую замену файла и забыла бы версию.
    if let Some(mut installed) = state.library.get_mod(id)
        && let Some(path) = result.install.path.as_ref()
        && let Ok(meta) = std::fs::metadata(path)
    {
        installed.size_bytes = i64::try_from(meta.len()).ok();
        state.library.upsert_mod(installed);
    }
    Ok(())
}

/// Оставляет в `mod_info` только поля, которые читает окно: остальное (описания,
/// картинки, длинные списки) раздувало кеш до мегабайт на мод.
fn compact_mod_info(value: &serde_json::Value) -> serde_json::Value {
    if value.is_null() {
        return serde_json::Value::Null;
    }
    let Some(object) = value.as_object() else {
        return serde_json::Value::Null;
    };
    let Some(id) = object.get("id").and_then(serde_json::Value::as_str) else {
        return serde_json::Value::Null;
    };
    let data = object.get("data").and_then(serde_json::Value::as_object);
    let mut picked = serde_json::Map::new();
    for key in [
        "tag_line",
        "title",
        "name",
        "version",
        "version_string",
        "username",
        "author",
        "last_update",
    ] {
        let Some(value) = data.and_then(|data| data.get(key)) else {
            continue;
        };
        let short = value
            .as_str()
            .is_some_and(|text| text.chars().count() <= 300);
        if short {
            picked.insert(key.to_string(), value.clone());
        }
    }
    if let Some(value) = data.and_then(|data| data.get("resource_id"))
        && (value.is_number() || value.is_string())
    {
        picked.insert("resource_id".to_string(), value.clone());
    }
    serde_json::json!({ "id": id, "data": picked })
}

/// Короткая подпись из `mod_info`: по ней видно, что «машина» на самом деле
/// человечек-реквизит (`75KG Ragdoll Humanoid`).
pub(crate) fn mod_info_hint(value: &serde_json::Value) -> Option<&str> {
    let data = value.get("data")?;
    data.get("tag_line")
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .or_else(|| data.get("title").and_then(serde_json::Value::as_str))
}

/// Роль мода по составу архива и группы повторов (см. [`bmm_packs::duplicates`]).
/// Состав — из кэша; ещё не прочитанные архивы не участвуют, пометка появится после
/// фонового чтения. В группе основной — самый свежий: в списке видна только его
/// карточка с числом копий, остальные — в его панели.
fn mark_duplicates(
    mut mods: Vec<ModCardDto>,
    preview_keys: &HashMap<String, String>,
    contents: &BTreeMap<String, Vec<String>>,
    infos: &BTreeMap<String, serde_json::Value>,
    modified: &HashMap<String, i64>,
) -> Vec<ModCardDto> {
    let empty: Vec<String> = Vec::new();
    let content_of = |id: &str| {
        preview_keys
            .get(id)
            .and_then(|key| contents.get(key))
            .unwrap_or(&empty)
    };
    let hint_of = |id: &str| {
        preview_keys
            .get(id)
            .and_then(|key| infos.get(key))
            .and_then(mod_info_hint)
    };
    for card in &mut mods {
        card.role =
            bmm_packs::duplicates::role_of_with_hint(content_of(&card.id), hint_of(&card.id))
                .map(|role| role.code().to_string());
    }
    let items: Vec<bmm_packs::duplicates::DupItem> = mods
        .iter()
        .filter(|card| !card.deleted)
        .filter_map(|card| {
            let content = content_of(&card.id);
            (!content.is_empty()).then_some(bmm_packs::duplicates::DupItem {
                id: &card.id,
                file_name: &card.file_name,
                size: card.size_bytes,
                content,
                version: card.version.as_deref(),
                modified: modified.get(&card.id).copied(),
            })
        })
        .collect();
    let found = bmm_packs::duplicates::find(&items);
    drop(items);
    let names: HashMap<String, String> = mods
        .iter()
        .map(|card| (card.id.clone(), card.name.clone()))
        .collect();
    let enabled: HashMap<String, bool> = mods
        .iter()
        .map(|card| (card.id.clone(), card.enabled))
        .collect();
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();
    let mut ids: Vec<&String> = found.keys().collect();
    ids.sort();
    for id in ids {
        groups
            .entry(found[id].of.clone())
            .or_default()
            .push(id.clone());
    }
    for card in &mut mods {
        if let Some(duplicate) = found.get(&card.id) {
            card.duplicate_of = Some(DuplicateDto {
                id: duplicate.of.clone(),
                name: names.get(&duplicate.of).cloned().unwrap_or_default(),
                exact: duplicate.exact,
            });
        }
        if let Some(members) = groups.get(&card.id) {
            let enabled_count = std::iter::once(&card.id)
                .chain(members.iter())
                .filter(|id| enabled.get(*id).copied().unwrap_or(false))
                .count();
            card.group = members.clone();
            card.group_conflict = enabled_count > 1;
        }
    }
    mods
}

static TITLES_BUSY: AtomicBool = AtomicBool::new(false);

/// Названия модов из их архивов (`info.json`) читаются в фоне и кэшируются: первый
/// запуск с сотнями модов не ждёт их, а карточки обновляются событием `library://titles`.
fn spawn_title_fill(app: tauri::AppHandle, state: Arc<AppState>) {
    if TITLES_BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        let added = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let added = fill_titles(&state);
            prune_previews(&state);
            added
        }))
        .unwrap_or(0);
        TITLES_BUSY.store(false, Ordering::SeqCst);
        if added > 0 {
            let _ = app.emit("library://titles", added);
        }
    });
}

/// Убирает из кэша картинки и сведения модов, которых больше нет на диске.
/// Список ещё не построен (папка не найдена) — ничего не трогаем, иначе стёрли бы всё.
pub(crate) fn prune_previews(state: &AppState) {
    let live: std::collections::HashSet<String> = state
        .previews_by_id
        .lock()
        .unwrap()
        .values()
        .map(|target| target.key.clone())
        .collect();
    if !live.is_empty() {
        state.previews.prune(&live);
    }
}

/// Дочитывает названия, которых ещё нет в кэше. Возвращает, сколько записали.
pub(crate) fn fill_titles(state: &AppState) -> usize {
    let targets: Vec<PreviewTarget> = state
        .previews_by_id
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect();
    let known = state.previews.titles_shared();
    let known_problems = state.previews.problems_shared();
    let known_contents = state.previews.contents_shared();
    let known_infos = state.previews.mod_infos_shared();
    let mut infos: Vec<(String, serde_json::Value)> = Vec::new();
    // Старые записи хранили весь info.json; сжимаем их при первом чтении.
    for (key, value) in known_infos.iter() {
        let compact = compact_mod_info(value);
        if compact != *value {
            infos.push((key.clone(), compact));
        }
    }
    let mut found: Vec<(String, Option<String>)> = Vec::new();
    let mut problems: Vec<(String, Option<String>)> = Vec::new();
    let mut contents: Vec<(String, Vec<String>)> = Vec::new();
    let mut written = 0usize;
    let mut flush = |found: &mut Vec<(String, Option<String>)>,
                     problems: &mut Vec<(String, Option<String>)>,
                     contents: &mut Vec<(String, Vec<String>)>,
                     infos: &mut Vec<(String, serde_json::Value)>| {
        if state.previews.put_mod_infos(infos).is_ok() {
            written += infos.len();
        }
        infos.clear();
        if state.previews.put_titles(found).is_ok() {
            written += found.len();
        }
        if state.previews.put_problems(problems).is_ok() {
            written += problems.len();
        }
        if state.previews.put_contents(contents).is_ok() {
            written += contents.len();
        }
        found.clear();
        problems.clear();
        contents.clear();
    };
    for target in targets {
        let need_title =
            !known.contains_key(&target.key) && !found.iter().any(|(key, _)| *key == target.key);
        let need_problem = !known_problems.contains_key(&target.key)
            && !problems.iter().any(|(key, _)| *key == target.key);
        let need_contents = !known_contents.contains_key(&target.key)
            && !contents.iter().any(|(key, _)| *key == target.key);
        let need_info = !known_infos.contains_key(&target.key)
            && !infos.iter().any(|(key, _)| *key == target.key)
            && target.path.is_file();
        if !(need_title || need_problem || need_contents || need_info) {
            continue;
        }
        let index = match bmm_packs::zip_index::read_index(&target.path) {
            Ok(index) => index,
            Err(bmm_packs::zip_index::ZipError::Zip(_)) => {
                // Повреждённый архив — это известный ответ, а не занятость файла.
                if need_title {
                    found.push((target.key.clone(), None));
                }
                if need_problem {
                    problems.push((
                        target.key.clone(),
                        Some(
                            bmm_packs::incoming::PackagingProblem::NotMod
                                .code()
                                .to_string(),
                        ),
                    ));
                }
                continue;
            }
            // Архив занят или пропал — спросим при следующей загрузке.
            Err(_) => continue,
        };
        if need_title {
            found.push((
                target.key.clone(),
                overview::archive_info_from_index(&target.path, &index).title,
            ));
        }
        if need_problem {
            problems.push((
                target.key.clone(),
                incoming::archive_problem_from_index(&index).map(|p| p.code().to_string()),
            ));
        }
        // Состав архива — для пометки повторов («weather_fx (1).zip» рядом с «weather_fx.zip»).
        if need_contents {
            contents.push((
                target.key.clone(),
                incoming::content_keys(&index).into_iter().collect(),
            ));
        }
        // mod_info из архива — страница в репозитории BeamNG и версия ещё до запуска игры.
        if need_info {
            match overview::try_mod_info_from_index(&target.path, &index) {
                Ok(Some((id, data))) => {
                    infos.push((
                        target.key.clone(),
                        compact_mod_info(&serde_json::json!({ "id": id, "data": data })),
                    ));
                }
                // Записи нет — это известный ответ, кэшируем, чтобы не читать архив снова.
                Ok(None) => infos.push((target.key.clone(), serde_json::Value::Null)),
                // Архив занят/недочитан: не кэшируем, попробуем в следующий раз.
                Err(_) => {}
            }
        }
        if found.len() + problems.len() + contents.len() + infos.len() >= 100 {
            flush(&mut found, &mut problems, &mut contents, &mut infos);
        }
    }
    flush(&mut found, &mut problems, &mut contents, &mut infos);
    written
}

/// Автор мода из его архива (`info.json`) — для поиска страницы. Не кэшируется:
/// нужен только при подборе, не больше нескольких десятков модов за раз.
pub(crate) fn archive_author_of(state: &AppState, mod_id: &str) -> Option<String> {
    let target = state.previews_by_id.lock().unwrap().get(mod_id).cloned()?;
    overview::archive_info(&target.path).ok()?.author
}

/// Название мода из его архива для поиска страницы: из кэша, иначе прочитать сейчас.
pub(crate) fn archive_title_of(state: &AppState, mod_id: &str) -> Option<String> {
    let target = state.previews_by_id.lock().unwrap().get(mod_id).cloned()?;
    if let Some(known) = state.previews.title(&target.key) {
        return known;
    }
    let title = overview::archive_title(&target.path).ok()?;
    let _ = state.previews.put_title(&target.key, title.as_deref());
    title
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

mod snapshot;
pub(crate) use snapshot::{load_snapshot, resolve_folder};

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
    let outcome = db.set_active(&changes, process::is_running);
    if outcome.success()
        || matches!(
            outcome.outcome,
            SetActiveOutcome::NotInDb | SetActiveOutcome::NoDb
        )
    {
        state.pending.lock().unwrap().clear();
        let _ = state.save_pending();
    }
}

fn apply_repo_info(
    state: &AppState,
    db: &bmm_game::db::GameDbReadResult,
    disks_by_file: &HashMap<String, &DiskMod>,
) {
    let infos = state.previews.mod_infos_shared();
    let _batch = state.library.batch();
    for installed in state.library.mods() {
        if state.library.is_checking_declined(&installed.id) {
            continue;
        }
        // Что игра записала в db.json, иначе — mod_info из самого архива (игру с этим
        // модом ещё не запускали).
        let from_db = if db.unreadable {
            None
        } else {
            db.find(&installed.file_name)
                .and_then(|entry| entry.repo.clone())
        };
        let from_archive = || {
            let disk = disks_by_file.get(&installed.file_name.to_lowercase())?;
            let key = PreviewCache::key(&installed.file_name, disk.size_bytes, disk.modified_utc);
            let value = infos.get(&key)?;
            let id = value.get("id")?.as_str()?;
            Some(bmm_game::db::RepoInfo::from_mod_data(
                id,
                value.get("data").unwrap_or(&serde_json::Value::Null),
            ))
        };
        let Some(repo) = from_db.or_else(from_archive) else {
            continue;
        };
        let repo = &repo;
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
        None => {
            // Игра выключает мод записью active=false в db.json, файл при этом остаётся
            // в mods. В disabled его переносит только сам менеджер.
            let disabled = layout.disabled_dir.join(&disk.file_name);
            if disabled.is_file() {
                disabled
            } else {
                layout.mods_dir.join(&disk.file_name)
            }
        }
    }
}

pub(crate) fn toggle_mod(
    state: &AppState,
    id: &str,
    enabled: bool,
) -> Result<ToggleResult, AppError> {
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
    // Обычный путь берём из уже построенного снимка: полный scan_mods на каждый
    // toggle — это O(всех модов) на клик и минуты на массовых действиях.
    let disk = state
        .previews_by_id
        .lock()
        .unwrap()
        .get(id)
        .filter(|target| target.file_name.eq_ignore_ascii_case(&installed.file_name))
        .and_then(|target| {
            let metadata = std::fs::metadata(&target.path).ok()?;
            Some(DiskMod {
                file_name: target.file_name.clone(),
                enabled: target.sub_folder.is_some()
                    || target.path.parent() == Some(layout.mods_dir.as_path()),
                size_bytes: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
                sub_folder: target.sub_folder.clone(),
                modified_utc: metadata
                    .modified()
                    .ok()
                    .map(chrono::DateTime::<chrono::Utc>::from),
            })
        })
        .or_else(|| {
            scan::scan_mods(&layout)
                .mods
                .into_iter()
                .find(|item| item.file_name.eq_ignore_ascii_case(&installed.file_name))
        })
        .ok_or_else(|| AppError::new("missing", "Файл мода не найден."))?;

    if process::is_running() {
        if let Some(result) = try_live_toggle(state, &mods_dir, &installed, &disk, enabled) {
            return Ok(result);
        }
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
        let result = db.set_active(&[(&disk.file_name, true)], process::is_running);
        match result.outcome {
            SetActiveOutcome::Done
            | SetActiveOutcome::Unchanged
            | SetActiveOutcome::NotInDb
            | SetActiveOutcome::NoDb => {}
            SetActiveOutcome::GameRunning
            | SetActiveOutcome::ChangedMeanwhile
            | SetActiveOutcome::IoError
            | SetActiveOutcome::Unreadable => {
                // Файл уже в mods, но игра/диск не дали отметить его активным.
                // Возвращаем в disabled, иначе карточка врёт: «включён», а игра не видит.
                let rolled = installer.disable(&disk.file_name);
                let detail = result
                    .message
                    .unwrap_or_else(|| "игра не подтвердила включение".to_string());
                if rolled.success {
                    return Err(AppError::new(
                        "game",
                        format!("{detail}; файл возвращён в выключенные."),
                    ));
                }
                return Err(AppError::new(
                    "game",
                    format!("{detail}; файл остался в mods — проверьте вручную."),
                ));
            }
        }
        let mut updated = installed;
        updated.enabled = true;
        state.library.upsert_mod(updated);
        clear_pending(state, &disk.file_name);
        return Ok(ToggleResult {
            outcome: "done".to_string(),
            enabled: true,
            message: Some("Включён.".to_string()),
        });
    }

    let db = GameModDb::new(&mods_dir);
    let result = db.set_active(&[(&disk.file_name, enabled)], process::is_running);
    match result.outcome {
        SetActiveOutcome::Done | SetActiveOutcome::Unchanged => {
            let mut updated = installed;
            updated.enabled = enabled;
            state.library.upsert_mod(updated);
            clear_pending(state, &disk.file_name);
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
            clear_pending(state, &disk.file_name);
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
            clear_pending(state, &disk.file_name);
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

#[tauri::command]
pub async fn mods_toggle(
    state: tauri::State<'_, Arc<AppState>>,
    ids: Vec<String>,
    enabled: bool,
) -> Result<BulkToggleResult, AppError> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || toggle_many(&state, &ids, enabled))
        .await
        .map_err(|error| AppError::new("task", error.to_string()))?
}

/// Массовое включение/выключение: один скан, один `db.json` и одна запись библиотеки.
pub(crate) fn toggle_many(
    state: &AppState,
    ids: &[String],
    enabled: bool,
) -> Result<BulkToggleResult, AppError> {
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
    let mut changed = Vec::new();
    let mut queued = Vec::new();
    let mut failed = Vec::new();
    if process::is_running() {
        for id in ids {
            if let Some(installed) = state.library.get_mod(id) {
                queue_toggle(state, &installed.file_name, enabled);
                queued.push(id.clone());
            } else {
                failed.push(id.clone());
            }
        }
        return Ok(BulkToggleResult {
            changed,
            queued,
            failed,
        });
    }

    let layout = ModLayout::in_app_folder(&mods_dir, state.paths.root());
    let disks = scan::scan_mods(&layout).mods;
    let by_file: HashMap<String, DiskMod> = disks
        .into_iter()
        .map(|disk| (disk.file_name.to_lowercase(), disk))
        .collect();
    let keep = settings.keep_backups.max(1) as usize;
    let installer = ModInstaller::new(layout, system_clock(), Arc::new(move || keep));
    let _batch = state.library.batch();

    let mut planned = Vec::new();
    let mut moved_enabled = Vec::new();
    for id in ids {
        let Some(installed) = state.library.get_mod(id) else {
            failed.push(id.clone());
            continue;
        };
        let Some(disk) = by_file.get(&installed.file_name.to_lowercase()).cloned() else {
            failed.push(id.clone());
            continue;
        };
        if enabled && !disk.enabled && disk.sub_folder.is_none() {
            let moved = installer.enable(&disk.file_name);
            if !moved.success {
                failed.push(id.clone());
                continue;
            }
            moved_enabled.push(disk.file_name.clone());
        }
        planned.push((id.clone(), installed, disk));
    }
    if planned.is_empty() {
        return Ok(BulkToggleResult {
            changed,
            queued,
            failed,
        });
    }

    let changes: Vec<(&str, bool)> = planned
        .iter()
        .map(|(_, _, disk)| (disk.file_name.as_str(), enabled))
        .collect();
    let db = GameModDb::new(&mods_dir);
    let outcome = db.set_active(&changes, process::is_running);
    let not_in_db: HashSet<String> = outcome
        .not_in_db
        .iter()
        .map(|file| file.to_lowercase())
        .collect();
    let db_missing = matches!(
        outcome.outcome,
        SetActiveOutcome::NotInDb | SetActiveOutcome::NoDb
    );

    match outcome.outcome {
        SetActiveOutcome::GameRunning => {
            for (id, installed, _) in planned {
                queue_toggle(state, &installed.file_name, enabled);
                queued.push(id);
            }
        }
        SetActiveOutcome::Done
        | SetActiveOutcome::Unchanged
        | SetActiveOutcome::NotInDb
        | SetActiveOutcome::NoDb => {
            let mut changed_files = Vec::new();
            for (id, installed, disk) in planned {
                let file_missing_in_db =
                    db_missing || not_in_db.contains(&disk.file_name.to_lowercase());
                if !enabled && file_missing_in_db && disk.enabled {
                    if disk.sub_folder.is_some() {
                        failed.push(id);
                        continue;
                    }
                    let moved = installer.disable(&disk.file_name);
                    if !moved.success {
                        failed.push(id);
                        continue;
                    }
                }
                let mut updated = installed;
                updated.enabled = enabled;
                state.library.upsert_mod(updated);
                changed_files.push(disk.file_name);
                changed.push(id);
            }
            clear_pending_many(state, &changed_files);
        }
        _ => {
            // db.json не удалось изменить: включённые файлы вернуть обратно.
            for file in &moved_enabled {
                let _ = installer.disable(file);
            }
            failed.extend(planned.into_iter().map(|(id, _, _)| id));
        }
    }
    Ok(BulkToggleResult {
        changed,
        queued,
        failed,
    })
}

fn clear_pending(state: &AppState, file_name: &str) {
    let mut pending = state.pending.lock().unwrap();
    let before = pending.len();
    pending.retain(|key, _| !key.eq_ignore_ascii_case(file_name));
    let changed = pending.len() != before;
    drop(pending);
    if changed {
        let _ = state.save_pending();
    }
}

fn clear_pending_many(state: &AppState, file_names: &[String]) {
    if file_names.is_empty() {
        return;
    }
    let mut pending = state.pending.lock().unwrap();
    let before = pending.len();
    pending.retain(|key, _| !file_names.iter().any(|file| key.eq_ignore_ascii_case(file)));
    let changed = pending.len() != before;
    drop(pending);
    if changed {
        let _ = state.save_pending();
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

fn try_live_toggle(
    state: &AppState,
    mods_dir: &Path,
    installed: &bmm_domain::library::InstalledMod,
    disk: &DiskMod,
    enabled: bool,
) -> Option<ToggleResult> {
    let bridge = bmm_game::bridge::GameBridge::with_system_clock(mods_dir);
    if !bridge.is_alive() {
        return None;
    }
    let id = bridge.send(&[(disk.file_name.as_str(), enabled)])?;
    let key = bmm_game::db::key_of(&disk.file_name);
    // Активация мода на сотни мегабайт занимает секунды; два секунды — мало.
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(answer) = bridge.read_answer()
            && answer.ack_id.as_deref() == Some(id.as_str())
        {
            return match answer.result(&key) {
                Some("ok") => {
                    let mut updated = installed.clone();
                    updated.enabled = enabled;
                    state.library.upsert_mod(updated);
                    clear_pending(state, &disk.file_name);
                    Some(ToggleResult {
                        outcome: "done".to_string(),
                        enabled,
                        message: Some(if enabled {
                            "Включён.".to_string()
                        } else {
                            "Выключен.".to_string()
                        }),
                    })
                }
                _ => None,
            };
        }
        if Instant::now() >= deadline {
            // Мост жив или занят этой командой — изменение уже у игры, не врём
            // «применится, когда закроете игру».
            if bridge.is_busy_for(&id) || bridge.is_alive() {
                return Some(ToggleResult {
                    outcome: "queued".to_string(),
                    enabled,
                    message: Some(
                        "Игра занята: изменение отправлено и применится, когда она закончит."
                            .to_string(),
                    ),
                });
            }
            return None;
        }
        std::thread::sleep(Duration::from_millis(50));
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
    // Как на карточке: название из архива, если оно уже прочитано.
    let archive_title = target
        .as_ref()
        .and_then(|target| state.previews.title(&target.key))
        .flatten();
    let name = ModTitles::choose(
        repo.and_then(|repo| repo.title.as_deref()),
        archive_title.as_deref(),
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
        checking_declined: state.library.is_checking_declined(&installed.id),
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
    let pick = match preview::pick(&target.path) {
        Ok(pick) => pick,
        Err(_) => return None,
    };
    let Some(pick) = pick else {
        let _ = state.previews.put_no_image(&target.key, &target.file_name);
        return None;
    };
    let bytes = preview::extract(&target.path, &pick.raw_name).ok()?;
    let Some((_, extension)) = state
        .previews
        .put_image(&target.key, &pick.extension, &bytes)
        .ok()?
    else {
        let _ = state.previews.put_no_image(&target.key, &target.file_name);
        return None;
    };
    Some(preview_url(&target.key, &extension))
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

/// «Включено 3 333 файла. Игра может грузиться дольше»: разряды и падеж.
/// Считаются файлы: включённые копии одного мода тоже замедляют загрузку.
fn many_enabled_text(count: usize) -> String {
    format!(
        "Включено {} {}. Игра может грузиться дольше",
        group_digits(count),
        plural(count, "файл", "файла", "файлов")
    )
}

fn group_digits(value: usize) -> String {
    let text = value.to_string();
    let mut result = String::with_capacity(text.len() + text.len() / 3);
    for (index, character) in text.chars().enumerate() {
        if index > 0 && (text.len() - index).is_multiple_of(3) {
            result.push(' ');
        }
        result.push(character);
    }
    result
}

fn plural<'a>(count: usize, one: &'a str, few: &'a str, many: &'a str) -> &'a str {
    let tens = count % 100;
    let units = count % 10;
    if (11..=14).contains(&tens) {
        many
    } else if units == 1 {
        one
    } else if (2..=4).contains(&units) {
        few
    } else {
        many
    }
}

/// Предупреждения о файле библиотеки. Несохранённая библиотека — молча терять
/// подписки и проверки нельзя: пишем об этом на главной.
fn library_warnings(loaded: &[String], save_error: Option<String>) -> Vec<String> {
    let mut all = loaded.to_vec();
    if let Some(error) = save_error {
        all.push(format!("Не получается сохранить данные программы: {error}"));
    }
    all
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
        many_enabled: None,
        deleted: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

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
            library: Arc::new(JsonLibraryStore::new(
                temp.path().join("data/library.json"),
                system_clock(),
            )),
            previews: PreviewCache::new(temp.path().join("data/previews")),
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
        crate::test_support::test_state(temp, mods)
    }

    #[test]
    fn repository_page_and_version_come_from_the_archive_before_the_game_runs() {
        // dummy.zip из скачанного набора: в имени нет версии, игру ещё не запускали
        // (db.json без modData), но в архиве лежит mod_info от репозитория BeamNG.
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        write_zip(
            &mods.join("dummy.zip"),
            &[
                ("vehicles/dummy/a.jbeam", b"x"),
                (
                    "mod_info/M9EL2DOCR/info.json",
                    br#"{"tagid":"M9EL2DOCR","title":"Crash Test Dummy","version_string":"1.1","resource_id":906,"username":"B25Mitch"}"#,
                ),
            ],
        );
        let state = test_state(&temp, &mods);
        let first = load_snapshot(&state).unwrap();
        let id = first.mods[0].id.clone();
        assert_eq!(state.library.get_mod(&id).unwrap().version, None);

        fill_titles(&state);
        load_snapshot(&state).unwrap();
        let installed = state.library.get_mod(&id).unwrap();
        assert_eq!(installed.version.as_deref(), Some("1.1"));
        assert_eq!(installed.name, "Crash Test Dummy");
        let binding = state
            .library
            .bindings()
            .into_iter()
            .find(|binding| binding.mod_id == id)
            .expect("страница в репозитории");
        assert_eq!(binding.source, "beamng");
        assert_eq!(binding.url.as_str(), "https://beamng.com/resources/906/");
        assert!(binding.confirmed);
    }

    #[test]
    fn cache_forgets_removed_mods_but_not_before_the_list_is_built() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        write_zip(&mods.join("keep.zip"), &[("vehicles/keep/a.jbeam", b"x")]);
        write_zip(&mods.join("gone.zip"), &[("vehicles/gone/a.jbeam", b"x")]);
        let state = test_state(&temp, &mods);

        // До первого показа списка чистить нечего — и стирать всё нельзя.
        state
            .previews
            .put_titles(&[("orphan".to_string(), Some("Old".to_string()))])
            .unwrap();
        prune_previews(&state);
        assert!(state.previews.titles().contains_key("orphan"));

        load_snapshot(&state).unwrap();
        fill_titles(&state);
        assert_eq!(state.previews.contents().len(), 2);
        std::fs::remove_file(mods.join("gone.zip")).unwrap();
        load_snapshot(&state).unwrap();
        prune_previews(&state);
        let contents = state.previews.contents();
        assert_eq!(contents.len(), 1, "остался только живой мод");
        assert!(!state.previews.titles().contains_key("orphan"));
    }

    #[test]
    fn copies_are_grouped_after_contents_are_read() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let weather: &[(&str, &[u8])] = &[
            ("lua/ge/extensions/weather_fx.lua", b"print(1)"),
            ("scripts/weather_fx/modScript.lua", b"x"),
        ];
        write_zip(&mods.join("weather_fx.zip"), weather);
        write_zip(&mods.join("weather_fx (1).zip"), weather);
        write_zip(&mods.join("rs3.zip"), &[("vehicles/rs3y8/a.jbeam", b"x")]);
        let state = test_state(&temp, &mods);

        // Первый показ: состав архивов ещё не прочитан — пометок нет, список не ждёт.
        let first = load_snapshot(&state).unwrap();
        assert!(first.mods.iter().all(|card| card.duplicate_of.is_none()));

        fill_titles(&state);
        let snapshot = load_snapshot(&state).unwrap();
        let weather: Vec<&ModCardDto> = snapshot
            .mods
            .iter()
            .filter(|card| card.file_name.starts_with("weather_fx"))
            .collect();
        let main = weather
            .iter()
            .find(|card| card.duplicate_of.is_none())
            .unwrap();
        let copy = weather
            .iter()
            .find(|card| card.duplicate_of.is_some())
            .unwrap();
        let duplicate = copy.duplicate_of.as_ref().unwrap();
        assert_eq!(duplicate.id, main.id);
        assert!(duplicate.exact);
        assert_eq!(main.group, vec![copy.id.clone()]);
        assert!(main.group_conflict, "включены обе — игра загрузит обе");
        assert_eq!(main.role.as_deref(), Some("addon"));
        // Счётчики фильтров — по карточкам, а не по файлам: копия скрыта за основной.
        assert_eq!(snapshot.counts.all, 2);
        assert_eq!(snapshot.counts.enabled, 2);
        let rs3 = snapshot
            .mods
            .iter()
            .find(|card| card.file_name == "rs3.zip")
            .unwrap();
        assert!(rs3.duplicate_of.is_none() && rs3.group.is_empty());
        assert_eq!(rs3.role.as_deref(), Some("vehicle"));

        // Выключенная копия остаётся в группе, но игре уже не мешает.
        toggle_mod(&state, &copy.id, false).unwrap();
        let after = load_snapshot(&state).unwrap();
        let main_after = after.mods.iter().find(|card| card.id == main.id).unwrap();
        assert_eq!(main_after.group.len(), 1);
        assert!(!main_after.group_conflict);
    }

    #[test]
    fn mod_info_cache_keeps_only_needed_fields() {
        let full = serde_json::json!({
            "id": "abc",
            "data": {
                "tag_line": "Hint",
                "title": "Title",
                "name": "Name",
                "version": "1.0",
                "version_string": "1.1",
                "resource_id": 42,
                "username": "user",
                "author": "author",
                "last_update": "2026-10-20T10:00:00Z",
                "description": "x".repeat(10_000),
                "screenshots": [1, 2, 3]
            }
        });
        let compact = compact_mod_info(&full);
        assert_eq!(compact["id"], "abc");
        assert_eq!(mod_info_hint(&compact), Some("Hint"));
        assert!(
            compact["data"].get("description").is_none(),
            "длинное описание в кеше не нужно"
        );
        let repo = bmm_game::db::RepoInfo::from_mod_data("abc", &compact["data"]);
        assert_eq!(repo.version.as_deref(), Some("1.1"));
        assert_eq!(repo.title.as_deref(), Some("Title"));
        assert_eq!(repo.author.as_deref(), Some("user"));
        assert_eq!(repo.resource_id, Some(42));
        assert!(repo.page_url().is_some());

        assert!(compact_mod_info(&serde_json::Value::Null).is_null());
        assert!(compact_mod_info(&serde_json::json!({"data": {}})).is_null());
    }

    #[test]
    fn roles_use_gameplay_and_mod_info_hint() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        write_zip(
            &mods.join("career.zip"),
            &[
                ("gameplay/overhaul/career.lua", b"x"),
                ("vehicles/testcar/a.jbeam", b"x"),
                ("levels/testlevel/a.json", b"x"),
            ],
        );
        write_zip(
            &mods.join("dummy.zip"),
            &[
                ("vehicles/dummy/dummy.jbeam", b"x"),
                (
                    "mod_info/ABCD/info.json",
                    br#"{"tag_line":"75KG Ragdoll Humanoid","title":"Crash Test Dummy"}"#,
                ),
            ],
        );
        write_zip(
            &mods.join("testcar.zip"),
            &[("vehicles/testcar/a.jbeam", b"x")],
        );
        write_zip(
            &mods.join("testmap.zip"),
            &[("levels/testlevel/a.json", b"x")],
        );
        let state = test_state(&temp, &mods);

        let first = load_snapshot(&state).unwrap();
        assert!(first.mods.iter().all(|card| card.role.is_none()));
        fill_titles(&state);
        let snapshot = load_snapshot(&state).unwrap();
        let role = |file: &str| {
            snapshot
                .mods
                .iter()
                .find(|card| card.file_name == file)
                .and_then(|card| card.role.as_deref())
        };
        assert_eq!(role("career.zip"), Some("addon"), "gameplay/ — не машина");
        assert_eq!(role("dummy.zip"), Some("addon"), "humanoid — не машина");
        assert_eq!(role("testcar.zip"), Some("vehicle"));
        assert_eq!(role("testmap.zip"), Some("map"));
    }

    #[test]
    fn many_enabled_text_has_digits_and_right_case() {
        assert_eq!(
            many_enabled_text(39),
            "Включено 39 файлов. Игра может грузиться дольше"
        );
        assert!(many_enabled_text(21).starts_with("Включено 21 файл."));
        assert!(many_enabled_text(3_333).starts_with("Включено 3 333 файла."));
        assert!(many_enabled_text(111).starts_with("Включено 111 файлов."));
        assert!(many_enabled_text(20_000).starts_with("Включено 20 000 файлов."));
        assert_eq!(group_digits(999), "999");
        assert_eq!(group_digits(1_000), "1 000");
    }

    #[test]
    fn unsaved_library_is_reported() {
        assert!(library_warnings(&[], None).is_empty());
        let warnings = library_warnings(&["старое".to_string()], Some("диск полон".to_string()));
        assert_eq!(warnings.len(), 2);
        assert!(warnings[1].contains("Не получается сохранить"));
        assert!(warnings[1].contains("диск полон"));
    }

    #[test]
    #[ignore = "замер: cargo test --release -- --ignored five_hundred"]
    fn five_hundred_mods_load_quickly() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        for index in 0..500 {
            let jbeam = format!("vehicles/car{index}/car.jbeam");
            let info = format!("vehicles/car{index}/info.json");
            let body = format!("{{\"Name\":\"Car {index}\"}}");
            write_zip(
                &mods.join(format!("car_{index}_v1.{}.zip", index % 7)),
                &[(jbeam.as_str(), b"{}"), (info.as_str(), body.as_bytes())],
            );
        }
        let state = test_state(&temp, &mods);
        let started = std::time::Instant::now();
        let first = load_snapshot(&state).unwrap();
        let cold = started.elapsed();
        let started = std::time::Instant::now();
        fill_titles(&state);
        let fill = started.elapsed();
        let started = std::time::Instant::now();
        let again = load_snapshot(&state).unwrap();
        let warm = started.elapsed();
        eprintln!("cold {cold:?} fill {fill:?} warm {warm:?}");
        assert_eq!(first.mods.len(), 500);
        assert_eq!(again.mods.len(), 500);
        assert!(again.mods.iter().all(|card| card.duplicate_of.is_none()));
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

    #[cfg(windows)]
    #[test]
    fn enable_rolls_back_when_db_cannot_be_updated() {
        use std::os::windows::fs::OpenOptionsExt;

        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(
            mods.join("db.json"),
            r#"{"header":{"version":1.1},"mods":{}}"#,
        )
        .unwrap();
        let state = test_state(&temp, &mods);
        let layout = ModLayout::in_app_folder(&mods, state.paths.root());
        std::fs::create_dir_all(&layout.disabled_dir).unwrap();
        std::fs::write(layout.disabled_dir.join("mod.zip"), b"zip").unwrap();

        let snapshot = load_snapshot(&state).unwrap();
        let id = snapshot
            .mods
            .iter()
            .find(|card| card.file_name == "mod.zip")
            .unwrap()
            .id
            .clone();
        assert!(!state.library.get_mod(&id).unwrap().enabled);

        let _lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(mods.join("db.json"))
            .unwrap();
        let result = toggle_mod(&state, &id, true);
        assert!(result.is_err(), "{result:?}");
        assert!(layout.disabled_dir.join("mod.zip").is_file());
        assert!(!mods.join("mod.zip").exists());
        assert!(!state.library.get_mod(&id).unwrap().enabled);
    }

    #[test]
    fn toggle_many_moves_all_with_one_result() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("a.zip"), b"zip").unwrap();
        std::fs::write(mods.join("b.zip"), b"zip").unwrap();
        let state = test_state(&temp, &mods);
        let layout = ModLayout::in_app_folder(&mods, state.paths.root());
        let snapshot = load_snapshot(&state).unwrap();
        let ids: Vec<String> = snapshot.mods.iter().map(|item| item.id.clone()).collect();
        assert_eq!(ids.len(), 2);

        let off = toggle_many(&state, &ids, false).unwrap();
        assert_eq!(off.changed.len(), 2);
        assert!(off.failed.is_empty());
        assert!(layout.disabled_dir.join("a.zip").is_file());
        assert!(layout.disabled_dir.join("b.zip").is_file());
        assert!(!mods.join("a.zip").exists());
        assert!(!mods.join("b.zip").exists());
        for id in &ids {
            assert!(!state.library.get_mod(id).unwrap().enabled);
        }

        let on = toggle_many(&state, &ids, true).unwrap();
        assert_eq!(on.changed.len(), 2);
        assert!(mods.join("a.zip").is_file());
        assert!(mods.join("b.zip").is_file());
        for id in &ids {
            assert!(state.library.get_mod(id).unwrap().enabled);
        }
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

    #[test]
    fn apply_pending_without_db_moves_file_and_clears_queue() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("fresh.zip"), b"zip").unwrap();
        let state = test_state(&temp, &mods);
        let layout = ModLayout::in_app_folder(&mods, state.paths.root());
        state
            .pending
            .lock()
            .unwrap()
            .insert("fresh.zip".to_string(), false);

        apply_pending(&state, &GameModDb::new(&mods), &layout);

        assert!(layout.disabled_dir.join("fresh.zip").is_file());
        assert!(!mods.join("fresh.zip").exists());
        assert!(state.pending.lock().unwrap().is_empty());
    }

    #[test]
    fn declined_repo_mod_keeps_user_title_and_version() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("mudstone_valley.zip"), b"zip").unwrap();
        std::fs::write(
            mods.join("db.json"),
            include_str!("../../../fixtures/game/db-5mods-0.39.4.json"),
        )
        .unwrap();
        let state = test_state(&temp, &mods);
        let snapshot = load_snapshot(&state).unwrap();
        let id = snapshot
            .mods
            .iter()
            .find(|item| item.file_name == "mudstone_valley.zip")
            .unwrap()
            .id
            .clone();

        let synced = state.library.get_mod(&id).unwrap();
        assert_eq!(synced.name, "Mudstone Valley Ultra 4 & Off Road Racing");
        assert_eq!(synced.version.as_deref(), Some(".901"));
        assert_eq!(
            synced.installed_at_utc.timestamp(),
            1_792_060_320,
            "дата установки берётся из db.json"
        );
        assert!(
            state
                .library
                .bindings()
                .iter()
                .any(|binding| binding.source == "beamng")
        );

        let mut custom = synced;
        custom.name = "Моя трасса".to_string();
        custom.version = None;
        state.library.upsert_mod(custom);
        state.library.set_checking_declined(&id, true);

        apply_repo_info(&state, &GameModDb::new(&mods).read(), &HashMap::new());

        let after = state.library.get_mod(&id).unwrap();
        assert_eq!(after.name, "Моя трасса");
        assert_eq!(after.version, None);
    }

    #[test]
    fn live_bridge_toggle_updates_library() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("mod.zip"), b"zip").unwrap();
        let state = test_state(&temp, &mods);
        let snapshot = load_snapshot(&state).unwrap();
        let installed = state.library.get_mod(&snapshot.mods[0].id).unwrap();
        let disk = scan::scan_mods(&ModLayout::in_app_folder(&mods, state.paths.root()))
            .mods
            .into_iter()
            .find(|item| item.file_name == "mod.zip")
            .unwrap();

        let bridge = bmm_game::bridge::GameBridge::with_system_clock(&mods);
        std::fs::create_dir_all(bridge.state_path().parent().unwrap()).unwrap();
        std::fs::write(
            bridge.state_path(),
            format!(r#"{{"alive":{}}}"#, chrono::Utc::now().timestamp()),
        )
        .unwrap();

        let mods_for_game = mods.clone();
        let game = std::thread::spawn(move || {
            let bridge = bmm_game::bridge::GameBridge::with_system_clock(&mods_for_game);
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Ok(text) = std::fs::read_to_string(bridge.command_path())
                    && let Ok(command) = serde_json::from_str::<serde_json::Value>(&text)
                    && let Some(id) = command.get("id").and_then(|value| value.as_str())
                {
                    let answer = serde_json::json!({
                        "alive": chrono::Utc::now().timestamp(),
                        "ackId": id,
                        "results": {"mod": "ok"},
                    });
                    std::fs::write(bridge.state_path(), answer.to_string()).unwrap();
                    return;
                }
                if Instant::now() >= deadline {
                    return;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });

        let result =
            try_live_toggle(&state, &mods, &installed, &disk, false).expect("мост должен ответить");
        game.join().unwrap();
        assert_eq!(result.outcome, "done");
        assert!(!result.enabled);
        assert!(!state.library.get_mod(&installed.id).unwrap().enabled);
    }

    #[test]
    fn preview_error_is_not_cached_as_no_image() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("bad.zip"), b"not a zip").unwrap();
        let state = test_state(&temp, &mods);
        let snapshot = load_snapshot(&state).unwrap();
        let id = snapshot.mods[0].id.clone();
        let key = state
            .previews_by_id
            .lock()
            .unwrap()
            .get(&id)
            .unwrap()
            .key
            .clone();

        assert!(ensure_preview(&state, &id).is_none());
        assert!(
            state.previews.get(&key).is_none(),
            "ошибка чтения не должна навсегда запрещать превью"
        );

        let mut png = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
        png.extend_from_slice(&13u32.to_be_bytes());
        png.extend_from_slice(b"IHDR");
        png.extend_from_slice(&100u32.to_be_bytes());
        png.extend_from_slice(&100u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0]);
        png.extend_from_slice(&[0, 0, 0, 0]);
        let file = std::fs::File::create(mods.join("bad.zip")).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file(
            "mod_info/car/icon.png",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&png).unwrap();
        zip.finish().unwrap();

        let snapshot = load_snapshot(&state).unwrap();
        let id = snapshot.mods[0].id.clone();
        assert!(
            ensure_preview(&state, &id).is_some(),
            "после исправления архива превью должно появиться"
        );
    }

    #[test]
    fn disabled_by_game_mod_keeps_its_archive_in_mods() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let layout = ModLayout::in_app_folder(&mods, temp.path().join("data"));
        std::fs::create_dir_all(&layout.disabled_dir).unwrap();
        std::fs::write(mods.join("Car.zip"), b"x").unwrap();
        let disk = DiskMod {
            file_name: "Car.zip".to_string(),
            enabled: false,
            size_bytes: 1,
            sub_folder: None,
            modified_utc: None,
        };
        assert_eq!(path_of(&layout, &disk), mods.join("Car.zip"));

        std::fs::write(layout.disabled_dir.join("Car.zip"), b"x").unwrap();
        assert_eq!(path_of(&layout, &disk), layout.disabled_dir.join("Car.zip"));
    }

    fn write_zip(path: &std::path::Path, entries: &[(&str, &[u8])]) {
        let file = std::fs::File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        for (name, bytes) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn garage_and_rust_agree_on_values() {
        use crate::test_support::ts_union;
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        write_zip(
            &mods.join("car.zip"),
            &[
                ("vehicles/car/car.jbeam", b"{}"),
                ("lua/ge/extensions/car.lua", b"--"),
            ],
        );
        std::fs::write(
            mods.join("db.json"),
            r#"{"header":{"version":1.1},"mods":{"car":{"filename":"car.zip","active":true,"modType":"vehicle"}}}"#,
        )
        .unwrap();
        let state = test_state(&temp, &mods);

        let snapshot = serde_json::to_value(load_snapshot(&state).unwrap()).unwrap();
        let id = snapshot["mods"][0]["id"].as_str().unwrap().to_string();
        toggle_mod(&state, &id, false).unwrap();
        details(&state, &id).unwrap();

        // Строки-значения, которые может прислать Rust, окно знает.
        let kinds = ts_union("unions.ts", "ModKind");
        for mod_type in [
            "vehicle", "terrain", "level", "sound", "scenario", "app", "script", "?",
        ] {
            assert!(
                kinds.contains(kind_of(mod_type)),
                "окно не знает тип {mod_type}"
            );
        }
        let pending = ts_union("unions.ts", "PendingState");
        assert!(pending.contains("none") && pending.contains("queued"));
        let roles = ts_union("unions.ts", "ModRole");
        for role in [
            bmm_packs::duplicates::Role::Vehicle,
            bmm_packs::duplicates::Role::Map,
            bmm_packs::duplicates::Role::Addon,
        ] {
            assert!(roles.contains(role.code()));
        }
        let problems = ts_union("unions.ts", "PackagingProblem");
        for problem in [
            bmm_packs::incoming::PackagingProblem::Wrapped,
            bmm_packs::incoming::PackagingProblem::Loose,
            bmm_packs::incoming::PackagingProblem::NotMod,
        ] {
            assert!(problems.contains(problem.code()));
        }
    }

    #[test]
    fn fix_packaging_repairs_archive_and_keeps_known_version() {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        write_zip(
            &mods.join("acadia.zip"),
            &[
                ("Acadia_16/acadia/acadia.jbeam", b"{}"),
                ("Acadia_16/acadia/info.json", b"{\"Name\":\"Acadia\"}"),
            ],
        );
        let state = test_state(&temp, &mods);
        let first = load_snapshot(&state).unwrap();
        let id = first.mods[0].id.clone();
        // Версию программа узнала раньше (например, с сайта после обновления).
        let mut known = state.library.get_mod(&id).unwrap();
        known.version = Some("1.2".to_string());
        state.library.upsert_mod(known);
        fill_titles(&state);
        assert_eq!(
            load_snapshot(&state).unwrap().mods[0].problem.as_deref(),
            Some("loose")
        );

        fix_packaging(&state, &id).unwrap();

        let after = load_snapshot(&state).unwrap();
        fill_titles(&state);
        let after = if after.mods[0].problem.is_some() {
            load_snapshot(&state).unwrap()
        } else {
            after
        };
        assert_eq!(after.mods.len(), 1);
        assert_eq!(after.mods[0].problem, None);
        assert_eq!(
            after.mods[0].version.as_deref(),
            Some("1.2"),
            "версия не потерялась"
        );
        let index = bmm_packs::zip_index::read_index(&mods.join("acadia.zip")).unwrap();
        assert!(
            index
                .entries
                .iter()
                .all(|entry| entry.name.starts_with("vehicles/acadia/")),
            "{:?}",
            index
                .entries
                .iter()
                .map(|entry| &entry.name)
                .collect::<Vec<_>>()
        );
    }
}

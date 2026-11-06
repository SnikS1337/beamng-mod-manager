use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use bmm_domain::library::LibraryStore;
use bmm_packs::fingerprint::{self, Fingerprint};
use bmm_packs::incoming::{
    self, IncomingAnalysis, IncomingItem, IncomingKind, IncomingOptions, InstalledArchive,
};
use bmm_packs::scan;
use bmm_packs::zip_index;

use crate::commands::versions::installer_for;
use crate::error::AppError;
use crate::state::{AppState, CachedContent};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IncomingFindingDto {
    #[ts(type = "FindingSeverity")]
    pub severity: String,
    pub code: String,
    pub message: String,
    pub entry: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IncomingItemDto {
    pub zip_path: String,
    pub file_name: String,
    pub size: u64,
    pub title: String,
    #[ts(type = "IncomingKind")]
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

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct SimilarModDto {
    pub file_name: String,
    pub title: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IncomingArchiveDto {
    pub source: String,
    pub problem: Option<String>,
    pub work_dir: String,
    pub items: Vec<IncomingItemDto>,
}

#[derive(Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IncomingInstallRequest {
    pub zip_path: String,
    pub file_name: String,
    pub replaces: Option<String>,
    pub allow_warnings: bool,
    /// Версия со страницы, с которой человек поставил обновление. В имени файла и в
    /// самом архиве её может не быть (или там версия игры, «v.0.20.2»): без неё мод
    /// после обновления так и висел в «Обновлениях».
    #[serde(default)]
    #[ts(optional)]
    pub version: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct IncomingInstallResult {
    pub success: bool,
    pub message: Option<String>,
    /// Установлено, но запись библиотеки обновилась не полностью.
    pub warning: Option<String>,
    pub file_name: String,
    pub replaces: Option<String>,
    pub backup_id: Option<String>,
    pub was_disabled: bool,
    pub same_name: bool,
}

#[derive(Debug, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "IncomingTicket")]
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
    // На месте поставленного архива уже другой файл (подменили, скачали заново под
    // тем же именем) — его не трогаем.
    if !may_remove_source(&path) {
        return Ok(false);
    }
    forget_checked(&path);
    match fs::remove_file(&path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(AppError::new("io", error.to_string())),
    }
}

fn may_remove_source(path: &Path) -> bool {
    let known = checked().get(path).cloned();
    fingerprint::verdict(known.as_ref(), path) == fingerprint::Verdict::Same
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
        .filter(|folder| crate::commands::settings::acceptable_folder(folder))
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
    let installer = installer_for(state)?;
    let installed = installed_archives(state, &installer);
    let root = incoming_root();
    let _ = fs::create_dir_all(&root);
    cleanup_incoming(&root);
    let cancel = AtomicBool::new(false);
    let mut result = Vec::new();
    for raw in paths {
        let source = PathBuf::from(raw);
        let work = root.join(format!("{}-{}", std::process::id(), unique_suffix()));
        // Отпечаток до и после разбора: менялся файл посередине — проверка не о нём.
        let before = Fingerprint::of(&source).ok();
        let mut analysis = incoming::analyze(
            &source,
            &installed,
            &work,
            IncomingOptions::default(),
            &cancel,
        );
        let after = Fingerprint::of(&source).ok();
        match checked_print(before, after) {
            Ok(print) => remember_checked(&source, print),
            Err(problem) => {
                forget_checked(&source);
                if analysis.problem.is_none() {
                    analysis.problem = Some(problem.to_string());
                }
            }
        }
        result.push(to_dto(analysis));
    }
    Ok(result)
}

/// Архивы, проверенные в этом запуске: путь → отпечаток на момент проверки. Ставить
/// и удалять после установки можно только тот же самый файл.
static CHECKED: std::sync::LazyLock<std::sync::Mutex<HashMap<PathBuf, Fingerprint>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));
/// Столько проверенных архивов помнить. Больше за один запуск не бывает.
const CHECKED_KEEP: usize = 512;

fn checked() -> std::sync::MutexGuard<'static, HashMap<PathBuf, Fingerprint>> {
    CHECKED.lock().unwrap_or_else(|error| error.into_inner())
}

fn remember_checked(path: &Path, print: Fingerprint) {
    let mut known = checked();
    if known.len() >= CHECKED_KEEP && !known.contains_key(path) {
        known.clear();
    }
    known.insert(path.to_path_buf(), print);
}

fn forget_checked(path: &Path) {
    checked().remove(path);
}

/// Что запомнить после разбора: отпечаток, если файл не менялся, пока его читали.
fn checked_print(
    before: Option<Fingerprint>,
    after: Option<Fingerprint>,
) -> Result<Fingerprint, &'static str> {
    match (before, after) {
        (Some(before), Some(after)) if before == after => Ok(before),
        (Some(_), Some(_)) => Err(
            "Архив менялся, пока его проверяли, — например, ещё докачивался. Откройте его ещё раз.",
        ),
        // Файла нет или он не читается — разбор и так скажет, что не так.
        _ => Err("Файл не найден."),
    }
}

/// Можно ли ставить этот файл. Наши копии во временной папке (перепакованные,
/// вложенные архивы) человек не трогает — их не сверяем.
fn install_verdict(source: &Path, work_root: &Path) -> fingerprint::Verdict {
    if source.starts_with(work_root) {
        return fingerprint::Verdict::Same;
    }
    let known = checked().get(source).cloned();
    fingerprint::verdict(known.as_ref(), source)
}

fn installed_archives(
    state: &AppState,
    installer: &bmm_packs::install::ModInstaller,
) -> Vec<InstalledArchive> {
    let scan = scan::scan_mods(installer.layout());
    let titles = state.previews.titles_shared();
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
    let installer = installer_for(state)?;
    let keep = state.settings.load().settings.keep_backups.max(1) as usize;
    let mut results = Vec::new();
    let work_root = incoming_root();
    for request in requests {
        let source = PathBuf::from(&request.zip_path);
        // Между проверкой и установкой архив могли удалить, переименовать или подменить:
        // ставим только тот файл, который проверили.
        if let Some(message) = install_verdict(&source, &work_root).message() {
            results.push(IncomingInstallResult {
                success: false,
                message: Some(message.to_string()),
                warning: None,
                file_name: request.file_name.clone(),
                replaces: request.replaces.clone(),
                backup_id: None,
                was_disabled: false,
                same_name: false,
            });
            continue;
        }
        let options = bmm_packs::install::InstallOptions {
            allow_warnings: request.allow_warnings,
            ..bmm_packs::install::InstallOptions::default()
        };
        if let Some(replaced) = &request.replaces {
            if replaced.is_empty()
                || replaced.contains(['/', '\\'])
                || replaced == "."
                || replaced == ".."
            {
                results.push(IncomingInstallResult {
                    success: false,
                    message: Some("Недопустимое имя установленного файла.".to_string()),
                    warning: None,
                    file_name: request.file_name.clone(),
                    replaces: Some(replaced.clone()),
                    backup_id: None,
                    was_disabled: false,
                    same_name: false,
                });
                continue;
            }
            // Мод из подпапки игры (репозиторий) не заменяем: игра держит его сама,
            // и рядом появилась бы вторая копия.
            let in_mods = installer.layout().mods_dir.join(replaced).is_file();
            let in_disabled = installer.layout().disabled_dir.join(replaced).is_file();
            if !in_mods && !in_disabled {
                results.push(IncomingInstallResult {
                    success: false,
                    message: Some(
                        "Установленная версия лежит в подпапке игры — обновите её там.".to_string(),
                    ),
                    warning: None,
                    file_name: request.file_name.clone(),
                    replaces: Some(replaced.clone()),
                    backup_id: None,
                    was_disabled: false,
                    same_name: false,
                });
                continue;
            }
            let update = installer.update_from_path(replaced, &source, &request.file_name, options);
            let mut warning = None;
            if update.install.success
                && !replaced.eq_ignore_ascii_case(&request.file_name)
                && let Some(path) = update.install.path.as_ref()
            {
                let size = fs::metadata(path)
                    .ok()
                    .and_then(|meta| i64::try_from(meta.len()).ok())
                    .unwrap_or(0);
                if !bmm_domain::library::rename_file(
                    state.library.as_ref(),
                    replaced,
                    &request.file_name,
                    size,
                ) {
                    warning = Some(
                        "Файл установлен, но запись библиотеки не обновилась — перечитайте список."
                            .to_string(),
                    );
                }
            }
            if update.install.success
                && let Some(version) = request.version.as_deref()
                && let Some(path) = update.install.path.as_ref()
            {
                remember_installed_version(state, &request.file_name, version, path);
            }
            let backup_id = if update.install.success {
                installer
                    .list_backups(replaced)
                    .first()
                    .map(|entry| entry.id.clone())
            } else {
                None
            };
            if update.install.success {
                prune_versions(
                    state,
                    &installer,
                    &request.file_name,
                    keep,
                    backup_id.as_deref(),
                );
            }
            results.push(IncomingInstallResult {
                success: update.install.success,
                message: update.install.message,
                warning,
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
                warning: None,
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

/// «Хранить копий» — на мод: после обновления стираем самые старые копии его
/// прошлых версий сверх настройки. Не трогаем копию, которую ждёт «Отменить» этого
/// обновления (`undo`), — даже если часы перевели назад и она вышла «старше» других, —
/// и копии, которые ждёт «Отменить» удаления.
fn prune_versions(
    state: &AppState,
    installer: &bmm_packs::install::ModInstaller,
    file_name: &str,
    keep: usize,
    undo: Option<&str>,
) -> usize {
    let Some(installed) = state
        .library
        .mods()
        .into_iter()
        .find(|item| item.file_name.eq_ignore_ascii_case(file_name))
    else {
        return 0;
    };
    let mut held: std::collections::HashSet<String> = state
        .deleted
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .values()
        .map(|ticket| ticket.backup_id.clone())
        .collect();
    held.extend(undo.map(str::to_string));
    bmm_packs::versions::surplus(installer, state.library.as_ref(), &installed, keep)
        .into_iter()
        .filter(|entry| !held.contains(&entry.id))
        .filter(|entry| installer.delete_backup(&entry.file_name, &entry.id).success)
        .count()
}

/// Версия, которую человек поставил со страницы, — в запись библиотеки вместе с
/// размером файла: так сверка с папкой не примет обновление за внешнюю замену и не
/// перепишет версию из имени файла.
fn remember_installed_version(state: &AppState, file_name: &str, version: &str, path: &Path) {
    let version = version.trim();
    if version.is_empty() {
        return;
    }
    let Some(mut installed) = state
        .library
        .mods()
        .into_iter()
        .find(|item| item.file_name.eq_ignore_ascii_case(file_name))
    else {
        return;
    };
    installed.version = Some(version.to_string());
    if let Some(size) = fs::metadata(path)
        .ok()
        .and_then(|meta| i64::try_from(meta.len()).ok())
    {
        installed.size_bytes = Some(size);
    }
    state.library.upsert_mod(installed);
}

fn undo_install(state: &AppState, ticket: &IncomingTicketDto) -> Result<(), AppError> {
    let _guard = state
        .load_lock
        .lock()
        .map_err(|_| AppError::new("lock", "не удалось заблокировать библиотеку"))?;
    let installer = installer_for(state)?;
    if let Some(replaced) = &ticket.replaces {
        // Новый файл убираем первым и проверяем, что убрался: иначе «Отменить»
        // говорило «готово», а в папке лежали обе версии мода.
        let mut new_copy: Option<(String, bool)> = None;
        if !ticket.same_name {
            let layout = installer.layout();
            let enabled = layout.mods_dir.join(&ticket.file_name).is_file();
            let disabled = layout.disabled_dir.join(&ticket.file_name).is_file();
            if enabled || disabled {
                let removed = installer.remove(&ticket.file_name);
                if !removed.success {
                    return Err(AppError::new("undo", removed.message));
                }
                new_copy = removed
                    .path
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .map(|name| (name.to_string_lossy().into_owned(), disabled && !enabled));
            }
        }
        if let Some(backup_id) = &ticket.backup_id {
            let result = installer.restore_backup(replaced, backup_id);
            if !result.success {
                // Старую версию вернуть не вышло — кладём новую обратно, чтобы мод
                // не пропал совсем.
                if let Some((id, into_disabled)) = &new_copy {
                    let _ = installer.unremove(&ticket.file_name, id, *into_disabled);
                }
                return Err(AppError::new("undo", result.message));
            }
            // Выключить не вышло — библиотеку всё равно приводим к вернувшемуся
            // файлу, а об ошибке говорим в конце.
            let mut disable_error = None;
            if ticket.was_disabled {
                let disabled = installer.disable(replaced);
                if !disabled.success {
                    disable_error = Some(format!(
                        "Прежняя версия возвращена, но выключить её не удалось: {}",
                        disabled.message
                    ));
                }
            }
            // Запись библиотеки возвращается на прежний файл вместе со страницей мода.
            if !ticket.same_name {
                let size = result
                    .path
                    .as_ref()
                    .and_then(|path| fs::metadata(path).ok())
                    .and_then(|meta| i64::try_from(meta.len()).ok())
                    .unwrap_or(0);
                if !bmm_domain::library::rename_file(
                    state.library.as_ref(),
                    &ticket.file_name,
                    replaced,
                    size,
                ) {
                    return Err(AppError::new(
                        "undo",
                        "Файл восстановлен, но запись библиотеки не обновилась — перечитайте список.",
                    ));
                }
            }
            if let Some(message) = disable_error {
                return Err(AppError::new("undo", message));
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

    /// Архив прошёл проверку окна: только такой можно ставить.
    fn checked_source(path: &Path) {
        remember_checked(path, Fingerprint::of(path).unwrap());
    }
    use std::io::Write;

    use bmm_domain::clock::system_clock;
    use bmm_packs::install::{InstallOptions, ModInstaller};

    fn setup() -> (tempfile::TempDir, Arc<AppState>) {
        let temp = tempfile::tempdir().unwrap();
        let mods = temp.path().join("mods");
        fs::create_dir_all(&mods).unwrap();
        let state = Arc::new(crate::test_support::test_state(&temp, &mods));
        (temp, state)
    }

    fn installer_for_test(state: &AppState) -> ModInstaller {
        installer_for(state).unwrap()
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
    fn every_dropped_archive_gets_an_answer() {
        let (temp, state) = setup();
        let mut paths = Vec::new();
        for index in 0..12 {
            let path = temp.path().join(format!("mod{index}.zip"));
            make_zip(&path, b"{}");
            paths.push(path.display().to_string());
        }

        let result = inspect_paths(&state, &paths).unwrap();

        assert_eq!(result.len(), 12, "ни один архив не потерялся");
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
        state.library.upsert_mod(bmm_domain::library::InstalledMod {
            id: "car_v1.zip".to_string(),
            name: "Car".to_string(),
            file_name: "car_v1.zip".to_string(),
            version: Some("1.0".to_string()),
            size_bytes: Some(3),
            installed_at_utc: system_clock()(),
            enabled: true,
        });
        assert!(
            installer
                .install_from_path(&v1, "car_v1.zip", InstallOptions::default())
                .success
        );
        assert!(installer.disable("car_v1.zip").success);

        checked_source(&v2);
        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: v2.display().to_string(),
                file_name: "car_v2.zip".to_string(),
                replaces: Some("car_v1.zip".to_string()),
                allow_warnings: true,
                version: None,
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
    fn update_remembers_the_version_of_its_page() {
        // Обновили «Манекен» со STMods до 1.2: файл «Dummy v.0.20.2.zip», внутри
        // архива версии нет. Версия — со страницы, а не 0.20.2 из имени.
        let (temp, state) = setup();
        let v1 = temp.path().join("v1.zip");
        let v2 = temp.path().join("v2.zip");
        make_zip(&v1, b"one");
        make_zip(&v2, b"two, longer");
        let installer = installer_for_test(&state);
        assert!(
            installer
                .install_from_path(&v1, "dummy.zip", InstallOptions::default())
                .success
        );
        state.library.upsert_mod(bmm_domain::library::InstalledMod {
            id: "dummy.zip".to_string(),
            name: "Crash Test Dummy".to_string(),
            file_name: "dummy.zip".to_string(),
            version: Some("1.1".to_string()),
            size_bytes: Some(fs::metadata(&v1).unwrap().len() as i64),
            installed_at_utc: system_clock()(),
            enabled: true,
        });
        checked_source(&v2);
        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: v2.display().to_string(),
                file_name: "Dummy v.0.20.2.zip".to_string(),
                replaces: Some("dummy.zip".to_string()),
                allow_warnings: true,
                version: Some("1.2".to_string()),
            }],
        )
        .unwrap();
        assert!(results[0].success, "{:?}", results[0].message);
        let after = state.library.get_mod("dummy.zip").unwrap();
        assert_eq!(after.file_name, "Dummy v.0.20.2.zip");
        assert_eq!(after.version.as_deref(), Some("1.2"));
        // Сверка с папкой версию не перепишет: размер в записи — от нового файла.
        let on_disk = installer.layout().mods_dir.join("Dummy v.0.20.2.zip");
        assert_eq!(
            after.size_bytes,
            Some(fs::metadata(on_disk).unwrap().len() as i64)
        );
    }

    #[test]
    fn failed_undo_of_update_keeps_the_new_version() {
        // Старую версию вернуть не вышло — новая не должна пропасть вместе с ней:
        // раньше в папке не оставалось ни одной версии мода.
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
        checked_source(&v2);
        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: v2.display().to_string(),
                file_name: "car_v2.zip".to_string(),
                replaces: Some("car_v1.zip".to_string()),
                allow_warnings: true,
                version: None,
            }],
        )
        .unwrap();
        assert!(results[0].success);
        let error = undo_install(
            &state,
            &IncomingTicketDto {
                file_name: "car_v2.zip".to_string(),
                replaces: Some("car_v1.zip".to_string()),
                backup_id: Some("пропавшая-копия".to_string()),
                was_disabled: false,
                same_name: false,
            },
        );
        assert!(error.is_err(), "неудача — это ошибка, а не «готово»");
        assert!(installer.layout().mods_dir.join("car_v2.zip").is_file());
        assert!(!installer.layout().mods_dir.join("car_v1.zip").exists());
    }

    #[test]
    fn install_window_and_rust_agree_on_values() {
        use crate::test_support::ts_union;
        let (temp, state) = setup();
        let source = temp.path().join("new.zip");
        make_zip(&source, b"body");

        let archives = inspect_paths(&state, &[source.display().to_string()]).unwrap();
        let archive = serde_json::to_value(&archives[0]).unwrap();
        let item = &archive["items"][0];
        assert!(ts_union("unions.ts", "IncomingKind").contains(item["kind"].as_str().unwrap()));
        let severities = ts_union("unions.ts", "FindingSeverity");
        for severity in ["info", "warning", "block"] {
            assert!(severities.contains(severity));
        }
        let kinds = ts_union("unions.ts", "IncomingKind");
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
        checked_source(&source);
        let results = install_items(&state, &[request]).unwrap();
        let result = serde_json::to_value(&results[0]).unwrap();

        // Отмена приходит из окна билетом — тем же набором полей.
        let ticket: IncomingTicketDto = serde_json::from_value(serde_json::json!({
            "fileName": result["fileName"],
            "replaces": result["replaces"],
            "backupId": result["backupId"],
            "wasDisabled": result["wasDisabled"],
            "sameName": result["sameName"],
        }))
        .unwrap();
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
        checked_source(&path);
        assert!(remove_download_source(&state, &path.display().to_string()).unwrap());
        assert!(!path.exists(), "включённая настройка удаляет архив");
    }

    fn new_mod_request(source: &Path) -> IncomingInstallRequest {
        IncomingInstallRequest {
            zip_path: source.display().to_string(),
            file_name: "new_mod.zip".to_string(),
            replaces: None,
            allow_warnings: true,
            version: None,
        }
    }

    fn installed_new_mod(state: &AppState) -> bool {
        installer_for_test(state)
            .layout()
            .mods_dir
            .join("new_mod.zip")
            .exists()
    }

    #[test]
    fn archive_checked_by_the_window_installs() {
        // Настоящий путь: окно проверяет архив, потом ставит то, что проверило.
        let (temp, state) = setup();
        let source = temp.path().join("new.zip");
        make_zip(&source, b"body");
        let found = inspect_paths(&state, &[source.display().to_string()]).unwrap();
        assert!(found[0].problem.is_none(), "{:?}", found[0].problem);
        let item = &found[0].items[0];
        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: item.zip_path.clone(),
                file_name: "new_mod.zip".to_string(),
                replaces: None,
                allow_warnings: true,
                version: None,
            }],
        )
        .unwrap();
        assert!(results[0].success, "{:?}", results[0].message);
    }

    #[test]
    fn archive_swapped_after_the_check_is_not_installed() {
        // Было: после «Архив проверен» файл в «Загрузках» подменили другим с тем же
        // именем — ставился непроверенный архив.
        let (temp, state) = setup();
        let source = temp.path().join("new.zip");
        make_zip(&source, b"checked body");
        checked_source(&source);
        make_zip(&source, b"another, unchecked body");
        let results = install_items(&state, &[new_mod_request(&source)]).unwrap();
        assert!(!results[0].success);
        assert!(
            results[0].message.as_deref().unwrap().contains("изменился"),
            "{:?}",
            results[0].message
        );
        assert!(!installed_new_mod(&state));
    }

    #[test]
    fn archive_removed_after_the_check_says_so() {
        let (temp, state) = setup();
        let source = temp.path().join("new.zip");
        make_zip(&source, b"body");
        checked_source(&source);
        std::fs::rename(&source, temp.path().join("renamed.zip")).unwrap();
        let results = install_items(&state, &[new_mod_request(&source)]).unwrap();
        assert!(!results[0].success);
        assert!(
            results[0].message.as_deref().unwrap().contains("пропал"),
            "{:?}",
            results[0].message
        );
    }

    #[test]
    fn unchecked_archive_is_not_installed() {
        let (temp, state) = setup();
        let source = temp.path().join("never_checked.zip");
        make_zip(&source, b"body");
        forget_checked(&source);
        let results = install_items(&state, &[new_mod_request(&source)]).unwrap();
        assert!(!results[0].success);
        assert!(!installed_new_mod(&state));
    }

    #[test]
    fn one_bad_archive_does_not_stop_the_rest() {
        // Пачка: один архив пропал — остальные всё равно ставятся.
        let (temp, state) = setup();
        let gone = temp.path().join("gone.zip");
        let fine = temp.path().join("fine.zip");
        make_zip(&gone, b"gone");
        make_zip(&fine, b"fine");
        checked_source(&gone);
        checked_source(&fine);
        std::fs::remove_file(&gone).unwrap();
        let mut second = new_mod_request(&fine);
        second.file_name = "fine_mod.zip".to_string();
        let results = install_items(&state, &[new_mod_request(&gone), second]).unwrap();
        assert!(!results[0].success);
        assert!(results[1].success, "{:?}", results[1].message);
    }

    #[test]
    fn archive_changing_during_the_check_is_not_remembered() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        std::fs::write(&path, b"part").unwrap();
        let before = Fingerprint::of(&path).ok();
        std::fs::write(&path, b"part and the rest").unwrap();
        let after = Fingerprint::of(&path).ok();
        let problem = checked_print(before.clone(), after).unwrap_err();
        assert!(problem.contains("менялся"), "{problem}");
        assert!(checked_print(before.clone(), before.clone()).is_ok());
        assert!(
            checked_print(before, None).is_err(),
            "пропал посреди проверки"
        );
    }

    #[test]
    fn own_copies_in_the_work_folder_are_not_compared() {
        let root = incoming_root();
        assert_eq!(
            install_verdict(&root.join("123-1").join("repacked-1.zip"), &root),
            fingerprint::Verdict::Same
        );
    }

    #[test]
    fn another_file_in_place_of_the_installed_archive_is_not_deleted() {
        // Поставили архив, а под тем же именем уже лежит другой (скачали заново):
        // удаление «скачанного архива после установки» не должно его тронуть.
        let temp = tempfile::tempdir().unwrap();
        let state = crate::test_support::test_state(&temp, temp.path());
        let downloads = temp.path().join("downloads");
        std::fs::create_dir_all(&downloads).unwrap();
        let path = downloads.join("mod.zip");
        std::fs::write(&path, b"installed one").unwrap();
        let mut settings = state.settings.load().settings;
        settings.downloads_folder = Some(downloads.display().to_string());
        settings.delete_downloads_after_install = true;
        state.settings.save(&settings).unwrap();
        checked_source(&path);
        std::fs::write(&path, b"a newer download").unwrap();
        assert!(!remove_download_source(&state, &path.display().to_string()).unwrap());
        assert!(path.is_file());
    }

    #[test]
    fn install_new_mod_appears_in_library() {
        let (temp, state) = setup();
        let source = temp.path().join("new.zip");
        make_zip(&source, b"body");
        checked_source(&source);
        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: source.display().to_string(),
                file_name: "new_mod.zip".to_string(),
                replaces: None,
                allow_warnings: true,
                version: None,
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

        checked_source(&v2);
        let results = install_items(
            &state,
            &[IncomingInstallRequest {
                zip_path: v2.display().to_string(),
                file_name: "GMC_Acadia_GMT968.zip".to_string(),
                replaces: Some("acadia.zip".to_string()),
                allow_warnings: true,
                version: None,
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

    #[test]
    fn copies_of_past_versions_are_kept_per_mod_not_per_file_name() {
        // Обновление обычно меняет имя файла: у каждого имени была одна копия, и
        // «Хранить копий: 2» не ограничивало ничего — копии копились гигабайтами.
        let (temp, state) = setup();
        let mut settings = state.settings.load().settings;
        settings.keep_backups = 2;
        state.settings.save(&settings).unwrap();
        let installer = installer_for_test(&state);
        let first = temp.path().join("hotw_v1.zip");
        make_zip(&first, b"one");
        assert!(
            installer
                .install_from_path(&first, "hotw_v1.zip", InstallOptions::default())
                .success
        );
        let id = crate::commands::library::load_snapshot(&state)
            .unwrap()
            .mods[0]
            .id
            .clone();

        let mut previous = "hotw_v1.zip".to_string();
        let mut last = None;
        for version in 2..=5 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let name = format!("hotw_v{version}.zip");
            let archive = temp.path().join(format!("download-{version}.zip"));
            make_zip(
                &archive,
                format!("body {version}").repeat(version).as_bytes(),
            );
            checked_source(&archive);
            let results = install_items(
                &state,
                &[IncomingInstallRequest {
                    zip_path: archive.display().to_string(),
                    file_name: name.clone(),
                    replaces: Some(previous.clone()),
                    allow_warnings: true,
                    version: None,
                }],
            )
            .unwrap();
            assert!(results[0].success, "{:?}", results[0].message);
            last = results[0].backup_id.clone();
            previous = name;
        }

        let installed = state.library.get_mod(&id).unwrap();
        assert_eq!(installed.file_name, "hotw_v5.zip");
        let versions =
            bmm_packs::versions::versions_for(&installer, state.library.as_ref(), &installed);
        let names: Vec<&str> = versions
            .iter()
            .map(|version| version.backup.file_name.as_str())
            .collect();
        assert_eq!(names, ["hotw_v4.zip", "hotw_v3.zip"], "две самые новые");
        assert!(
            installer.list_backups("hotw_v1.zip").is_empty(),
            "старые копии стёрты с диска"
        );

        // «Отменить» последнего обновления всё ещё работает: его копия самая новая.
        undo_install(
            &state,
            &IncomingTicketDto {
                file_name: "hotw_v5.zip".to_string(),
                replaces: Some("hotw_v4.zip".to_string()),
                backup_id: last,
                was_disabled: false,
                same_name: false,
            },
        )
        .unwrap();
        assert_eq!(state.library.get_mod(&id).unwrap().file_name, "hotw_v4.zip");
    }

    #[test]
    fn copy_held_by_undo_of_a_delete_is_not_pruned() {
        let (temp, state) = setup();
        let mut settings = state.settings.load().settings;
        settings.keep_backups = 1;
        state.settings.save(&settings).unwrap();
        let installer = installer_for_test(&state);
        let first = temp.path().join("car_v1.zip");
        make_zip(&first, b"one");
        installer.install_from_path(&first, "car_v1.zip", InstallOptions::default());
        let id = crate::commands::library::load_snapshot(&state)
            .unwrap()
            .mods[0]
            .id
            .clone();
        let mut previous = "car_v1.zip".to_string();
        for version in 2..=3 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let name = format!("car_v{version}.zip");
            let archive = temp.path().join(format!("download-{version}.zip"));
            make_zip(
                &archive,
                format!("body {version}").repeat(version).as_bytes(),
            );
            if version == 3 {
                // Копию v1 держит «Отменить» удаления — пока тост виден, её не стираем.
                let held = installer.list_backups("car_v1.zip")[0].id.clone();
                state.deleted.lock().unwrap().insert(
                    "other".to_string(),
                    crate::state::DeletedTicket {
                        mod_id: "other".to_string(),
                        file_name: "car_v1.zip".to_string(),
                        backup_id: held,
                        was_in_disabled: false,
                        was_enabled: true,
                    },
                );
            }
            checked_source(&archive);
            let results = install_items(
                &state,
                &[IncomingInstallRequest {
                    zip_path: archive.display().to_string(),
                    file_name: name.clone(),
                    replaces: Some(previous.clone()),
                    allow_warnings: true,
                    version: None,
                }],
            )
            .unwrap();
            assert!(results[0].success, "{:?}", results[0].message);
            previous = name;
        }
        assert_eq!(state.library.get_mod(&id).unwrap().file_name, "car_v3.zip");
        assert_eq!(
            installer.list_backups("car_v1.zip").len(),
            1,
            "держит «Отменить»"
        );
        assert_eq!(
            installer.list_backups("car_v2.zip").len(),
            1,
            "только что сделанная"
        );
    }

    #[test]
    fn copy_for_undo_survives_a_clock_turned_back() {
        // Часы перевели назад: копия, только что сделанная обновлением, выглядит старше
        // прежних. «Отменить» без неё не вернуло бы прошлую версию.
        let (temp, state) = setup();
        let layout = installer_for_test(&state).layout().clone();
        let tick = Arc::new(std::sync::atomic::AtomicI64::new(100));
        let clock_tick = tick.clone();
        let installer = ModInstaller::new(
            layout,
            Arc::new(move || {
                system_clock()()
                    - chrono::TimeDelta::seconds(
                        clock_tick.fetch_sub(10, std::sync::atomic::Ordering::SeqCst),
                    )
            }),
            Arc::new(|| 1),
        );
        let source = temp.path().join("source.zip");
        make_zip(&source, b"body");
        installer.install_from_path(&source, "car_v1.zip", InstallOptions::default());
        installer.update_from_path(
            "car_v1.zip",
            &source,
            "car_v2.zip",
            InstallOptions::default(),
        );
        tick.store(1000, std::sync::atomic::Ordering::SeqCst);
        installer.update_from_path(
            "car_v2.zip",
            &source,
            "car_v3.zip",
            InstallOptions::default(),
        );
        state.library.upsert_mod(bmm_domain::library::InstalledMod {
            id: "car".to_string(),
            name: "Car".to_string(),
            file_name: "car_v3.zip".to_string(),
            version: None,
            size_bytes: Some(1),
            installed_at_utc: system_clock()(),
            enabled: true,
        });
        state.library.add_file_history("car", "car_v1.zip");
        state.library.add_file_history("car", "car_v2.zip");
        let undo = installer.list_backups("car_v2.zip")[0].id.clone();

        prune_versions(&state, &installer, "car_v3.zip", 1, Some(&undo));
        assert_eq!(
            installer.list_backups("car_v2.zip").len(),
            1,
            "копия для «Отменить»"
        );

        prune_versions(&state, &installer, "car_v3.zip", 1, None);
        assert!(
            installer.list_backups("car_v2.zip").is_empty(),
            "без «Отменить» лишняя копия уходит"
        );
        assert_eq!(installer.list_backups("car_v1.zip").len(), 1);
    }

    #[test]
    fn updates_under_the_same_name_keep_the_setting_too() {
        let (temp, state) = setup();
        let mut settings = state.settings.load().settings;
        settings.keep_backups = 2;
        state.settings.save(&settings).unwrap();
        let installer = installer_for_test(&state);
        let first = temp.path().join("first.zip");
        make_zip(&first, b"one");
        installer.install_from_path(&first, "dummy.zip", InstallOptions::default());
        crate::commands::library::load_snapshot(&state).unwrap();
        for version in 2..=5 {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let archive = temp.path().join(format!("download-{version}.zip"));
            make_zip(
                &archive,
                format!("body {version}").repeat(version).as_bytes(),
            );
            checked_source(&archive);
            let results = install_items(
                &state,
                &[IncomingInstallRequest {
                    zip_path: archive.display().to_string(),
                    file_name: "dummy.zip".to_string(),
                    replaces: Some("dummy.zip".to_string()),
                    allow_warnings: true,
                    version: None,
                }],
            )
            .unwrap();
            assert!(results[0].success, "{:?}", results[0].message);
            assert!(results[0].backup_id.is_some());
        }
        assert_eq!(installer.list_backups("dummy.zip").len(), 2);
    }
}

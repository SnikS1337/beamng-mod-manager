//! Снимок библиотеки: диск, db.json, повторы и карточки.

use super::*;

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
    let _ = bmm_game::bridge::GameBridge::ensure_installed(&mods_dir);

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
    } else {
        // db.json занят/недочитан: не считаем все моды включёнными и не
        // перезаписываем библиотеку. Выключенное состояние берём из неё.
        let known: HashMap<String, bool> = state
            .library
            .mods()
            .into_iter()
            .map(|item| (item.file_name.to_lowercase(), item.enabled))
            .collect();
        for disk in &mut scan_result.mods {
            if let Some(enabled) = known.get(&disk.file_name.to_lowercase())
                && !enabled
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
    let sync_report = bmm_domain::library::reconcile_with_complete(
        state.library.as_ref(),
        &scan_result.mods,
        scan_result.folder_available,
        scan_result.complete,
        Utc::now().fixed_offset(),
    );

    if !sync_report.added.is_empty() {
        let mut by_file: HashMap<String, bmm_domain::library::InstalledMod> = state
            .library
            .mods()
            .into_iter()
            .map(|item| (item.file_name.to_lowercase(), item))
            .collect();
        for file_name in &sync_report.added {
            let Some(added) = db_read.find(file_name).and_then(|entry| entry.date_added) else {
                continue;
            };
            let Some(item) = by_file.get(&file_name.to_lowercase()).cloned() else {
                continue;
            };
            if added < item.installed_at_utc {
                let mut updated = item;
                updated.installed_at_utc = added;
                state.library.upsert_mod(updated.clone());
                by_file.insert(updated.file_name.to_lowercase(), updated);
            }
        }
    }

    apply_repo_info(state, &db_read, &disks_by_file);

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
    let update_findings: HashMap<String, (String, Option<String>)> =
        bmm_checks::UpdateCheckService::pending(state.library.as_ref(), false)
            .into_iter()
            .map(|finding| {
                (
                    finding.mod_id,
                    (
                        match finding.status {
                            bmm_domain::versioning::UpdateStatus::Likely => "likely",
                            bmm_domain::versioning::UpdateStatus::Possible => "possible",
                            bmm_domain::versioning::UpdateStatus::None => "none",
                        }
                        .to_string(),
                        finding.latest_version,
                    ),
                )
            })
            .collect();

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
    let archive_titles = state.previews.titles_shared();
    let archive_problems = state.previews.problems_shared();
    let archive_infos = state.previews.mod_infos_shared();
    let archive_contents = state.previews.contents_shared();

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
            let archive_title = preview_keys
                .get(&installed.id)
                .and_then(|key| archive_titles.get(key))
                .map(String::as_str)
                .filter(|title| !title.is_empty());
            let name = ModTitles::choose(repo_title, archive_title, site_title, &installed.name);
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
            let (update, update_version) = update_findings
                .get(&installed.id)
                .cloned()
                .unwrap_or_else(|| ("none".to_string(), None));
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
                update,
                update_version,
                unchecked: !confirmed_ids.contains(&installed.id),
                pending: if pending { "queued" } else { "none" }.to_string(),
                from_game_repo: disk.and_then(|disk| disk.sub_folder.as_ref()).is_some(),
                deleted: false,
                copies: 0,
                saved_at: None,
                // Моды из репозитория игры кладёт сама игра — их упаковку не судим.
                problem: preview_keys
                    .get(&installed.id)
                    .and_then(|key| archive_problems.get(key))
                    .filter(|code| !code.is_empty())
                    .filter(|_| disk.and_then(|disk| disk.sub_folder.as_ref()).is_none())
                    .cloned(),
                duplicate_of: None,
                group: Vec::new(),
                group_conflict: false,
                role: None,
            }
        })
        .collect();
    let modified: HashMap<String, i64> = state
        .library
        .mods()
        .iter()
        .filter_map(|installed| {
            let disk = disks_by_file.get(&installed.file_name.to_lowercase())?;
            Some((installed.id.clone(), disk.modified_utc?.timestamp()))
        })
        .collect();
    let mods = mark_duplicates(
        mods,
        &preview_keys,
        &archive_contents,
        &archive_infos,
        &modified,
    );

    let ticket_files: HashSet<String> = tickets_before
        .values()
        .map(|ticket| ticket.file_name.to_lowercase())
        .collect();
    let deleted_cards: Vec<ModCardDto> = versions::deleted(&installer, state.library.as_ref())
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
                update_version: None,
                unchecked: false,
                pending: "none".to_string(),
                from_game_repo: false,
                deleted: true,
                copies: item.copies,
                saved_at: item.last_saved_at.map(|date| date.to_rfc3339()),
                problem: None,
                duplicate_of: None,
                group: Vec::new(),
                group_conflict: false,
                role: None,
            }
        })
        .collect();

    // Счётчики фильтров — по видимым карточкам, а не по файлам: скрытая копия
    // не должна давать «С обновлением 2» при одной карточке в списке.
    let visible = || mods.iter().filter(|item| item.duplicate_of.is_none());
    let all = visible().count();
    let enabled = visible().filter(|item| item.enabled).count();
    let unchecked = visible().filter(|item| item.unchecked).count();
    let updates = visible().filter(|item| item.update != "none").count();
    let enabled_files = mods.iter().filter(|item| item.enabled).count();
    let warning = join_warnings(
        settings_load.warnings,
        scan_warnings,
        db_read.warnings,
        sync_report.warning,
        library_warnings(state.library.load_warnings(), state.library.save_error()),
    );
    let many_enabled = (settings.active_mods_warn_above > 0
        && enabled_files as i32 > settings.active_mods_warn_above)
        .then(|| many_enabled_text(enabled_files));

    Ok(LibrarySnapshot {
        counts: FilterCounts {
            all,
            enabled,
            updates,
            unchecked,
            deleted: deleted_cards.len(),
        },
        mods,
        folder: Some(mods_dir.display().to_string()),
        folder_error,
        game_version,
        game_running,
        warning,
        many_enabled,
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
    if let Some(found) = folders::auto_detect(&folders::game_dirs(), &roots) {
        return (Some(found), None);
    }
    (None, Some("Папка игры не выбрана.".to_string()))
}

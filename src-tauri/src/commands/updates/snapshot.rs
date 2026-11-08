//! Снимок проверки и его DTO.

use super::*;

pub(crate) fn build_snapshot(state: &AppState) -> UpdatesSnapshotDto {
    let store = state.library.as_ref();
    // Как на карточках модов: название и картинка из архива, если уже прочитаны.
    let titles = state.previews.titles_shared();
    let keys: HashMap<String, String> = state
        .previews_by_id
        .lock()
        .unwrap()
        .iter()
        .map(|(id, target)| (id.clone(), target.key.clone()))
        .collect();
    let contents = state.previews.contents_shared();
    let infos = state.previews.mod_infos_shared();
    let vehicle_types = state.previews.vehicle_types_shared();
    let role_of = |mod_id: &str| -> Option<String> {
        let key = keys.get(mod_id)?;
        let empty: &[String] = &[];
        let content = contents.get(key).map(Vec::as_slice).unwrap_or(empty);
        let hint = infos
            .get(key)
            .and_then(crate::commands::library::mod_info_hint);
        let types = crate::commands::library::types_from_cache(Some(key), &vehicle_types);
        bmm_packs::duplicates::role_of_full(content, hint, &types)
            .map(|role| role.code().to_string())
    };
    let name_of = |mod_id: &str| -> String {
        keys.get(mod_id)
            .and_then(|key| titles.get(key))
            .filter(|title| !title.is_empty())
            .cloned()
            .or_else(|| store.get_mod(mod_id).map(|item| item.name))
            .unwrap_or_else(|| mod_id.to_string())
    };
    let preview_of = |mod_id: &str| -> Option<String> {
        let key = keys.get(mod_id)?;
        match state.previews.get(key) {
            Some(bmm_packs::cache::CachedPreview::Image { extension, .. }) => {
                Some(crate::commands::library::preview_url(key, &extension))
            }
            _ => None,
        }
    };
    let last_report_sources = state
        .update
        .as_ref()
        .and_then(|runtime| runtime.coordinator.last_report())
        .map(|report| report.sources)
        .unwrap_or_default();
    let down = down_now(state, &last_report_sources);
    let off_sites = state.settings.load().settings.disabled_sources;
    let finding = |finding: UpdateFinding| -> UpdateFindingDto {
        let check = store.get_check(&finding.mod_id, &finding.source);
        let mut dto = finding_dto(&name_of, finding);
        mark_down(&mut dto, &down, &off_sites);
        dto.role = role_of(&dto.mod_id);
        dto.preview_url = preview_for_role(dto.role.as_deref(), preview_of(&dto.mod_id));
        dto.installed_size = store.get_mod(&dto.mod_id).and_then(|item| item.size_bytes);
        dto.latest_size = check.as_ref().and_then(|check| check.last_size);
        dto.noticed_at = check
            .as_ref()
            .and_then(|check| check.last_changed_utc)
            .map(|at| at.to_rfc3339());
        dto
    };
    let checks = store.checks();
    let last_checked = checks
        .iter()
        .map(|check| check.last_checked_utc)
        .chain(store.last_run())
        .max()
        .map(|at| at.to_rfc3339());
    let statuses = store.source_statuses();
    let disabled = state.settings.load().settings.disabled_sources;
    let check_disabled = state.settings.load().settings.check_disabled_mods;
    let tracked: Vec<ModBinding> = store
        .bindings()
        .into_iter()
        .filter(|binding| binding.confirmed)
        .filter(|binding| {
            check_disabled
                || store
                    .get_mod(&binding.mod_id)
                    .map(|installed| installed.enabled)
                    .unwrap_or(true)
        })
        .collect();
    let sites = state
        .update
        .as_ref()
        .map(|runtime| {
            runtime
                .sources
                .iter()
                .map(|source| {
                    let id = source.id().to_string();
                    let off = disabled.iter().any(|item| item.eq_ignore_ascii_case(&id));
                    let status = if off {
                        "off"
                    } else {
                        match statuses.get(&id) {
                            Some(bmm_domain::library::SourceStatus::NeedsHumanCheck) => {
                                "human_check"
                            }
                            Some(bmm_domain::library::SourceStatus::Unavailable) => "unavailable",
                            _ => "ok",
                        }
                    };
                    UpdateSiteDto {
                        tracked: tracked
                            .iter()
                            .filter(|binding| binding.source == id)
                            .count(),
                        display_name: source.display_name().to_string(),
                        status: status.to_string(),
                        last_checked: checks
                            .iter()
                            .filter(|check| check.source == id)
                            .map(|check| check.last_checked_utc)
                            .max()
                            .map(|at| at.to_rfc3339()),
                        note: last_report_sources
                            .iter()
                            .find(|report| report.source == id)
                            .and_then(|report| report.note.clone()),
                        id,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    UpdatesSnapshotDto {
        findings: UpdateCheckService::pending(store, false)
            .into_iter()
            .map(&finding)
            .collect(),
        hidden: UpdateCheckService::pending(store, true)
            .into_iter()
            .filter(|item| item.is_dismissed)
            .map(&finding)
            .collect(),
        suggestions: bmm_checks::suggestions(store)
            .into_iter()
            .map(|binding| suggestion_dto(&name_of, binding))
            .collect(),
        sites,
        last_checked,
        lost: bmm_checks::BindingRepair::dead(store)
            .into_iter()
            .map(|dead| {
                let role = role_of(&dead.installed.id);
                LostPageDto {
                    mod_name: name_of(&dead.installed.id),
                    preview_url: preview_for_role(role.as_deref(), preview_of(&dead.installed.id)),
                    role,
                    mod_id: dead.installed.id,
                    source: dead.binding.source,
                    url: dead.binding.url.to_string(),
                }
            })
            .collect(),
        coverage: coverage(state),
    }
}

/// Сколько модов программа проверяет. Копии одного мода — одна позиция, как в гараже:
/// иначе четыре «weather_fx» дали бы четыре мода без страницы.
pub(crate) fn coverage(state: &AppState) -> CoverageDto {
    let store = state.library.as_ref();
    let mods = store.mods();
    let keys: HashMap<String, String> = state
        .previews_by_id
        .lock()
        .unwrap()
        .iter()
        .map(|(id, target)| (id.clone(), target.key.clone()))
        .collect();
    let contents = state.previews.contents_shared();
    let empty: Vec<String> = Vec::new();
    let items: Vec<bmm_packs::duplicates::DupItem> = mods
        .iter()
        .map(|item| bmm_packs::duplicates::DupItem {
            id: &item.id,
            file_name: &item.file_name,
            size: item.size_bytes.unwrap_or(0),
            content: keys
                .get(&item.id)
                .and_then(|key| contents.get(key))
                .unwrap_or(&empty),
            version: item.version.as_deref(),
            modified: None,
        })
        .collect();
    let copies = bmm_packs::duplicates::find(&items);
    let followed: std::collections::HashSet<String> = store
        .bindings()
        .into_iter()
        .filter(|binding| binding.confirmed)
        .map(|binding| binding.mod_id)
        .collect();
    let ids: Vec<&str> = mods.iter().map(|item| item.id.as_str()).collect();
    tally(&ids, &copies, &followed, &|id| {
        store.is_checking_declined(id)
    })
}

/// Подсчёт по группам копий. Группа отслеживается, если отслеживается любая её копия,
/// и «без проверки», если от проверки отказались у любой копии: отказ мог быть
/// сделан у копии, а не у основного файла.
pub(crate) fn tally(
    ids: &[&str],
    copies: &HashMap<String, bmm_packs::duplicates::Duplicate>,
    followed: &std::collections::HashSet<String>,
    declined: &dyn Fn(&str) -> bool,
) -> CoverageDto {
    let mut groups: HashMap<&str, Vec<&str>> = HashMap::new();
    for id in ids {
        let head = copies.get(*id).map_or(*id, |copy| copy.of.as_str());
        groups.entry(head).or_default().push(id);
    }
    let mut result = CoverageDto {
        mods: groups.len(),
        ..CoverageDto::default()
    };
    for members in groups.values() {
        if members.iter().any(|id| followed.contains(*id)) {
            result.tracked += 1;
        } else if members.iter().any(|id| declined(id)) {
            result.declined += 1;
        } else {
            result.without_page += 1;
        }
    }
    result
}

pub(crate) fn finding_dto(
    name_of: &dyn Fn(&str) -> String,
    finding: UpdateFinding,
) -> UpdateFindingDto {
    let name = name_of(&finding.mod_id);
    UpdateFindingDto {
        mod_id: finding.mod_id,
        mod_name: name,
        source: finding.source,
        status: status_text(finding.status).to_string(),
        installed_version: finding.installed_version,
        latest_version: finding.latest_version,
        url: finding.url.to_string(),
        dismissed: finding.is_dismissed,
        preview_url: None,
        role: None,
        installed_size: None,
        latest_size: None,
        noticed_at: None,
        author_version: finding.author_version,
        file_verdict: finding.file_verdict,
        file_name: finding.file_name,
        file_version: finding.file_version,
        file_gone: finding.file_gone,
        site_down: None,
        alternatives: finding
            .alternatives
            .into_iter()
            .map(|other| UpdateAlternativeDto {
                source: other.source,
                url: other.url.to_string(),
                latest_version: other.latest_version,
            })
            .collect(),
    }
}

/// Сайты, которые сейчас не открываются: сетевой слой держит их на паузе после
/// сбоев или последняя проверка их не открыла. Подпись — самая свежая.
pub(crate) fn down_now(
    state: &AppState,
    last_report: &[SourceRunReport],
) -> HashMap<String, SiteDownDto> {
    let mut down: HashMap<String, SiteDownDto> = HashMap::new();
    for report in last_report {
        if report.status == bmm_sources::sources::SourceStatus::Unavailable {
            down.insert(
                report.source.clone(),
                SiteDownDto {
                    note: report
                        .note
                        .clone()
                        .unwrap_or_else(|| "сайт не открылся при проверке".to_string()),
                    reach: report.reach.map(|reach| reach.code().to_string()),
                },
            );
        }
    }
    if let Some(search) = &state.search {
        for source in search.sources.iter() {
            let id = source.id();
            if search.fetcher.status_of(id) == bmm_sources::sources::SourceStatus::Unavailable
                && let Some(note) = search.fetcher.message_of(id)
            {
                down.insert(
                    id.to_string(),
                    SiteDownDto {
                        note,
                        reach: search
                            .fetcher
                            .reach_of(id)
                            .map(|reach| reach.code().to_string()),
                    },
                );
            }
        }
    }
    down
}

/// Сведения о недоступности для строки обновления: запасные сайты, которые тоже не
/// открываются или выключены в настройках, не предлагаем.
pub(crate) fn mark_down(
    dto: &mut UpdateFindingDto,
    down: &HashMap<String, SiteDownDto>,
    disabled: &[String],
) {
    dto.site_down = down.get(&dto.source).cloned();
    dto.alternatives.retain(|other| {
        !down.contains_key(&other.source)
            && !disabled
                .iter()
                .any(|site| site.eq_ignore_ascii_case(&other.source))
    });
}

/// У дополнений картинка из архива — случайная текстура, поэтому она не нужна
/// ни в «Обновлениях», ни в окне скачивания.
pub(crate) fn preview_for_role(role: Option<&str>, preview: Option<String>) -> Option<String> {
    if role == Some("addon") { None } else { preview }
}

pub(crate) fn suggestion_dto(
    name_of: &dyn Fn(&str) -> String,
    binding: ModBinding,
) -> PageSuggestionDto {
    let name = name_of(&binding.mod_id);
    PageSuggestionDto {
        mod_id: binding.mod_id,
        mod_name: name,
        source: binding.source,
        source_id: binding.source_id,
        url: binding.url.to_string(),
    }
}

pub(crate) fn status_text(status: UpdateStatus) -> &'static str {
    match status {
        UpdateStatus::None => "none",
        UpdateStatus::Likely => "likely",
        UpdateStatus::Possible => "possible",
    }
}

pub(crate) fn progress_dto(progress: &UpdateProgress) -> UpdateProgressDto {
    UpdateProgressDto {
        source: progress.source.clone(),
        stage: progress.stage.clone(),
        done: progress.done,
        total: progress.total,
    }
}

/// Итог проверки и дополнительной проверки модов, которые программа только что
/// взяла под наблюдение сама: новые находки обеих, состояние сайтов — свежее.
pub(crate) fn merge_reports(first: UpdateRunReport, again: UpdateRunReport) -> UpdateRunReport {
    let mut new_findings = first.new_findings;
    for finding in again.new_findings {
        if !new_findings
            .iter()
            .any(|known| known.mod_id == finding.mod_id)
        {
            new_findings.push(finding);
        }
    }
    // Повторный ход проверяет только что взятые под наблюдение моды: его отчёт по сайту
    // дополняет первый, а не заменяет — иначе пропали бы сбои и «не нашлось» первого хода.
    let mut sources = first.sources;
    for source in again.sources {
        match sources
            .iter_mut()
            .find(|known| known.source == source.source)
        {
            Some(known) => {
                let keep = bmm_checks::coordinator::keeps_reason(known, &source);
                let answered = source.feed_pages_fetched + source.cards_fetched > 0;
                known.feed_pages_fetched += source.feed_pages_fetched;
                known.cards_fetched += source.cards_fetched;
                known.postponed += source.postponed;
                for id in source.missing_mods {
                    if !known.missing_mods.contains(&id) {
                        known.missing_mods.push(id);
                    }
                }
                for id in source.unreadable_mods {
                    if !known.unreadable_mods.contains(&id) {
                        known.unreadable_mods.push(id);
                    }
                }
                known.layout_changed |= source.layout_changed;
                bmm_checks::coordinator::merge_status(
                    known,
                    source.status,
                    source.note,
                    source.reach,
                    answered,
                    keep,
                );
            }
            None => sources.push(source),
        }
    }
    UpdateRunReport {
        findings: again.findings,
        sources,
        requests: first.requests + again.requests,
        up_to_date: first.up_to_date + again.up_to_date,
        new_findings,
        fresh_skipped: first.fresh_skipped,
        cancelled: first.cancelled || again.cancelled,
        stopped_by_time: first.stopped_by_time || again.stopped_by_time,
        reused: false,
    }
}

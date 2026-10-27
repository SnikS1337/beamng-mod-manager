//! Снимок проверки и его DTO.

use super::*;

pub(crate) fn build_snapshot(state: &AppState) -> UpdatesSnapshotDto {
    let store = state.library.as_ref();
    // Как на карточках модов: название и картинка из архива, если уже прочитаны.
    let titles = state.previews.titles();
    let keys: HashMap<String, String> = state
        .previews_by_id
        .lock()
        .unwrap()
        .iter()
        .map(|(id, target)| (id.clone(), target.key.clone()))
        .collect();
    let contents = state.previews.contents();
    let infos = state.previews.mod_infos();
    let role_of = |mod_id: &str| -> Option<String> {
        let key = keys.get(mod_id)?;
        let empty: &[String] = &[];
        let content = contents.get(key).map(Vec::as_slice).unwrap_or(empty);
        let hint = infos
            .get(key)
            .and_then(crate::commands::library::mod_info_hint);
        bmm_packs::duplicates::role_of_with_hint(content, hint).map(|role| role.code().to_string())
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
    let finding = |finding: UpdateFinding| -> UpdateFindingDto {
        let check = store.get_check(&finding.mod_id, &finding.source);
        let mut dto = finding_dto(&name_of, finding);
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
    let last_report_sources = state
        .update
        .as_ref()
        .and_then(|runtime| runtime.coordinator.last_report())
        .map(|report| report.sources)
        .unwrap_or_default();
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
    }
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
    }
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
                if source.status != bmm_sources::sources::SourceStatus::Ok {
                    known.status = source.status;
                }
                if source.note.is_some() {
                    known.note = source.note;
                }
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

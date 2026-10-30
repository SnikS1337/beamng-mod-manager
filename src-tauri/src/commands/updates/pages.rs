//! Поиск и привязка страниц модов.

use super::*;

#[tauri::command]
pub async fn updates_find_pages(
    app: AppHandle,
    state: tauri::State<'_, Arc<AppState>>,
    request_id: u64,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    let runtime = state
        .update
        .clone()
        .ok_or_else(|| AppError::new("pages", "Поиск страниц недоступен."))?;
    let Some(guard) = PageBusyGuard::acquire(runtime.page_busy.clone()) else {
        return Err(AppError::new("pages", "Поиск страниц уже идёт."));
    };
    runtime.page_cancel.store(false, Ordering::SeqCst);
    let cancel = runtime.page_cancel.clone();
    let state_for_task = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        let report = std::panic::catch_unwind(AssertUnwindSafe(|| {
            // Нажал человек: ищем все моды без страницы, даже искавшиеся недавно,
            // а модам со страницей — уверенные страницы на остальных сайтах.
            let mut report = run_page_search(&app, &state_for_task, request_id, &cancel, true);
            if !report.cancelled {
                let extra = run_extra_page_search(&app, &state_for_task, request_id, &cancel);
                report.found += extra.found;
                report.followed += extra.followed;
                report.searched += extra.searched;
                report.cancelled = extra.cancelled;
            }
            report
        }))
        .unwrap_or_default();
        if recheck_after_pages(&report) {
            let _batch = state_for_task.library.batch();
            let _ = runtime.coordinator.check(false, None);
        }
        let snapshot = build_snapshot(&state_for_task);
        let event = UpdatesEventDto {
            followed: report.followed,
            coverage: Some(snapshot.coverage.clone()),
            request_id,
            phase: "pages".to_string(),
            progress: None,
            findings: snapshot.findings,
            hidden: snapshot.hidden,
            suggestions: snapshot.suggestions,
            sources: Vec::new(),
            new_count: 0,
            up_to_date: 0,
            requests: report.searched as u32,
            cancelled: report.cancelled,
            stopped_by_time: false,
            error: None,
        };
        let _ = app.emit("updates://event", event);
    });
    Ok(())
}

#[tauri::command]
pub async fn updates_find_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    let state = state.inner().clone();
    if state
        .library
        .bindings()
        .iter()
        .any(|binding| binding.mod_id == mod_id)
    {
        return Err(AppError::new("pages", "У мода уже есть страница."));
    }
    let runtime = state
        .update
        .clone()
        .ok_or_else(|| AppError::new("pages", "Поиск страниц недоступен."))?;
    let Some(guard) = PageBusyGuard::acquire(runtime.page_busy.clone()) else {
        return Err(AppError::new("pages", "Поиск страниц уже идёт."));
    };
    runtime.page_cancel.store(false, Ordering::SeqCst);
    let cancel = runtime.page_cancel.clone();
    let state_for_task = state.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = guard;
        // Снимаем 14-дневную паузу только у выбранного мода.
        state_for_task.library.set_page_search(&mod_id, None);
        let report = std::panic::catch_unwind(AssertUnwindSafe(|| {
            run_single_page_search(&state_for_task, &mod_id, &cancel)
        }))
        .unwrap_or_default();
        if recheck_after_pages(&report) {
            let _batch = state_for_task.library.batch();
            let _ = runtime.coordinator.check(false, None);
        }
        build_snapshot(&state_for_task)
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))
}

#[tauri::command]
pub async fn updates_verify_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<PageVerificationDto, AppError> {
    let state = state.inner().clone();
    let search = page_search_service(&state)
        .ok_or_else(|| AppError::new("pages", "Поиск страниц недоступен."))?;
    tauri::async_runtime::spawn_blocking(move || {
        let verification = bmm_checks::pages::verify_page(
            state.library.as_ref(),
            search.as_ref(),
            &mod_id,
            &AtomicBool::new(false),
            &|id| page_title(&state, id),
        )
        .map_err(|message| AppError::new("pages", message))?;
        Ok(PageVerificationDto {
            page_source: verification.page_source,
            page_title: verification.page_title,
            page_url: verification.page_url.to_string(),
            page_size: verification.page_size,
            matches: verification
                .matches
                .into_iter()
                .map(|item| PageMatchDto {
                    source: item.source,
                    title: item.title,
                    url: item.url.to_string(),
                    version: item.version,
                    size: item.size,
                    author: item.author,
                    similarity: item.similarity,
                    same_size: item.same_size,
                    same_version: item.same_version,
                    same_author: item.same_author,
                })
                .collect(),
        })
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn updates_confirm_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    bmm_checks::confirm_page(state.library.as_ref(), &mod_id)
        .ok_or_else(|| AppError::new("pages", "Предложение страницы не найдено."))?;
    Ok(build_snapshot(&state))
}

#[tauri::command]
pub async fn updates_reject_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    bmm_checks::reject_page(state.library.as_ref(), &mod_id, Utc::now().fixed_offset())
        .ok_or_else(|| AppError::new("pages", "Предложение страницы не найдено."))?;
    Ok(build_snapshot(&state))
}

/// Снятые «Это другой мод» страницы, которые ещё можно вернуть кнопкой «Отменить».
static UNFOLLOWED: std::sync::Mutex<Vec<bmm_checks::Unfollowed>> =
    std::sync::Mutex::new(Vec::new());

fn unfollowed() -> std::sync::MutexGuard<'static, Vec<bmm_checks::Unfollowed>> {
    UNFOLLOWED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// «Это другой мод» для страницы, за которой уже следим.
pub(crate) fn unfollow(state: &AppState, mod_id: &str, source: &str) -> Result<(), AppError> {
    let saved = bmm_checks::unfollow_page(
        state.library.as_ref(),
        mod_id,
        source,
        Utc::now().fixed_offset(),
    )
    .ok_or_else(|| AppError::new("pages", "Программа не следит за этой страницей."))?;
    let mut list = unfollowed();
    list.retain(|item| !(item.binding.mod_id == mod_id && item.binding.source == source));
    list.push(saved);
    // Отменить можно только недавнее: хранить всё подряд незачем.
    let excess = list.len().saturating_sub(16);
    list.drain(..excess);
    Ok(())
}

/// «Отменить» после «Это другой мод».
pub(crate) fn refollow(state: &AppState, mod_id: &str, source: &str) -> Result<(), AppError> {
    let saved = {
        let mut list = unfollowed();
        let index = list
            .iter()
            .position(|item| item.binding.mod_id == mod_id && item.binding.source == source)
            .ok_or_else(|| AppError::new("pages", "Вернуть уже нечего."))?;
        list.remove(index)
    };
    if bmm_checks::refollow_page(state.library.as_ref(), &saved) {
        return Ok(());
    }
    let mut list = unfollowed();
    if !list
        .iter()
        .any(|item| item.binding.mod_id == mod_id && item.binding.source == saved.binding.source)
    {
        list.push(saved);
    }
    Err(AppError::new(
        "pages",
        "Страницу не вернуть: мод удалён или за ним уже следят по другой странице.",
    ))
}

#[tauri::command]
pub async fn updates_unfollow(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
    source: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    unfollow(&state, &mod_id, &source)?;
    Ok(build_snapshot(&state))
}

#[tauri::command]
pub async fn updates_refollow(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
    source: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    refollow(&state, &mod_id, &source)?;
    Ok(build_snapshot(&state))
}

/// «Страница пропала»: перестать следить за ней. `search_again` — разрешить поиск
/// новой страницы сразу (иначе отказ от страницы откладывает поиск, как «Это другой мод»).
pub(crate) fn forget_lost(
    state: &AppState,
    mod_id: &str,
    source: &str,
    search_again: bool,
) -> Result<(), AppError> {
    unfollow(state, mod_id, source)?;
    if search_again {
        state.library.set_page_search(mod_id, None);
    }
    Ok(())
}

#[tauri::command]
pub async fn updates_forget_lost(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
    source: String,
    search_again: bool,
) -> Result<UpdatesSnapshotDto, AppError> {
    forget_lost(&state, &mod_id, &source, search_again)?;
    Ok(build_snapshot(&state))
}

/// «Указать страницу» из панели мода: ссылка, которую вставил человек.
#[tauri::command]
pub async fn mod_bind_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
    url: String,
) -> Result<(), AppError> {
    let state = state.inner().clone();
    let runtime = state
        .update
        .clone()
        .ok_or_else(|| AppError::new("pages", "Проверка обновлений недоступна."))?;
    tauri::async_runtime::spawn_blocking(move || {
        bmm_checks::bind_url(
            state.library.as_ref(),
            runtime.sources.as_ref(),
            &mod_id,
            &url,
            &AtomicBool::new(false),
            Utc::now().fixed_offset(),
        )
        .map(|_| ())
        .map_err(|error| AppError::new(error.code.as_str(), error.message))
    })
    .await
    .map_err(|error| AppError::new("task", error.to_string()))?
}

#[tauri::command]
pub async fn updates_undo_page(
    state: tauri::State<'_, Arc<AppState>>,
    mod_id: String,
    source: String,
    source_id: String,
    url: String,
) -> Result<UpdatesSnapshotDto, AppError> {
    let url = url
        .parse()
        .map_err(|_| AppError::new("url", "Некорректный адрес страницы."))?;
    let binding = ModBinding {
        mod_id: mod_id.clone(),
        source,
        source_id,
        url,
        confirmed: false,
    };
    if !bmm_checks::undo_page(state.library.as_ref(), &binding, Utc::now().fixed_offset()) {
        return Err(AppError::new(
            "pages",
            "Предложение страницы не вернуть: мод удалён или за ним уже следят по другой странице.",
        ));
    }
    Ok(build_snapshot(&state))
}

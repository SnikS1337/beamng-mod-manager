//! Выполнение плана проверки: ленты сайтов, затем карточки.
//!
//! API синхронное: вызывающий (Tauri-команда) оборачивает его в `spawn_blocking`.
//! Отмена — атомарный флаг, результат того, что успели, сохраняется и возвращается.
//! Сайты обрабатываются по очереди: у каждого свои паузы, а общий бюджет времени
//! всё равно ограничивает проверку.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Instant;

use chrono::{DateTime, FixedOffset, TimeDelta};
use url::Url;

use bmm_domain::library::LibraryStore;
use bmm_domain::versioning::{VersionComparer, VersionOrder};
use bmm_sources::sources::{ModSource, SourceError, SourceStatus};

use crate::options::UpdateCheckOptions;
use crate::planner::{FeedCoverage, FeedEntry, TrackedBinding, UpdateCheckPlanner};
use crate::service::{UpdateCheckService, UpdateFinding, mod_label, status_to_store};

/// Ход проверки для интерфейса.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateProgress {
    /// Пустая строка в финальном сообщении «готово».
    pub source: String,
    /// «лента», «карточки», «готово».
    pub stage: String,
    pub done: usize,
    pub total: usize,
}

/// Что произошло с одним сайтом за проверку.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRunReport {
    pub source: String,
    pub status: SourceStatus,
    pub feed_pages_fetched: u32,
    pub cards_fetched: u32,
    pub postponed: usize,
    /// Моды, чьи страницы не открылись (404): прошлое состояние не тронуто.
    pub missing_mods: Vec<String>,
    /// Моды, чьи страницы открылись, но не прочитались (ни версии, ни размера, хотя раньше были).
    pub unreadable_mods: Vec<String>,
    /// Похоже, сайт поменял вёрстку: лента пуста, карточки не находятся или не читаются.
    pub layout_changed: bool,
    pub note: Option<String>,
}

/// Итог одной проверки.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateRunReport {
    pub findings: Vec<UpdateFinding>,
    pub sources: Vec<SourceRunReport>,
    /// Сколько запросов к сайтам сделано за проверку.
    pub requests: u32,
    /// Сколько модов подтверждено лентой без открытия карточки.
    pub up_to_date: usize,
    /// Обновления, которых до этой проверки в списке не было.
    pub new_findings: Vec<UpdateFinding>,
    /// Сколько модов не проверялось, потому что их проверили недавно (срок ещё не вышел).
    pub fresh_skipped: usize,
    pub cancelled: bool,
    pub stopped_by_time: bool,
    /// Отчёт предыдущей проверки: новая не запускалась (слишком частое «принудительно»).
    pub reused: bool,
}

impl UpdateRunReport {
    /// Всё ли удалось проверить: ни один сайт не отказал, ничего не отложено и не прервано.
    pub fn complete(&self) -> bool {
        !self.cancelled
            && !self.stopped_by_time
            && self.sources.iter().all(|report| {
                report.status == SourceStatus::Ok && report.postponed == 0 && report.note.is_none()
            })
    }

    fn empty() -> Self {
        Self {
            findings: Vec::new(),
            sources: Vec::new(),
            requests: 0,
            up_to_date: 0,
            new_findings: Vec::new(),
            fresh_skipped: 0,
            cancelled: false,
            stopped_by_time: false,
            reused: false,
        }
    }
}

/// Всё, что нужно одной проверке. Позволяет не тащить восемь аргументов в функцию.
pub struct UpdateRun<'a> {
    pub store: &'a dyn LibraryStore,
    pub sources: &'a [std::sync::Arc<dyn ModSource>],
    pub now: DateTime<FixedOffset>,
    pub options: UpdateCheckOptions,
    /// Флаг отмены. Проверяется между запросами и адаптерами источников.
    pub cancel: &'a AtomicBool,
    /// Часы для лимита времени (в тестах подменяются); `None` — только секундомер.
    pub clock: Option<&'a (dyn Fn() -> DateTime<FixedOffset> + Sync)>,
    /// Сайты на паузе после недавнего сбоя: сообщение для отчёта.
    pub hold_off: BTreeMap<String, String>,
}

impl UpdateRun<'_> {
    /// Удобный конструктор для типового запуска без подмены часов и пауз.
    pub fn new<'a>(
        store: &'a dyn LibraryStore,
        sources: &'a [std::sync::Arc<dyn ModSource>],
        now: DateTime<FixedOffset>,
        options: UpdateCheckOptions,
        cancel: &'a AtomicBool,
    ) -> UpdateRun<'a> {
        UpdateRun {
            store,
            sources,
            now,
            options,
            cancel,
            clock: None,
            hold_off: BTreeMap::new(),
        }
    }
}

/// Кнопка «проверить обновления».
///
/// Один сбой сайта (отказ, «вы человек?», исключение адаптера) касается только
/// этого сайта. Отмена и лимит времени не теряют результат: возвращается то,
/// что успели. Одновременные запуски и частые нажатия — на [`crate::UpdateCoordinator`].
pub struct UpdateRunner;

impl UpdateRunner {
    /// Выполняет план проверки. Сети вне `ModSource` не касается.
    pub fn run(
        request: &UpdateRun<'_>,
        progress: Option<&mut dyn FnMut(&UpdateProgress)>,
    ) -> UpdateRunReport {
        run(request, progress)
    }
}

/// Свободная функция на случай, когда удобнее вызвать без имени типа.
pub fn run(
    request: &UpdateRun<'_>,
    progress: Option<&mut dyn FnMut(&UpdateProgress)>,
) -> UpdateRunReport {
    let mut runner = Runner {
        request,
        budget: Budget::new(request),
        progress,
        file_pages: std::sync::atomic::AtomicUsize::new(FILE_PAGES_PER_RUN),
    };
    runner.run()
}

/// Глубже этого датированную ленту не читаем даже ради давней проверки.
const DEEP_FEED_PAGES: u32 = 40;

/// У всех прочитанных записей ленты есть дата: по ней можно доказать «не менялся».
fn is_dated(ordered: &[FeedEntry]) -> bool {
    !ordered.is_empty() && ordered.iter().all(|entry| entry.updated.is_some())
}

struct Budget<'a> {
    started: DateTime<FixedOffset>,
    clock: Option<&'a (dyn Fn() -> DateTime<FixedOffset> + Sync)>,
    max: TimeDelta,
    stopwatch: Instant,
}

impl<'a> Budget<'a> {
    fn new(request: &UpdateRun<'a>) -> Self {
        let started = request.clock.map_or(request.now, |clock| clock());
        let max = TimeDelta::from_std(request.options.max_duration).unwrap_or(TimeDelta::MAX);
        Self {
            started,
            clock: request.clock,
            max,
            stopwatch: Instant::now(),
        }
    }

    fn is_up(&self) -> bool {
        // Лимит меряем и по часам (их подменяют в тестах), и по секундомеру:
        // перевод часов не должен растянуть проверку.
        self.stopwatch.elapsed() > self.request_duration()
            || self
                .clock
                .is_some_and(|clock| clock() - self.started > self.max)
    }

    fn request_duration(&self) -> std::time::Duration {
        self.max.to_std().unwrap_or(std::time::Duration::MAX)
    }
}

struct Runner<'r, 'a, 'p> {
    request: &'r UpdateRun<'a>,
    budget: Budget<'a>,
    progress: Option<&'p mut dyn FnMut(&UpdateProgress)>,
    /// Сколько ещё страниц файлообменника можно открыть за эту проверку: их читает
    /// встроенный браузер, это медленно. Остальные — в следующий раз.
    file_pages: std::sync::atomic::AtomicUsize,
}

/// Страниц файлообменника на одну проверку (на все сайты вместе).
pub const FILE_PAGES_PER_RUN: usize = 12;

/// Как долго сведения о файле за ссылкой считаются свежими.
pub const FILE_FRESH_FOR: chrono::TimeDelta = chrono::TimeDelta::days(7);

/// Идти ли сейчас по ссылке «Скачать». Сайт может считать такой заход скачиванием, и
/// полная проверка по кнопке накручивала бы авторам счётчики. Поэтому идём, только
/// если о файле ничего не знаем, сменилась подпись страницы или сама ссылка (STMods
/// даёт новую на каждый новый файл), или прошла неделя.
pub fn file_needs_look(
    known: Option<&bmm_domain::library::FileSnapshot>,
    check: &bmm_domain::library::CheckState,
    offer: Option<&str>,
    now: chrono::DateTime<chrono::FixedOffset>,
) -> bool {
    let Some(known) = known else {
        return true;
    };
    let key = bmm_domain::file_check::page_key(check.last_version.as_deref(), check.last_size);
    (known.name.is_none() && !known.gone)
        || known.page_key.as_deref() != Some(key.as_str())
        || known.offer.as_deref() != offer
        || known
            .seen_at
            .is_none_or(|seen| now - seen >= FILE_FRESH_FOR)
}

/// Взять одно место из остатка, если оно есть. Сайты идут параллельно — без гонок.
fn take_one(left: &std::sync::atomic::AtomicUsize) -> bool {
    use std::sync::atomic::Ordering;
    let mut current = left.load(Ordering::SeqCst);
    while current > 0 {
        match left.compare_exchange(current, current - 1, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => return true,
            Err(actual) => current = actual,
        }
    }
    false
}

#[derive(Default)]
struct SourceOutcome {
    requests: u32,
    up_to_date: usize,
    feed_pages: u32,
    cards_done: Option<u32>,
    cancelled: bool,
    stopped_by_time: bool,
    layout_changed: bool,
    note: Option<String>,
    failure: Option<(SourceStatus, String)>,
    missing: Vec<String>,
    unreadable: Vec<String>,
    postponed: Vec<TrackedBinding>,
}

impl Runner<'_, '_, '_> {
    fn run(&mut self) -> UpdateRunReport {
        let request = self.request;
        let store = request.store;
        let now = request.now;
        let options = &request.options;

        let before: HashSet<(String, Option<String>)> = UpdateCheckService::pending(store, false)
            .into_iter()
            .map(|finding| (finding.mod_id, finding.latest_version))
            .collect();

        let mut by_id: HashMap<&str, &std::sync::Arc<dyn ModSource>> = HashMap::new();
        for source in request.sources {
            by_id.entry(source.id()).or_insert(source);
        }

        // Сайт, выключенный в настройках, не приходит фабрикой адаптеров:
        // его привязки молча пропускаем, а не жалуемся «нет адаптера».
        let tracked: Vec<TrackedBinding> = UpdateCheckService::tracked(store)
            .into_iter()
            .filter(|binding| by_id.contains_key(binding.source.as_str()))
            .collect();
        let statuses = UpdateCheckService::source_statuses(store);
        let plan = UpdateCheckPlanner::plan_feeds(&tracked, &statuses, now, options);
        let due_set: HashSet<(String, String)> = plan
            .due
            .iter()
            .map(|binding| (binding.mod_id.clone(), binding.source.clone()))
            .collect();
        let fresh_skipped = tracked
            .iter()
            .filter(|binding| {
                statuses
                    .get(&binding.source)
                    .is_none_or(|status| *status == SourceStatus::Ok)
                    && !due_set.contains(&(binding.mod_id.clone(), binding.source.clone()))
            })
            .count();

        let mut reports: BTreeMap<String, SourceRunReport> = BTreeMap::new();
        let mut failed: BTreeMap<String, (SourceStatus, String)> = BTreeMap::new();
        let mut notes: BTreeMap<String, String> = BTreeMap::new();

        for skipped in &plan.skipped_sources {
            let note = if skipped.status == SourceStatus::NeedsHumanCheck {
                "сайт просит пройти проверку «вы человек?»"
            } else {
                "сайт недоступен"
            };
            reports.insert(
                skipped.source.clone(),
                SourceRunReport {
                    source: skipped.source.clone(),
                    status: skipped.status,
                    feed_pages_fetched: 0,
                    cards_fetched: 0,
                    postponed: 0,
                    missing_mods: Vec::new(),
                    unreadable_mods: Vec::new(),
                    layout_changed: false,
                    note: Some(note.to_string()),
                },
            );
        }
        for (source, why) in &request.hold_off {
            if plan.due.iter().any(|binding| &binding.source == source)
                && !reports.contains_key(source)
            {
                reports.insert(
                    source.clone(),
                    SourceRunReport {
                        source: source.clone(),
                        status: SourceStatus::Unavailable,
                        feed_pages_fetched: 0,
                        cards_fetched: 0,
                        postponed: 0,
                        missing_mods: Vec::new(),
                        unreadable_mods: Vec::new(),
                        layout_changed: false,
                        note: Some(why.clone()),
                    },
                );
            }
        }

        let mut feed_pages_done: BTreeMap<String, u32> = BTreeMap::new();
        let mut cards_done: BTreeMap<String, u32> = BTreeMap::new();
        let mut missing: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut unreadable: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut layout: BTreeSet<String> = BTreeSet::new();
        let mut postponed_all: Vec<TrackedBinding> = Vec::new();
        let mut requests = 0u32;
        let mut up_to_date = 0usize;
        let mut cancelled = false;
        let mut stopped_by_time = false;

        // Находки без сведений о файле: их страницы открываем вне очереди.
        let stale_files = UpdateCheckService::stale_file_findings(store);
        let refresh: Vec<TrackedBinding> = tracked
            .iter()
            .filter(|binding| {
                stale_files.contains(&(binding.mod_id.clone(), binding.source.clone()))
                    && by_id
                        .get(binding.source.as_str())
                        .is_some_and(|adapter| adapter.probes_files())
                    && statuses
                        .get(&binding.source)
                        .is_none_or(|status| *status == SourceStatus::Ok)
            })
            .cloned()
            .collect();

        let mut jobs: Vec<(&String, u32, &std::sync::Arc<dyn ModSource>)> = Vec::new();
        let refresh_sources: BTreeSet<&String> =
            refresh.iter().map(|binding| &binding.source).collect();
        let mut feed_pages: Vec<(&String, u32)> = plan
            .feed_pages
            .iter()
            .map(|(source, pages)| (source, *pages))
            .collect();
        for source in refresh_sources {
            if !plan.feed_pages.contains_key(source) {
                // Сайт по очереди не нужен — только эти страницы, без ленты.
                feed_pages.push((source, 0));
            }
        }
        for (source, pages) in feed_pages {
            let pages = &pages;
            if reports.contains_key(source) {
                continue;
            }
            let Some(adapter) = by_id.get(source.as_str()).copied() else {
                let note = "нет адаптера для этого сайта".to_string();
                failed.insert(source.clone(), (SourceStatus::Unavailable, note.clone()));
                notes.insert(source.clone(), note);
                continue;
            };
            jobs.push((source, *pages, adapter));
        }

        let due = &plan.due;
        let refresh = &refresh;
        let budget = &self.budget;
        let file_pages = &self.file_pages;
        let progress = &mut self.progress;
        let outcomes: Vec<(&String, SourceOutcome)> = std::thread::scope(|scope| {
            let (sender, receiver) = mpsc::channel::<UpdateProgress>();
            let handles: Vec<_> = jobs
                .iter()
                .map(|&(source, pages, adapter)| {
                    let worker = Worker {
                        request,
                        budget,
                        progress: sender.clone(),
                        file_pages,
                        refresh,
                    };
                    let handle = std::thread::Builder::new()
                        .name(format!("bmm-check-{source}"))
                        .spawn_scoped(scope, move || {
                            worker.run_source(source, pages, due, adapter)
                        });
                    (source, handle)
                })
                .collect();
            drop(sender);
            // Ход проверки отдаём из этого потока: обратный вызов интерфейса не обязан быть потокобезопасным.
            for event in receiver {
                if let Some(callback) = progress.as_mut() {
                    (**callback)(&event);
                }
            }
            handles
                .into_iter()
                .map(|(source, handle)| {
                    let outcome = match handle {
                        Ok(handle) => handle.join().unwrap_or_else(|_| SourceOutcome {
                            failure: Some((
                                SourceStatus::Unavailable,
                                "внутренняя ошибка при проверке сайта".to_string(),
                            )),
                            ..SourceOutcome::default()
                        }),
                        Err(_) => SourceOutcome {
                            failure: Some((
                                SourceStatus::Unavailable,
                                "не удалось запустить проверку сайта".to_string(),
                            )),
                            ..SourceOutcome::default()
                        },
                    };
                    (source, outcome)
                })
                .collect()
        });

        for (source, outcome) in outcomes {
            requests += outcome.requests;
            up_to_date += outcome.up_to_date;
            cancelled |= outcome.cancelled;
            stopped_by_time |= outcome.stopped_by_time;
            if outcome.feed_pages > 0 {
                feed_pages_done.insert(source.clone(), outcome.feed_pages);
            }
            if let Some(cards) = outcome.cards_done {
                cards_done.insert(source.clone(), cards);
            }
            if !outcome.missing.is_empty() {
                missing.insert(source.clone(), outcome.missing);
            }
            if !outcome.unreadable.is_empty() {
                unreadable.insert(source.clone(), outcome.unreadable);
            }
            if outcome.layout_changed {
                layout.insert(source.clone());
            }
            if let Some(note) = outcome.note {
                notes.insert(source.clone(), note);
            }
            if let Some(failure) = outcome.failure {
                failed.insert(source.clone(), failure);
            }
            postponed_all.extend(outcome.postponed);
        }

        let mut postponed_by: BTreeMap<String, usize> = BTreeMap::new();
        for binding in &postponed_all {
            *postponed_by.entry(binding.source.clone()).or_default() += 1;
        }

        let due_sources: BTreeSet<String> = plan
            .due
            .iter()
            .map(|binding| binding.source.clone())
            .collect();
        for source in due_sources {
            if reports.contains_key(&source) {
                continue;
            }
            let failure = failed.get(&source);
            let mut note = failure
                .map(|(_, note)| note.clone())
                .or_else(|| notes.get(&source).cloned());
            if note.is_none()
                && (cancelled || stopped_by_time)
                && !feed_pages_done.contains_key(&source)
            {
                note = Some(if cancelled {
                    "проверка прервана".to_string()
                } else {
                    "не хватило времени, продолжится при следующей проверке".to_string()
                });
            }
            reports.insert(
                source.clone(),
                SourceRunReport {
                    source: source.clone(),
                    status: failure.map_or(SourceStatus::Ok, |(status, _)| *status),
                    feed_pages_fetched: feed_pages_done.get(&source).copied().unwrap_or(0),
                    cards_fetched: cards_done.get(&source).copied().unwrap_or(0),
                    postponed: postponed_by.get(&source).copied().unwrap_or(0),
                    missing_mods: missing.get(&source).cloned().unwrap_or_default(),
                    unreadable_mods: unreadable.get(&source).cloned().unwrap_or_default(),
                    layout_changed: layout.contains(&source),
                    note,
                },
            );
        }

        self.emit(&UpdateProgress {
            source: String::new(),
            stage: "готово".to_string(),
            done: 1,
            total: 1,
        });

        let findings = UpdateCheckService::pending(store, false);
        let new_findings = findings
            .iter()
            .filter(|finding| {
                !before.contains(&(finding.mod_id.clone(), finding.latest_version.clone()))
            })
            .cloned()
            .collect();

        UpdateRunReport {
            findings,
            sources: reports.into_values().collect(),
            requests,
            up_to_date,
            new_findings,
            fresh_skipped,
            cancelled,
            stopped_by_time,
            reused: false,
        }
    }

    fn emit(&mut self, progress: &UpdateProgress) {
        if let Some(callback) = self.progress.as_mut() {
            (**callback)(progress);
        }
    }
}

/// Работа с одним сайтом. Сайты независимы (у каждого своя очередь запросов и паузы),
/// поэтому идут в своих потоках: проверка длится столько, сколько самый медленный сайт,
/// а не их сумму: сайты проверяются параллельно.
struct Worker<'r, 'a> {
    request: &'r UpdateRun<'a>,
    budget: &'r Budget<'a>,
    progress: mpsc::Sender<UpdateProgress>,
    /// Общий на все сайты остаток страниц файлообменника ([`FILE_PAGES_PER_RUN`]).
    file_pages: &'r std::sync::atomic::AtomicUsize,
    /// Находки без сведений о файле: их страницы открываются вне очереди.
    refresh: &'r [TrackedBinding],
}

impl Worker<'_, '_> {
    fn run_source(
        &self,
        source: &str,
        pages: u32,
        due: &[TrackedBinding],
        adapter: &std::sync::Arc<dyn ModSource>,
    ) -> SourceOutcome {
        let mut outcome = SourceOutcome::default();
        let due_here: Vec<&TrackedBinding> = due
            .iter()
            .filter(|binding| binding.source == source)
            .collect();

        // 1. Лента. Если у записей есть даты, читаем её ровно настолько глубоко,
        // чтобы дойти до последней проверки самого «давнего» мода.
        let mut entries: HashMap<String, FeedEntry> = HashMap::new();
        let mut ordered: Vec<FeedEntry> = Vec::new();
        let oldest_checked = due_here
            .iter()
            .filter(|binding| binding.last_version.is_some())
            .filter_map(|binding| binding.last_checked_utc)
            .min();

        let limit = pages;
        // Датированная лента (репозиторий BeamNG) покрывает все изменения со времени
        // прошлой проверки сама: читаем её глубже плана, пока не дойдём до самой давней
        // проверки. Но не дальше, чем стоило бы открыть страницы самих модов: одна
        // страница ленты не должна стоить дороже, чем она экономит.
        let deep_limit = if self.request.options.force
            || !self.request.options.deep_feed
            || oldest_checked.is_none()
        {
            limit
        } else {
            DEEP_FEED_PAGES.min(due_here.len() as u32).max(limit)
        };
        let mut page = 1u32;
        while page <= limit || (page <= deep_limit && is_dated(&ordered)) {
            let total = if page > limit { deep_limit } else { limit };
            if self.is_cancelled() {
                outcome.cancelled = true;
                return outcome;
            }
            if self.budget.is_up() {
                outcome.stopped_by_time = true;
                break;
            }
            self.emit(&UpdateProgress {
                source: source.to_string(),
                stage: "лента".to_string(),
                done: page.saturating_sub(1) as usize,
                total: total as usize,
            });
            match adapter.feed_page(page, self.request.cancel) {
                Ok(items) => {
                    outcome.requests += 1;
                    outcome.feed_pages = page;
                    if items.is_empty() {
                        if page == 1 {
                            outcome.note =
                                Some("лента пуста: возможно, изменилась вёрстка сайта".to_string());
                            outcome.layout_changed = true;
                        }
                        break;
                    }
                    for listing in items {
                        let entry = FeedEntry {
                            source_id: listing.source_id.clone(),
                            version: listing.mod_version.clone(),
                            size_bytes: listing.size_bytes,
                            updated: listing.updated_at,
                        };
                        if let std::collections::hash_map::Entry::Vacant(slot) =
                            entries.entry(listing.source_id.clone())
                        {
                            slot.insert(entry.clone());
                            ordered.push(entry);
                        }
                    }
                }
                Err(error) => {
                    outcome.requests += 1;
                    if self.is_cancelled() {
                        outcome.cancelled = true;
                        return outcome;
                    }
                    outcome.failure = Some(self.failure_of(source, &error));
                    return outcome; // у сайта, отказавшего на ленте, карточки не открываем
                }
            }

            if !self.request.options.force
                && let Some(covered) = FeedCoverage::compute(&ordered)
                && let Some(checked) = oldest_checked
                && covered <= checked - FeedCoverage::MARGIN
            {
                break; // дошли до самой давней проверки
            }
            page += 1;
        }

        // 2. Знакомство: версия есть в ленте, прошлых данных нет — карточка не нужна,
        // если версия совпала с установленной. Иначе это возможное обновление, и для
        // решения нужен размер файла — его даёт только карточка (лента STMods без
        // размера: rls 2.8.0-alpha2 против «2.8.0» был бы обновлением вслепую).
        let mut card_candidates: Vec<TrackedBinding> = Vec::new();
        for binding in &due_here {
            if binding.last_version.is_none()
                && let Some(entry) = entries.get(&binding.source_id)
                && entry.version.is_some()
                && (entry.size_bytes.is_some()
                    || VersionComparer::compare(
                        self.request
                            .store
                            .get_mod(&binding.mod_id)
                            .and_then(|installed| installed.version)
                            .as_deref(),
                        entry.version.as_deref(),
                    ) == VersionOrder::Same)
            {
                UpdateCheckService::apply_feed_baseline(
                    self.request.store,
                    binding,
                    entry,
                    self.request.now,
                );
                outcome.up_to_date += 1;
                continue;
            }
            card_candidates.push((*binding).clone());
        }

        // 3. Карточки.
        let mut feeds: HashMap<String, HashMap<String, FeedEntry>> = HashMap::new();
        feeds.insert(source.to_string(), entries);
        let coverage = if self.request.options.force {
            None
        } else {
            FeedCoverage::compute(&ordered)
                .map(|coverage| HashMap::from([(source.to_string(), coverage)]))
        };
        let mod_pages: HashMap<String, Url> = {
            let mut map = HashMap::new();
            for binding in self.request.store.bindings() {
                if binding.source == source {
                    map.entry(binding.mod_id).or_insert(binding.url);
                }
            }
            map
        };
        let in_feed = |binding: &TrackedBinding| -> bool {
            match mod_pages.get(&binding.mod_id) {
                Some(url) => adapter.feed_covers(url),
                None => true,
            }
        };
        let card_plan = UpdateCheckPlanner::plan_cards(
            &card_candidates,
            &feeds,
            self.request.now,
            &self.request.options,
            coverage.as_ref(),
            Some(&in_feed),
        );
        outcome.postponed.extend(card_plan.postponed);
        for binding in &card_plan.confirmed_unchanged {
            UpdateCheckService::apply_feed_unchanged(self.request.store, binding, self.request.now);
            outcome.up_to_date += 1;
        }

        let mut list: Vec<TrackedBinding> =
            card_plan.to_fetch.get(source).cloned().unwrap_or_default();
        for binding in self
            .refresh
            .iter()
            .filter(|binding| binding.source == source)
        {
            if !list.iter().any(|planned| planned.mod_id == binding.mod_id) {
                list.push(binding.clone());
            }
        }
        if list.is_empty() {
            return outcome;
        }
        let list = &list;

        let mut done = 0usize;
        let mut attempted = 0usize;
        let mut misses: Vec<TrackedBinding> = Vec::new();
        outcome.cards_done = Some(0);
        for binding in list {
            if self.is_cancelled() {
                outcome.cancelled = true;
                break;
            }
            if self.budget.is_up() {
                outcome.stopped_by_time = true;
                break;
            }
            // Мод или привязку могли удалить, пока шла проверка.
            let Some(current_binding) = self
                .request
                .store
                .bindings()
                .into_iter()
                .find(|item| item.mod_id == binding.mod_id && item.source == binding.source)
            else {
                continue;
            };
            self.emit(&UpdateProgress {
                source: source.to_string(),
                stage: "карточки".to_string(),
                done,
                total: list.len(),
            });
            match adapter.listing(&current_binding.url, self.request.cancel) {
                Ok(current) => {
                    outcome.requests += 1;
                    done += 1;
                    attempted += 1;
                    outcome.cards_done = Some(done as u32);
                    match current {
                        None => {
                            // Страницы нет (убрана автором или сменила адрес): прошлое состояние
                            // не трогаем, сообщаем.
                            outcome
                                .missing
                                .push(mod_label(self.request.store, &binding.mod_id));
                            misses.push(binding.clone());
                        }
                        Some(current) => {
                            // Страница есть, но разбор не нашёл на ней ничего, хотя раньше находил:
                            // это не «без изменений», а «не поняли». Прошлое и срок не трогаем.
                            let previous = self
                                .request
                                .store
                                .get_check(&binding.mod_id, &binding.source);
                            if current.mod_version.is_none()
                                && current.size_bytes.is_none()
                                && previous.as_ref().is_some_and(|state| {
                                    state.last_version.is_some() || state.last_size.is_some()
                                })
                            {
                                outcome
                                    .unreadable
                                    .push(mod_label(self.request.store, &binding.mod_id));
                                continue;
                            }
                            UpdateCheckService::apply_card(
                                self.request.store,
                                binding,
                                &current,
                                self.request.now,
                            );
                            move_to_newest_page(
                                self.request.store,
                                adapter.as_ref(),
                                &current_binding,
                                &current,
                            );
                            self.note_file(adapter, binding, &current);
                        }
                    }
                }
                Err(error) => {
                    outcome.requests += 1;
                    if self.is_cancelled() {
                        outcome.cancelled = true;
                        break;
                    }
                    outcome.failure = Some(self.failure_of(source, &error));
                    break;
                }
            }
        }

        // Не нашлись сразу много карточек подряд: скорее сломались адреса или вёрстка,
        // чем всё удалили.
        if attempted >= 3 && outcome.missing.len() == attempted {
            outcome.note = Some(
                "карточки не найдены ни у одного мода: возможно, изменились адреса или вёрстка сайта"
                    .to_string(),
            );
            outcome.layout_changed = true;
        } else {
            for binding in &misses {
                UpdateCheckService::apply_missing(self.request.store, binding, self.request.now);
            }
        }
        if !outcome.unreadable.is_empty() && outcome.note.is_none() {
            outcome.note = Some(
                if outcome.unreadable.len() == attempted - outcome.missing.len() {
                    "страницы модов открываются, но не читаются: возможно, изменилась вёрстка сайта"
                        .to_string()
                } else {
                    format!(
                        "не прочитали {} из {} страниц: возможно, изменилась вёрстка сайта",
                        outcome.unreadable.len(),
                        attempted
                    )
                },
            );
            outcome.layout_changed = true;
        }

        outcome
    }

    /// Что лежит за кнопкой «Скачать»: имя и размер файла — к той подписи страницы,
    /// что только что записали. Сбой здесь проверку мода не портит.
    fn note_file(
        &self,
        adapter: &std::sync::Arc<dyn ModSource>,
        binding: &TrackedBinding,
        current: &bmm_domain::models::Listing,
    ) {
        use std::sync::atomic::Ordering;
        if self.is_cancelled() || self.budget.is_up() {
            return;
        }
        let store = self.request.store;
        let Some(check) = store.get_check(&binding.mod_id, &binding.source) else {
            return;
        };
        let offer = bmm_sources::files::offer_url(current).map(|url| url.to_string());
        if !file_needs_look(
            check.last_file.as_ref(),
            &check,
            offer.as_deref(),
            self.request.now,
        ) {
            return;
        }
        // Место под страницу файлообменника занимаем заранее: сайты идут параллельно.
        let open_page = take_one(self.file_pages);
        let probe = adapter.probe_file(
            current,
            check.last_file.as_ref(),
            open_page,
            self.request.cancel,
        );
        if open_page && !probe.opened_page {
            self.file_pages.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(mut file) = probe.file {
            file.offer = offer;
            file.seen_at = Some(self.request.now);
            UpdateCheckService::apply_file(store, &binding.mod_id, &binding.source, file);
        }
    }

    fn failure_of(&self, source: &str, error: &SourceError) -> (SourceStatus, String) {
        // Проверка «вы человек?» запоминается, пока пользователь её не пройдёт;
        // недоступность — нет, она может пройти сама.
        if error.status == SourceStatus::NeedsHumanCheck {
            self.request
                .store
                .set_source_status(source, status_to_store(SourceStatus::NeedsHumanCheck));
        }
        (error.status, error.message.clone())
    }

    fn is_cancelled(&self) -> bool {
        self.request.cancel.load(Ordering::Relaxed)
    }

    fn emit(&self, progress: &UpdateProgress) {
        let _ = self.progress.send(progress.clone());
    }
}

/// Пустой отчёт — удобная заготовка для ошибок координатора.
impl Default for UpdateRunReport {
    fn default() -> Self {
        Self::empty()
    }
}

/// Сайт, где каждая версия — своя страница (Telegram-канал): привязка переезжает на
/// страницу новой версии. Иначе «Обновить» открывал бы пост прежней версии, и
/// человек скачал бы то, что у него уже стоит.
fn move_to_newest_page(
    store: &dyn bmm_domain::library::LibraryStore,
    adapter: &dyn ModSource,
    binding: &bmm_domain::library::ModBinding,
    current: &bmm_domain::models::Listing,
) {
    if !adapter.page_per_version()
        || current.url == binding.url
        || !adapter.owns(&current.url)
        || !adapter.is_mod_page(&current.url)
    {
        return;
    }
    let mut moved = binding.clone();
    moved.url = current.url.clone();
    if !current.source_id.is_empty() {
        moved.source_id = current.source_id.clone();
    }
    let _ = store.upsert_binding(moved);
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    use chrono::TimeDelta;
    use url::Url;

    use bmm_domain::library::{
        CheckState, FileSnapshot, InMemoryLibraryStore, InstalledMod, ModBinding,
    };
    use bmm_domain::models::Listing;
    use bmm_sources::sources::{ModSource, SourceError};

    use super::*;

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-13T12:00:00+00:00").unwrap()
    }

    fn u(source: &str, id: &str) -> String {
        format!("https://{source}.test/{id}")
    }

    fn l(source: &str, id: &str, version: Option<&str>, size: Option<i64>) -> Listing {
        let mut listing = Listing::new(source, Url::parse(&u(source, id)).unwrap(), id, id);
        listing.mod_version = version.map(str::to_string);
        listing.size_bytes = size;
        listing
    }

    struct Site {
        id: &'static str,
        feed: Mutex<Vec<Listing>>,
        cards: Mutex<HashMap<String, Listing>>,
        feed_calls: AtomicUsize,
        card_calls: AtomicUsize,
        feed_fails: Mutex<Option<SourceError>>,
        card_fails: Mutex<Option<SourceError>>,
        on_card: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
        feed_covers: bool,
        /// Многостраничная лента: страница N — элемент N-1. Пусто — только `feed`.
        pages: Mutex<Vec<Vec<Listing>>>,
        /// Что лежит за кнопкой «Скачать»; `None` — ссылка не проходит.
        file: Mutex<Option<FileSnapshot>>,
        /// Какие страницы просили проверить файл и можно ли было открыть страницу.
        probes: Mutex<Vec<(String, bool)>>,
        /// Каждая версия — своя страница, как в Telegram-канале.
        per_version: AtomicBool,
    }

    impl Site {
        fn new(id: &'static str) -> Arc<Self> {
            Arc::new(Self {
                id,
                feed: Mutex::new(Vec::new()),
                cards: Mutex::new(HashMap::new()),
                feed_calls: AtomicUsize::new(0),
                card_calls: AtomicUsize::new(0),
                feed_fails: Mutex::new(None),
                card_fails: Mutex::new(None),
                on_card: Mutex::new(None),
                feed_covers: true,
                pages: Mutex::new(Vec::new()),
                file: Mutex::new(None),
                probes: Mutex::new(Vec::new()),
                per_version: AtomicBool::new(false),
            })
        }

        /// Карточка по адресу `url` — запись с другим адресом (новый пост).
        fn set_card_at(&self, url: &str, listing: Listing) {
            self.cards.lock().unwrap().insert(url.to_string(), listing);
        }

        fn set_file(&self, file: Option<FileSnapshot>) {
            *self.file.lock().unwrap() = file;
        }

        fn probes(&self) -> Vec<(String, bool)> {
            self.probes.lock().unwrap().clone()
        }

        fn with_feed(self: Arc<Self>, feed: Vec<Listing>) -> Arc<Self> {
            *self.feed.lock().unwrap() = feed;
            self
        }

        fn set_feed(&self, feed: Vec<Listing>) {
            *self.feed.lock().unwrap() = feed;
        }

        fn set_card(&self, listing: Listing) {
            self.cards
                .lock()
                .unwrap()
                .insert(listing.url.to_string(), listing);
        }

        fn set_feed_failure(&self, error: Option<SourceError>) {
            *self.feed_fails.lock().unwrap() = error;
        }

        fn set_card_failure(&self, error: Option<SourceError>) {
            *self.card_fails.lock().unwrap() = error;
        }

        fn on_card(&self, callback: Arc<dyn Fn() + Send + Sync>) {
            *self.on_card.lock().unwrap() = Some(callback);
        }

        fn feed_calls(&self) -> usize {
            self.feed_calls.load(Ordering::SeqCst)
        }

        fn card_calls(&self) -> usize {
            self.card_calls.load(Ordering::SeqCst)
        }
    }

    impl ModSource for Site {
        fn id(&self) -> &'static str {
            self.id
        }
        fn display_name(&self) -> &'static str {
            self.id
        }
        fn supports_search(&self) -> bool {
            false
        }
        fn home(&self) -> Url {
            Url::parse(&format!("https://{}.test/", self.id)).unwrap()
        }
        fn owns(&self, url: &Url) -> bool {
            let host = format!("{}.test", self.id);
            url.host_str() == Some(host.as_str())
        }
        fn is_mod_page(&self, url: &Url) -> bool {
            self.owns(url) && url.path() != "/"
        }
        fn feed_covers(&self, _url: &Url) -> bool {
            self.feed_covers
        }
        fn search(&self, _query: &str, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }
        fn feed_page(&self, page: u32, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            self.feed_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(error) = self.feed_fails.lock().unwrap().clone() {
                return Err(error);
            }
            let pages = self.pages.lock().unwrap();
            if !pages.is_empty() {
                return Ok(pages.get(page as usize - 1).cloned().unwrap_or_default());
            }
            if page == 1 {
                Ok(self.feed.lock().unwrap().clone())
            } else {
                Ok(Vec::new())
            }
        }
        fn listing(&self, url: &Url, _cancel: &AtomicBool) -> Result<Option<Listing>, SourceError> {
            self.card_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(callback) = self.on_card.lock().unwrap().clone() {
                callback();
            }
            if let Some(error) = self.card_fails.lock().unwrap().clone() {
                return Err(error);
            }
            Ok(self.cards.lock().unwrap().get(url.as_str()).cloned())
        }
        fn probes_files(&self) -> bool {
            true
        }
        fn page_per_version(&self) -> bool {
            self.per_version.load(Ordering::SeqCst)
        }
        fn probe_file(
            &self,
            listing: &Listing,
            _previous: Option<&FileSnapshot>,
            open_page: bool,
            _cancel: &AtomicBool,
        ) -> bmm_sources::files::FileProbe {
            self.probes
                .lock()
                .unwrap()
                .push((listing.url.to_string(), open_page));
            bmm_sources::files::FileProbe {
                file: self.file.lock().unwrap().clone(),
                // Страницу «открываем», только если разрешили.
                opened_page: open_page,
            }
        }
    }

    fn add(
        store: &InMemoryLibraryStore,
        mod_id: &str,
        source: &str,
        source_id: &str,
        version: Option<&str>,
        last_version: Option<&str>,
        checked_at: Option<DateTime<FixedOffset>>,
    ) {
        store.upsert_mod(InstalledMod {
            id: mod_id.to_string(),
            name: mod_id.to_string(),
            file_name: format!("{mod_id}.zip"),
            version: version.map(str::to_string),
            size_bytes: None,
            installed_at_utc: now() - TimeDelta::days(60),
            enabled: true,
        });
        store
            .upsert_binding(ModBinding {
                mod_id: mod_id.to_string(),
                source: source.to_string(),
                source_id: source_id.to_string(),
                url: Url::parse(&u(source, source_id)).unwrap(),
                confirmed: true,
            })
            .unwrap();
        if let Some(checked_at) = checked_at {
            store.save_check(CheckState {
                last_file: None,
                mod_id: mod_id.to_string(),
                source: source.to_string(),
                last_checked_utc: checked_at,
                last_changed_utc: Some(checked_at),
                last_version: last_version.map(str::to_string),
                last_size: None,
                miss_count: 0,
                last_game_version: None,
                last_title: None,
            });
        }
    }

    fn sources(site: &Arc<Site>) -> Vec<Arc<dyn ModSource>> {
        vec![Arc::clone(site) as Arc<dyn ModSource>]
    }

    fn run_simple(
        store: &InMemoryLibraryStore,
        sources: &[Arc<dyn ModSource>],
        at: DateTime<FixedOffset>,
        options: UpdateCheckOptions,
    ) -> UpdateRunReport {
        let cancel = AtomicBool::new(false);
        let request = UpdateRun::new(store, sources, at, options, &cancel);
        UpdateRunner::run(&request, None)
    }

    fn snapshot(name: &str) -> FileSnapshot {
        FileSnapshot {
            gone: false,
            offer: None,
            seen_at: None,
            url: "https://modsfire.com/abc".to_string(),
            name: Some(name.to_string()),
            size: Some(1000),
            exact: false,
            page_key: None,
        }
    }

    #[test]
    fn opened_card_also_notes_the_file_behind_the_download_button() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.1"), None)]);
        site.set_card(l("x", "a", Some("1.1"), None));
        site.set_file(Some(snapshot("a_v1.1.zip")));
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        let file = store.get_check("a", "x").unwrap().last_file.unwrap();
        assert_eq!(file.name.as_deref(), Some("a_v1.1.zip"));
        assert_eq!(
            file.page_key.as_deref(),
            Some("1.1"),
            "снимок привязан к подписи, при которой его сделали"
        );
    }

    #[test]
    fn binding_moves_to_the_post_of_the_new_version() {
        // Telegram: v1.1 — новый пост. «Обновить» должен вести к нему, а не к посту v1.0.
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.1"), None)]);
        site.per_version.store(true, Ordering::SeqCst);
        site.set_card_at(&u("x", "a"), l("x", "a-new", Some("1.1"), None));
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        let binding = store
            .bindings()
            .into_iter()
            .find(|item| item.mod_id == "a")
            .unwrap();
        assert_eq!(binding.url.as_str(), u("x", "a-new"));
        assert_eq!(binding.source_id, "a-new");
        assert!(binding.confirmed);
        assert_eq!(
            store.get_check("a", "x").unwrap().last_version.as_deref(),
            Some("1.1")
        );
    }

    #[test]
    fn ordinary_site_keeps_its_page_even_if_the_card_says_otherwise() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.1"), None)]);
        site.set_card_at(&u("x", "a"), l("x", "a-new", Some("1.1"), None));
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        let binding = store
            .bindings()
            .into_iter()
            .find(|item| item.mod_id == "a")
            .unwrap();
        assert_eq!(binding.url.as_str(), u("x", "a"));
    }

    #[test]
    fn page_of_another_site_is_never_taken() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.1"), None)]);
        site.per_version.store(true, Ordering::SeqCst);
        site.set_card_at(&u("x", "a"), l("y", "a", Some("1.1"), None));
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        let binding = store
            .bindings()
            .into_iter()
            .find(|item| item.mod_id == "a")
            .unwrap();
        assert_eq!(binding.url.as_str(), u("x", "a"));
    }

    #[test]
    fn mods_confirmed_by_the_feed_are_not_probed() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.0"), None)]);
        site.set_file(Some(snapshot("a.zip")));
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert!(
            site.probes().is_empty(),
            "карточку не открывали — ссылку тоже"
        );
    }

    #[test]
    fn broken_download_link_keeps_the_previous_file_details() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        let mut check = store.get_check("a", "x").unwrap();
        check.last_file = Some(FileSnapshot {
            page_key: Some("1.0".to_string()),
            ..snapshot("a_v1.0.zip")
        });
        store.save_check(check);
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.1"), None)]);
        site.set_card(l("x", "a", Some("1.1"), None));
        site.set_file(None);
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        let file = store.get_check("a", "x").unwrap().last_file.unwrap();
        // Старый снимок остался, но он о прежней подписи — новую версию не скроет.
        assert_eq!(file.page_key.as_deref(), Some("1.0"));
        let pending = UpdateCheckService::pending(&store, false);
        assert_eq!(
            pending[0].status,
            bmm_domain::versioning::UpdateStatus::Likely
        );
    }

    #[test]
    fn finding_without_file_details_is_reopened_out_of_turn() {
        // Было: сведения о файле пропали (их стёрла прежняя версия), а мод проверен
        // час назад — до завтра «тот же файл» никто не увидит.
        let store = InMemoryLibraryStore::new();
        let hour_ago = now() - TimeDelta::hours(1);
        add(
            &store,
            "smoke",
            "x",
            "smoke",
            Some("0.14.1"),
            Some("1.14.1"),
            Some(hour_ago),
        );
        add(
            &store,
            "calm",
            "x",
            "calm",
            Some("1.0"),
            Some("1.0"),
            Some(hour_ago),
        );
        let site = Site::new("x");
        site.set_card(l("x", "smoke", Some("1.14.1"), None));
        site.set_file(Some(snapshot("smoke.zip")));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.card_calls(), 1, "только находка, без очереди");
        assert_eq!(site.feed_calls(), 0, "ленту сайта по очереди не читали");
        assert_eq!(report.fresh_skipped, 2);
        let file = store.get_check("smoke", "x").unwrap().last_file.unwrap();
        assert_eq!(file.page_key.as_deref(), Some("1.14.1"));

        // Сведения свежие — второй раз не открываем.
        let again = Site::new("x");
        again.set_card(l("x", "smoke", Some("1.14.1"), None));
        run_simple(
            &store,
            &sources(&again),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(again.card_calls(), 0);
    }

    #[test]
    fn site_asking_for_a_human_check_is_not_reopened_for_files() {
        let store = InMemoryLibraryStore::new();
        let hour_ago = now() - TimeDelta::hours(1);
        add(
            &store,
            "smoke",
            "x",
            "smoke",
            Some("0.14.1"),
            Some("1.14.1"),
            Some(hour_ago),
        );
        store.set_source_status("x", bmm_domain::library::SourceStatus::NeedsHumanCheck);
        let site = Site::new("x");
        site.set_card(l("x", "smoke", Some("1.14.1"), None));
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.card_calls(), 0);
    }

    #[test]
    fn download_link_is_not_visited_without_need() {
        // Ссылку «Скачать» сайт может считать скачиванием: при проверке по кнопке
        // каждый раз проходить её у всех модов — накручивать авторам счётчики.
        let store = InMemoryLibraryStore::new();
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(now()));
        let check = store.get_check("a", "x").unwrap();
        let fresh = FileSnapshot {
            page_key: Some("1.0".to_string()),
            offer: Some("https://x.test/dl/1".to_string()),
            seen_at: Some(now() - TimeDelta::days(1)),
            ..snapshot("a.zip")
        };
        let offer = Some("https://x.test/dl/1");
        assert!(
            !file_needs_look(Some(&fresh), &check, offer, now()),
            "всё известно"
        );
        assert!(
            file_needs_look(None, &check, offer, now()),
            "ничего не знаем"
        );
        assert!(
            file_needs_look(Some(&fresh), &check, Some("https://x.test/dl/2"), now()),
            "новая ссылка — новый файл"
        );
        let other_label = FileSnapshot {
            page_key: Some("0.9".to_string()),
            ..fresh.clone()
        };
        assert!(file_needs_look(Some(&other_label), &check, offer, now()));
        let old = FileSnapshot {
            seen_at: Some(now() - FILE_FRESH_FOR),
            ..fresh.clone()
        };
        assert!(
            file_needs_look(Some(&old), &check, offer, now()),
            "неделя прошла"
        );
        let nameless = FileSnapshot {
            name: None,
            ..fresh.clone()
        };
        assert!(file_needs_look(Some(&nameless), &check, offer, now()));
        let deleted = FileSnapshot {
            name: None,
            gone: true,
            ..fresh.clone()
        };
        assert!(
            !file_needs_look(Some(&deleted), &check, offer, now()),
            "файл удалён — ждём новую ссылку"
        );
        let unseen = FileSnapshot {
            seen_at: None,
            ..fresh
        };
        assert!(
            file_needs_look(Some(&unseen), &check, offer, now()),
            "старый снимок"
        );
    }

    #[test]
    fn full_check_by_the_button_does_not_walk_every_download_link() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::hours(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        // Ленты нет — проверить мод можно только по его странице.
        let site = Site::new("x");
        site.set_card(l("x", "a", Some("1.0"), None));
        site.set_file(Some(snapshot("a.zip")));
        let force = UpdateCheckOptions::default().with_force(true);
        run_simple(&store, &sources(&site), now(), force.clone());
        assert_eq!(site.probes().len(), 1, "первый раз — смотрим");
        run_simple(&store, &sources(&site), now(), force);
        assert_eq!(
            site.card_calls(),
            2,
            "страницу по кнопке открываем каждый раз"
        );
        assert_eq!(site.probes().len(), 1, "а по ссылке второй раз не идём");
    }

    #[test]
    fn file_host_pages_are_limited_per_check() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        let count = FILE_PAGES_PER_RUN + 5;
        let mut feed = Vec::new();
        let site = Site::new("x");
        for index in 0..count {
            let id = format!("m{index}");
            add(&store, &id, "x", &id, Some("1.0"), Some("1.0"), Some(old));
            feed.push(l("x", &id, Some("1.1"), None));
            site.set_card(l("x", &id, Some("1.1"), None));
        }
        site.set_feed(feed);
        site.set_file(Some(snapshot("m.zip")));
        let options = UpdateCheckOptions {
            max_card_checks_per_source: count + 10,
            ..UpdateCheckOptions::default()
        };
        run_simple(&store, &sources(&site), now(), options);
        let probes = site.probes();
        assert_eq!(probes.len(), count, "ссылку проходим у каждого");
        assert_eq!(
            probes.iter().filter(|(_, page)| *page).count(),
            FILE_PAGES_PER_RUN,
            "страницу файлообменника — не больше предела"
        );
    }

    #[test]
    fn finds_update_by_feed_and_card_and_confirms_unchanged_by_feed() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        add(&store, "b", "x", "b", Some("1.0"), Some("1.0"), Some(old));
        let site = Site::new("x").with_feed(vec![
            l("x", "a", Some("1.1"), None),
            l("x", "b", Some("1.0"), None),
        ]);
        site.set_card(l("x", "a", Some("1.1"), None));

        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].mod_id, "a");
        assert_eq!(report.findings[0].latest_version.as_deref(), Some("1.1"));
        assert_eq!(site.card_calls(), 1, "b подтверждён лентой");
        assert_eq!(report.up_to_date, 1);
        assert!(report.complete());
    }

    #[test]
    fn sites_are_checked_in_parallel() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        add(&store, "b", "y", "b", Some("1.0"), Some("1.0"), Some(old));
        let x = Site::new("x").with_feed(vec![l("x", "a", Some("1.1"), None)]);
        let y = Site::new("y").with_feed(vec![l("y", "b", Some("1.1"), None)]);
        x.set_card(l("x", "a", Some("1.1"), None));
        y.set_card(l("y", "b", Some("1.1"), None));
        let slow: Arc<dyn Fn() + Send + Sync> =
            Arc::new(|| std::thread::sleep(std::time::Duration::from_millis(400)));
        x.on_card(slow.clone());
        y.on_card(slow);
        let all: Vec<Arc<dyn ModSource>> = vec![x.clone(), y.clone()];

        let started = std::time::Instant::now();
        let report = run_simple(&store, &all, now(), UpdateCheckOptions::default());
        let elapsed = started.elapsed();

        assert_eq!(report.findings.len(), 2);
        assert!(report.complete());
        // По очереди было бы ≥ 800 мс; вместе — время самого медленного сайта.
        assert!(
            elapsed < std::time::Duration::from_millis(750),
            "{elapsed:?}"
        );
    }

    #[test]
    fn progress_from_all_sites_reaches_the_callback() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        add(&store, "b", "y", "b", Some("1.0"), Some("1.0"), Some(old));
        let x = Site::new("x").with_feed(vec![l("x", "a", Some("1.0"), None)]);
        let y = Site::new("y").with_feed(vec![l("y", "b", Some("1.0"), None)]);
        let all: Vec<Arc<dyn ModSource>> = vec![x, y];
        let cancel = AtomicBool::new(false);
        let request = UpdateRun::new(&store, &all, now(), UpdateCheckOptions::default(), &cancel);
        let mut seen: Vec<String> = Vec::new();
        let mut callback = |progress: &UpdateProgress| seen.push(progress.source.clone());
        run(&request, Some(&mut callback));
        assert!(seen.iter().any(|source| source == "x"));
        assert!(seen.iter().any(|source| source == "y"));
        assert_eq!(
            seen.last().map(String::as_str),
            Some(""),
            "«готово» последним"
        );
    }

    #[test]
    fn two_hundred_mods_with_fresh_feed_need_no_cards() {
        let store = InMemoryLibraryStore::new();
        let site = Site::new("x");
        let old = now() - TimeDelta::days(2);
        for index in 0..200 {
            add(
                &store,
                &format!("m{index}"),
                "x",
                &format!("m{index}"),
                Some("1.0"),
                Some("1.0"),
                Some(old),
            );
            site.feed
                .lock()
                .unwrap()
                .push(l("x", &format!("m{index}"), Some("1.0"), None));
        }
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.card_calls(), 0);
        assert!(report.requests <= 10, "запросов {}", report.requests);
        assert_eq!(report.up_to_date, 200);
    }

    #[test]
    fn missing_from_feed_uses_card() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(5)),
        );
        let site = Site::new("x");
        site.set_card(l("x", "a", Some("2.0"), None));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.card_calls(), 1);
        assert_eq!(report.findings[0].latest_version.as_deref(), Some("2.0"));
    }

    #[test]
    fn human_check_marks_only_that_site() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        add(&store, "b", "y", "b", Some("1.0"), Some("1.0"), Some(old));
        let x = Site::new("x");
        x.set_feed_failure(Some(SourceError {
            status: SourceStatus::NeedsHumanCheck,
            message: "challenge".to_string(),
        }));
        let y = Site::new("y").with_feed(vec![l("y", "b", Some("1.5"), None)]);
        y.set_card(l("y", "b", Some("1.5"), None));

        let all: Vec<Arc<dyn ModSource>> = vec![
            Arc::clone(&x) as Arc<dyn ModSource>,
            Arc::clone(&y) as Arc<dyn ModSource>,
        ];
        let report = run_simple(&store, &all, now(), UpdateCheckOptions::default());
        assert_eq!(
            store.source_status("x"),
            bmm_domain::library::SourceStatus::NeedsHumanCheck
        );
        assert_eq!(report.findings[0].mod_id, "b");
        assert!(!report.complete());
        assert_eq!(
            report
                .sources
                .iter()
                .find(|source| source.source == "x")
                .unwrap()
                .status,
            SourceStatus::NeedsHumanCheck
        );
        assert_eq!(x.card_calls(), 0);
    }

    #[test]
    fn next_run_skips_human_check_site() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(2)),
        );
        store.set_source_status("x", bmm_domain::library::SourceStatus::NeedsHumanCheck);
        let site = Site::new("x");
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.feed_calls(), 0);
        assert_eq!(report.requests, 0);
        assert!(
            report.sources[0]
                .note
                .as_deref()
                .unwrap()
                .contains("человек")
        );
    }

    #[test]
    fn unavailable_site_keeps_old_state_and_finding() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.5"),
            Some(now() - TimeDelta::days(2)),
        );
        let site = Site::new("x");
        site.set_feed_failure(Some(SourceError::unavailable("502")));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(report.findings[0].latest_version.as_deref(), Some("1.5"));
        assert_eq!(report.sources[0].status, SourceStatus::Unavailable);
        assert_eq!(
            store.source_status("x"),
            bmm_domain::library::SourceStatus::Ok,
            "недоступность не залипает"
        );
    }

    #[test]
    fn missing_card_404_does_not_damage_state() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(5)),
        );
        let site = Site::new("x");
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(report.sources[0].missing_mods, vec!["a".to_string()]);
        assert_eq!(
            store.get_check("a", "x").unwrap().last_version.as_deref(),
            Some("1.0")
        );
        assert_eq!(store.get_check("a", "x").unwrap().miss_count, 1);
    }

    #[test]
    fn disabled_site_without_adapter_is_skipped_silently() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        add(&store, "b", "y", "b", Some("1.0"), Some("1.0"), Some(old));
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.0"), None)]);

        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );

        assert_eq!(report.sources.len(), 1, "{:?}", report.sources);
        assert_eq!(report.sources[0].source, "x");
        assert!(report.requests <= 2, "к выключенному сайту не ходим");
    }

    #[test]
    fn immediate_second_run_requests_nothing() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(2)),
        );
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.0"), None)]);
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        let before = site.feed_calls();
        let report = run_simple(
            &store,
            &sources(&site),
            now() + TimeDelta::minutes(5),
            UpdateCheckOptions::default(),
        );
        assert_eq!(before, site.feed_calls());
        assert_eq!(report.requests, 0);
    }

    #[test]
    fn force_ignores_freshness() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::minutes(5)),
        );
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.0"), None)]);
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default().with_force(true),
        );
        assert!(site.feed_calls() > 0);
    }

    #[test]
    fn binding_to_unknown_site_is_silently_skipped() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "zzz",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(2)),
        );
        let report = run_simple(&store, &[], now(), UpdateCheckOptions::default());
        assert!(report.sources.is_empty(), "{:?}", report.sources);
        assert_eq!(report.requests, 0);
    }

    #[test]
    fn cancellation_keeps_what_was_done() {
        let store = Arc::new(InMemoryLibraryStore::new());
        let site = Site::new("x");
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancel);
        site.on_card(Arc::new(move || flag.store(true, Ordering::SeqCst)));
        for index in 0..4 {
            add(
                &store,
                &format!("m{index}"),
                "x",
                &format!("m{index}"),
                Some("1.0"),
                Some("1.0"),
                Some(now() - TimeDelta::days(5)),
            );
            site.set_card(l("x", &format!("m{index}"), Some("2.0"), None));
        }
        let all = sources(&site);
        // Отменяем именно тот флаг, который передан проверке.
        let request = UpdateRun {
            store: &*store,
            sources: &all,
            now: now(),
            options: UpdateCheckOptions::default(),
            cancel: &cancel,
            clock: None,
            hold_off: BTreeMap::new(),
        };
        let report = UpdateRunner::run(&request, None);
        assert!(report.cancelled);
        assert!(!report.complete());
        assert_eq!(site.card_calls(), 1);
        assert_eq!(report.findings.len(), 1);
    }

    #[test]
    fn foreign_adapter_failure_does_not_stop_others() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        add(&store, "b", "y", "b", Some("1.0"), Some("1.0"), Some(old));
        let x = Site::new("x");
        x.set_feed_failure(Some(SourceError::unavailable("reset")));
        let y = Site::new("y").with_feed(vec![l("y", "b", Some("2.0"), None)]);
        y.set_card(l("y", "b", Some("2.0"), None));
        let all: Vec<Arc<dyn ModSource>> = vec![
            Arc::clone(&x) as Arc<dyn ModSource>,
            Arc::clone(&y) as Arc<dyn ModSource>,
        ];
        let report = run_simple(&store, &all, now(), UpdateCheckOptions::default());
        assert_eq!(
            report
                .sources
                .iter()
                .find(|source| source.source == "x")
                .unwrap()
                .status,
            SourceStatus::Unavailable
        );
        assert_eq!(report.findings[0].mod_id, "b");
    }

    #[test]
    fn timeout_without_user_cancel_is_site_failure() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(5)),
        );
        let site = Site::new("x");
        site.set_card_failure(Some(SourceError::unavailable("timeout")));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert!(!report.cancelled);
        assert_eq!(report.sources[0].status, SourceStatus::Unavailable);
    }

    #[test]
    fn deletion_during_check_leaves_no_ghost_state() {
        let store = Arc::new(InMemoryLibraryStore::new());
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(5)),
        );
        let site = Site::new("x");
        site.set_card(l("x", "a", Some("2.0"), None));
        let store_for_callback = Arc::clone(&store);
        site.on_card(Arc::new(move || store_for_callback.remove_mod("a")));
        let all = sources(&site);
        let cancel = AtomicBool::new(false);
        let request = UpdateRun::new(&*store, &all, now(), UpdateCheckOptions::default(), &cancel);
        let report = UpdateRunner::run(&request, None);
        assert!(report.findings.is_empty());
        assert!(store.checks().is_empty());
        assert_eq!(report.sources[0].status, SourceStatus::Ok);
    }

    #[test]
    fn unparsed_card_version_does_not_forget_known_one() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.5"),
            Some(now() - TimeDelta::days(5)),
        );
        let site = Site::new("x");
        site.set_card(l("x", "a", None, None));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(
            store.get_check("a", "x").unwrap().last_version.as_deref(),
            Some("1.5")
        );
        assert_eq!(report.findings[0].latest_version.as_deref(), Some("1.5"));
    }

    #[test]
    fn future_check_time_does_not_block_forever() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() + TimeDelta::days(3)),
        );
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.0"), None)]);
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert!(site.feed_calls() > 0);
    }

    #[test]
    fn empty_feed_marks_layout_but_still_opens_cards() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(5)),
        );
        let site = Site::new("x");
        site.set_card(l("x", "a", Some("1.0"), None));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert!(
            report.sources[0]
                .note
                .as_deref()
                .unwrap()
                .contains("лента пуста")
        );
        assert!(!report.complete());
        assert_eq!(site.card_calls(), 1);
    }

    #[test]
    fn many_missing_cards_mean_broken_layout_not_deleted_mods() {
        let store = InMemoryLibraryStore::new();
        for index in 0..4 {
            add(
                &store,
                &format!("m{index}"),
                "x",
                &format!("m{index}"),
                Some("1.0"),
                Some("1.0"),
                Some(now() - TimeDelta::days(5)),
            );
        }
        let report = run_simple(
            &store,
            &sources(&Site::new("x")),
            now(),
            UpdateCheckOptions::default(),
        );
        assert!(
            report.sources[0]
                .note
                .as_deref()
                .unwrap()
                .contains("изменились адреса")
        );
        assert!(store.checks().iter().all(|check| check.miss_count == 0));
    }

    #[test]
    fn first_check_of_two_hundred_uses_feed_baseline() {
        let store = InMemoryLibraryStore::new();
        let site = Site::new("x");
        for index in 0..200 {
            store.upsert_mod(InstalledMod {
                id: format!("m{index}"),
                name: format!("m{index}"),
                file_name: format!("m{index}.zip"),
                version: Some("1.0".to_string()),
                size_bytes: None,
                installed_at_utc: now() - TimeDelta::days(60),
                enabled: true,
            });
            store
                .upsert_binding(ModBinding {
                    mod_id: format!("m{index}"),
                    source: "x".to_string(),
                    source_id: format!("m{index}"),
                    url: Url::parse(&u("x", &format!("m{index}"))).unwrap(),
                    confirmed: true,
                })
                .unwrap();
            site.feed.lock().unwrap().push(l(
                "x",
                &format!("m{index}"),
                Some(if index == 7 { "2.0" } else { "1.0" }),
                None,
            ));
        }
        site.set_card(l("x", "m7", Some("2.0"), Some(50 * 1024 * 1024)));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        // Карточка — только у того, чья версия в ленте другая: нужен размер для решения.
        assert_eq!(site.card_calls(), 1);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].mod_id, "m7");
        assert!(report.complete());
    }

    #[test]
    fn first_check_opens_card_when_feed_version_differs() {
        // rls_career_overhaul_v2.8.0_alpha2.zip: лента STMods даёт «2.8.0» без размера.
        // Без карточки это было «обновление» вслепую; с карточкой размер тот же — не обновление.
        let mb = 1024 * 1024;
        let store = InMemoryLibraryStore::new();
        let site = Site::new("x");
        store.upsert_mod(InstalledMod {
            id: "rls".to_string(),
            name: "rls career overhaul".to_string(),
            file_name: "rls_career_overhaul_v2.8.0_alpha2.zip".to_string(),
            version: Some("2.8.0-alpha2".to_string()),
            size_bytes: Some(368 * mb),
            installed_at_utc: now() - TimeDelta::days(3),
            enabled: true,
        });
        store
            .upsert_binding(ModBinding {
                mod_id: "rls".to_string(),
                source: "x".to_string(),
                source_id: "rls".to_string(),
                url: Url::parse(&u("x", "rls")).unwrap(),
                confirmed: true,
            })
            .unwrap();
        site.feed
            .lock()
            .unwrap()
            .push(l("x", "rls", Some("2.8.0"), None));
        site.set_card(l("x", "rls", Some("2.8.0"), Some(368 * mb)));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.card_calls(), 1);
        assert_eq!(
            store.get_check("rls", "x").unwrap().last_size,
            Some(368 * mb)
        );
        assert!(report.findings.is_empty(), "{:?}", report.findings);
    }

    #[test]
    fn time_limit_stops_and_next_run_finishes() {
        let store = InMemoryLibraryStore::new();
        let site = Site::new("x");
        for index in 0..6 {
            add(
                &store,
                &format!("m{index}"),
                "x",
                &format!("m{index}"),
                Some("1.0"),
                Some("1.0"),
                Some(now() - TimeDelta::days(5)),
            );
            site.set_card(l("x", &format!("m{index}"), Some("1.0"), None));
        }
        let tick = Arc::new(Mutex::new(now()));
        let tick_for_card = Arc::clone(&tick);
        site.on_card(Arc::new(move || {
            *tick_for_card.lock().unwrap() += TimeDelta::minutes(4);
        }));
        let options = UpdateCheckOptions {
            max_duration: std::time::Duration::from_secs(10 * 60),
            ..UpdateCheckOptions::default()
        };
        let all = sources(&site);
        let cancel = AtomicBool::new(false);
        let clock = {
            let tick = Arc::clone(&tick);
            move || *tick.lock().unwrap()
        };
        let request = UpdateRun {
            store: &store,
            sources: &all,
            now: now(),
            options: options.clone(),
            cancel: &cancel,
            clock: Some(&clock),
            hold_off: BTreeMap::new(),
        };
        let report = UpdateRunner::run(&request, None);
        assert!(report.stopped_by_time);
        assert!(!report.complete());
        let calls = site.card_calls();
        assert!(calls > 0 && calls < 6, "карточек {calls}");

        let report = run_simple(&store, &all, now() + TimeDelta::minutes(1), options);
        assert!(!report.stopped_by_time);
        let checked = store
            .checks()
            .iter()
            .filter(|check| check.last_checked_utc >= now())
            .count();
        assert_eq!(checked, 6);
    }

    #[test]
    fn duplicate_adapters_with_same_id_do_not_break() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(2)),
        );
        let first = Site::new("x").with_feed(vec![l("x", "a", Some("1.0"), None)]);
        let second = Site::new("x");
        let all: Vec<Arc<dyn ModSource>> = vec![first, second];
        let report = run_simple(&store, &all, now(), UpdateCheckOptions::default());
        assert_eq!(report.up_to_date, 1);
    }

    #[test]
    fn new_findings_and_fresh_skipped_across_presses() {
        let store = InMemoryLibraryStore::new();
        let old = now() - TimeDelta::days(2);
        add(&store, "a", "x", "a", Some("1.0"), Some("1.0"), Some(old));
        add(&store, "b", "x", "b", Some("1.0"), Some("1.0"), Some(old));
        let site = Site::new("x").with_feed(vec![
            l("x", "a", Some("1.5"), None),
            l("x", "b", Some("1.0"), None),
        ]);
        site.set_card(l("x", "a", Some("1.5"), None));
        let all = sources(&site);
        let options = UpdateCheckOptions::default();

        let first = run_simple(&store, &all, now(), options.clone());
        assert_eq!(first.new_findings.len(), 1);

        let second = run_simple(&store, &all, now() + TimeDelta::hours(7), options.clone());
        assert_eq!(second.new_findings.len(), 0);

        site.set_feed(vec![
            l("x", "a", Some("1.5"), None),
            l("x", "b", Some("3.0"), None),
        ]);
        site.set_card(l("x", "b", Some("3.0"), None));
        let third = run_simple(&store, &all, now() + TimeDelta::hours(14), options.clone());
        let ids: Vec<&str> = third
            .new_findings
            .iter()
            .map(|finding| finding.mod_id.as_str())
            .collect();
        assert_eq!(ids, ["b"]);

        let fourth = run_simple(
            &store,
            &all,
            now() + TimeDelta::hours(14) + TimeDelta::minutes(1),
            options,
        );
        assert_eq!(fourth.fresh_skipped, 2);
    }

    #[test]
    fn dated_feed_coverage_skips_cards() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(2)),
        );
        let site = Site::new("x");
        {
            let mut feed = site.feed.lock().unwrap();
            for index in 0..20 {
                let mut entry = l("x", &format!("id{index}"), Some("1.0"), None);
                entry.updated_at = Some(now() - TimeDelta::days(index));
                feed.push(entry);
            }
        }
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.card_calls(), 0);
        assert_eq!(report.up_to_date, 1);
        assert_eq!(site.feed_calls(), 1, "лента дошла до давней проверки");
    }

    /// Лента на `days` страниц: на странице p — 20 записей, обновлённых p дней назад.
    fn dated_pages(site: &Site, days: i64) {
        let mut pages = site.pages.lock().unwrap();
        for day in 1..=days {
            pages.push(
                (0..20)
                    .map(|index| {
                        let mut entry = l("x", &format!("f{day}-{index}"), Some("1.0"), None);
                        entry.updated_at = Some(now() - TimeDelta::days(day));
                        entry
                    })
                    .collect(),
            );
        }
    }

    #[test]
    fn dated_feed_is_read_back_to_the_last_check_instead_of_opening_pages() {
        // Программу открыли через 20 дней: лента репозитория за эти дни — 21 страница,
        // а модов 30. Читаем ленту вглубь вместо 30 страниц модов.
        let store = InMemoryLibraryStore::new();
        for index in 0..30 {
            add(
                &store,
                &format!("m{index}"),
                "x",
                &format!("m{index}"),
                Some("1.0"),
                Some("1.0"),
                Some(now() - TimeDelta::days(20)),
            );
        }
        let site = Site::new("x");
        dated_pages(&site, 40);
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.card_calls(), 0, "страницы модов не открывали");
        assert_eq!(site.feed_calls(), 21, "лента ровно до прошлой проверки");
        assert_eq!(report.up_to_date, 30);
    }

    #[test]
    fn later_batches_do_not_reread_the_deep_feed() {
        // Вторая пачка той же проверки: ленту вглубь уже читали — снова 21 страница
        // дала бы те же записи.
        let store = InMemoryLibraryStore::new();
        for index in 0..30 {
            add(
                &store,
                &format!("m{index}"),
                "x",
                &format!("m{index}"),
                Some("1.0"),
                Some("1.0"),
                Some(now() - TimeDelta::days(20)),
            );
        }
        let site = Site::new("x");
        dated_pages(&site, 40);
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions {
                deep_feed: false,
                ..UpdateCheckOptions::default()
            },
        );
        assert_eq!(site.feed_calls(), 3, "только по плану");
    }

    #[test]
    fn deep_feed_never_costs_more_than_opening_the_pages() {
        // Модов всего 5 — двадцать страниц ленты ради них дороже, чем 5 страниц модов.
        let store = InMemoryLibraryStore::new();
        for index in 0..5 {
            add(
                &store,
                &format!("m{index}"),
                "x",
                &format!("m{index}"),
                Some("1.0"),
                Some("1.0"),
                Some(now() - TimeDelta::days(20)),
            );
        }
        let site = Site::new("x");
        dated_pages(&site, 40);
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.feed_calls(), 5);
    }

    #[test]
    fn undated_feed_is_not_read_deeper_than_planned() {
        let store = InMemoryLibraryStore::new();
        for index in 0..30 {
            add(
                &store,
                &format!("m{index}"),
                "x",
                &format!("m{index}"),
                Some("1.0"),
                Some("1.0"),
                Some(now() - TimeDelta::days(20)),
            );
        }
        let site = Site::new("x");
        {
            let mut pages = site.pages.lock().unwrap();
            for page in 0..40 {
                pages.push(
                    (0..20)
                        .map(|index| l("x", &format!("u{page}-{index}"), None, None))
                        .collect(),
                );
            }
        }
        run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.feed_calls(), 3, "без дат лента ничего не доказывает");
    }

    #[test]
    fn feed_coverage_does_not_apply_when_page_is_not_in_feed() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(2)),
        );
        let mut site = Site::new("x");
        Arc::get_mut(&mut site).unwrap().feed_covers = false;
        {
            let mut feed = site.feed.lock().unwrap();
            for index in 0..20 {
                let mut entry = l("x", &format!("id{index}"), Some("1.0"), None);
                entry.updated_at = Some(now() - TimeDelta::days(index));
                feed.push(entry);
            }
        }
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(
            site.card_calls(),
            1,
            "mod вне раздела ленты проверяем карточкой"
        );
        assert_eq!(report.up_to_date, 0);
    }

    #[test]
    fn hold_off_skips_source_without_requests() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(2)),
        );
        let site = Site::new("x");
        let all = sources(&site);
        let cancel = AtomicBool::new(false);
        let request = UpdateRun {
            store: &store,
            sources: &all,
            now: now(),
            options: UpdateCheckOptions::default(),
            cancel: &cancel,
            clock: None,
            hold_off: BTreeMap::from([("x".to_string(), "сайт недавно не отвечал".to_string())]),
        };
        let report = UpdateRunner::run(&request, None);
        assert_eq!(site.feed_calls(), 0);
        assert_eq!(report.requests, 0);
        assert_eq!(report.sources[0].status, SourceStatus::Unavailable);
        assert!(
            report.sources[0]
                .note
                .as_deref()
                .unwrap()
                .contains("не отвечал")
        );
    }

    #[test]
    fn miss_count_grows_then_dead_page_is_checked_weekly() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(5)),
        );
        let site = Site::new("x");
        let all = sources(&site);
        let options = UpdateCheckOptions::default();

        run_simple(&store, &all, now(), options.clone());
        assert_eq!(store.get_check("a", "x").unwrap().miss_count, 1);

        run_simple(&store, &all, now() + TimeDelta::days(2), options.clone());
        assert_eq!(store.get_check("a", "x").unwrap().miss_count, 2);
        let calls = site.card_calls();
        assert_eq!(calls, 2);

        // Мёртвую страницу теперь проверяем раз в неделю.
        run_simple(&store, &all, now() + TimeDelta::days(4), options.clone());
        assert_eq!(site.card_calls(), calls);

        // «Принудительно» — сразу.
        run_simple(
            &store,
            &all,
            now() + TimeDelta::days(4),
            options.with_force(true),
        );
        assert_eq!(site.card_calls(), calls + 1);
        assert_eq!(store.get_check("a", "x").unwrap().miss_count, 3);
    }

    #[test]
    fn feed_entry_resets_miss_count() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(2)),
        );
        let mut check = store.get_check("a", "x").unwrap();
        check.miss_count = 2;
        store.save_check(check);
        let site = Site::new("x").with_feed(vec![l("x", "a", Some("1.0"), None)]);
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        assert_eq!(site.card_calls(), 0);
        assert_eq!(report.up_to_date, 1);
        assert_eq!(store.get_check("a", "x").unwrap().miss_count, 0);
    }

    #[test]
    fn unreadable_card_keeps_previous_check_time() {
        let store = InMemoryLibraryStore::new();
        let checked = now() - TimeDelta::days(5);
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(checked),
        );
        let site = Site::new("x");
        site.set_card(l("x", "a", None, None));
        let report = run_simple(
            &store,
            &sources(&site),
            now(),
            UpdateCheckOptions::default(),
        );
        let check = store.get_check("a", "x").unwrap();
        assert_eq!(check.last_checked_utc, checked, "срок проверки не сдвинут");
        assert_eq!(check.last_version.as_deref(), Some("1.0"));
        assert!(
            report.sources[0]
                .note
                .as_deref()
                .unwrap()
                .contains("вёрстка")
        );
    }

    #[test]
    fn progress_is_reported_for_feeds_cards_and_finish() {
        let store = InMemoryLibraryStore::new();
        add(
            &store,
            "a",
            "x",
            "a",
            Some("1.0"),
            Some("1.0"),
            Some(now() - TimeDelta::days(5)),
        );
        let site = Site::new("x");
        site.set_card(l("x", "a", Some("1.0"), None));
        let all = sources(&site);
        let cancel = AtomicBool::new(false);
        let request = UpdateRun::new(&store, &all, now(), UpdateCheckOptions::default(), &cancel);
        let mut stages = Vec::new();
        UpdateRunner::run(
            &request,
            Some(&mut |progress: &UpdateProgress| {
                stages.push(format!("{}:{}", progress.source, progress.stage));
            }),
        );
        assert!(stages.iter().any(|stage| stage == "x:лента"));
        assert!(stages.iter().any(|stage| stage == "x:карточки"));
        assert_eq!(stages.last().map(String::as_str), Some(":готово"));
    }
}

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
    };
    runner.run()
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

        let tracked = UpdateCheckService::tracked(store);
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

        let mut jobs: Vec<(&String, u32, &std::sync::Arc<dyn ModSource>)> = Vec::new();
        for (source, pages) in &plan.feed_pages {
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
        let budget = &self.budget;
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
/// а не их сумму — как в legacy (`Task.WhenAll`).
struct Worker<'r, 'a> {
    request: &'r UpdateRun<'a>,
    budget: &'r Budget<'a>,
    progress: mpsc::Sender<UpdateProgress>,
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

        let mut limit = pages;
        let mut page = 1u32;
        while page <= limit {
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
                total: limit as usize,
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
            {
                if let Some(checked) = oldest_checked
                    && covered <= checked - FeedCoverage::MARGIN
                {
                    break; // дошли до самой давней проверки
                }
                if page == limit && limit < self.request.options.max_feed_pages_per_source {
                    limit += 1; // датированная лента: идём глубже
                }
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

        let Some(list) = card_plan.to_fetch.get(source) else {
            return outcome;
        };

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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{Arc, Mutex};

    use chrono::TimeDelta;
    use url::Url;

    use bmm_domain::library::{CheckState, InMemoryLibraryStore, InstalledMod, ModBinding};
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
            })
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
    fn binding_to_unknown_site_is_reported() {
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
        assert!(
            report.sources[0]
                .note
                .as_deref()
                .unwrap()
                .contains("адаптер")
        );
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

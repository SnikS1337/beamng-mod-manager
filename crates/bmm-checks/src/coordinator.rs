//! Единственная точка, через которую интерфейс запускает проверку.
//!
//! Защищает от серии нажатий: пока проверка идёт, второй запуск не начинается,
//! а ждущий получает тот же отчёт. Слишком частое «принудительно» возвращает
//! прошлый отчёт. Сайт, упавший с `Unavailable`, не опрашивается 5 минут.
//! После проверки «вы человек?» сайт пропускается, пока не вызван
//! [`UpdateCoordinator::resolve_human_check`].
//!
//! Проверка синхронная: Tauri-команда оборачивает вызов в `spawn_blocking`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use chrono::{DateTime, FixedOffset, TimeDelta};

use bmm_domain::clock::Clock;
use bmm_domain::library::{LibraryStore, SourceStatus as StoreSourceStatus};
use bmm_sources::sources::{ModSource, SourceStatus};

use crate::options::UpdateCheckOptions;
use crate::planner::UpdateCheckPlanner;
use crate::runner::{SourceRunReport, UpdateProgress, UpdateRun, UpdateRunReport};
use crate::service::UpdateCheckService;

/// Настройки координатора.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCoordinatorOptions {
    pub check: UpdateCheckOptions,
    /// После сбоя сайта (недоступен) не обращаться к нему столько времени.
    pub failure_backoff: TimeDelta,
    /// «Проверить заново» чаще этого срока возвращает прошлый отчёт.
    pub min_force_interval: TimeDelta,
    /// Предел числа частей одной проверки (5 минут × 12 = час).
    pub max_batches: u32,
}

impl Default for UpdateCoordinatorOptions {
    fn default() -> Self {
        Self {
            check: UpdateCheckOptions::default(),
            failure_backoff: TimeDelta::minutes(5),
            min_force_interval: TimeDelta::minutes(1),
            max_batches: 12,
        }
    }
}

/// Фабрика адаптеров: вызывается на каждый запуск, чтобы учитывать настройки сайтов.
type SourcesFactory = Arc<dyn Fn() -> Vec<Arc<dyn ModSource>> + Send + Sync>;
type Callback = Arc<dyn Fn() + Send + Sync>;
type SourceCallback = Arc<dyn Fn(&str) + Send + Sync>;

/// Координатор проверок: один запуск за раз, отмена, пауза после сбоя.
pub struct UpdateCoordinator {
    store: Arc<dyn LibraryStore + Send + Sync>,
    sources: SourcesFactory,
    clock: Clock,
    options: Mutex<UpdateCoordinatorOptions>,
    clear_cache: Option<Callback>,
    reset_source: Option<SourceCallback>,
    state: Mutex<State>,
    idle: Condvar,
}

#[derive(Default)]
struct State {
    running: Option<Running>,
    last_report: Option<UpdateRunReport>,
    last_finished: Option<DateTime<FixedOffset>>,
    next_run_id: u64,
    last_completed_run: Option<u64>,
    last_cancelled_run: Option<u64>,
    retry_after: HashMap<String, DateTime<FixedOffset>>,
    /// Сколько вызовов ждут идущую проверку (диагностика и тесты коалесинга).
    waiters: usize,
}

struct Running {
    id: u64,
    cancel: Arc<AtomicBool>,
}

impl UpdateCoordinator {
    pub fn new(
        store: Arc<dyn LibraryStore + Send + Sync>,
        sources: SourcesFactory,
        clock: Clock,
    ) -> Self {
        Self {
            store,
            sources,
            clock,
            options: Mutex::new(UpdateCoordinatorOptions::default()),
            clear_cache: None,
            reset_source: None,
            state: Mutex::new(State::default()),
            idle: Condvar::new(),
        }
    }

    pub fn with_options(self, options: UpdateCoordinatorOptions) -> Self {
        *self.options.lock().unwrap() = options;
        self
    }

    /// Настройки проверки применяются к следующему запуску без перезапуска.
    /// Интервалы координатора при этом не сбрасываются.
    pub fn update_check_options(&self, check: UpdateCheckOptions) {
        self.options.lock().unwrap().check = check;
    }

    /// Сброс кэша страниц перед принудительной проверкой.
    pub fn with_clear_cache(mut self, callback: Callback) -> Self {
        self.clear_cache = Some(callback);
        self
    }

    /// Сброс сетевого состояния сайта после прохождения проверки «вы человек?».
    pub fn with_reset_source(mut self, callback: SourceCallback) -> Self {
        self.reset_source = Some(callback);
        self
    }

    pub fn is_running(&self) -> bool {
        self.lock().running.is_some()
    }

    pub fn last_report(&self) -> Option<UpdateRunReport> {
        self.lock().last_report.clone()
    }

    /// Запускает проверку (или присоединяется к идущей) и возвращает её отчёт.
    ///
    /// `progress` вызывается по ходу; у ждущего вызова прогресс не дублируется.
    /// Отмена не даёт исключения: возвращается отчёт с `cancelled = true`,
    /// уже проверенное сохранено.
    pub fn check(
        &self,
        force: bool,
        progress: Option<&mut dyn FnMut(&UpdateProgress)>,
    ) -> UpdateRunReport {
        let mut waited: Option<u64> = None;
        loop {
            let mut state = self.lock();
            if let Some(running) = &state.running {
                let id = running.id;
                waited = Some(id);
                // Дожидаемся конца идущей; после пробуждения решаем, отдавать её
                // отчёт или (если её отменили) запускать свежую.
                state.waiters += 1;
                while state.running.is_some() {
                    state = self
                        .idle
                        .wait(state)
                        .unwrap_or_else(|error| error.into_inner());
                }
                state.waiters -= 1;
                continue;
            }

            if let Some(id) = waited
                && state.last_completed_run == Some(id)
                && state.last_cancelled_run != Some(id)
            {
                // Присоединяться к прерванной (или упавшей) бессмысленно:
                // запускаем свежую ниже.
                return state.last_report.clone().unwrap_or_default();
            }

            if force
                && let Some(last) = &state.last_report
                && !last.cancelled
                && let Some(finished) = state.last_finished
            {
                let since = self.now() - finished;
                if since >= TimeDelta::zero()
                    && since < self.options.lock().unwrap().min_force_interval
                {
                    let mut report = last.clone();
                    report.reused = true;
                    return report;
                }
            }

            state.next_run_id += 1;
            let id = state.next_run_id;
            let cancel = Arc::new(AtomicBool::new(false));
            state.running = Some(Running {
                id,
                cancel: Arc::clone(&cancel),
            });
            drop(state);

            let mut guard = ResetGuard {
                coordinator: self,
                id,
                armed: true,
            };
            let report = self.run_batches(force, &cancel, progress);
            self.finish_run(id, &report);
            guard.armed = false;
            return report;
        }
    }

    /// Останавливает идущую проверку (если она есть). Сделанное сохраняется.
    pub fn cancel(&self) {
        let state = self.lock();
        if let Some(running) = &state.running {
            running.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Пользователь прошёл проверку «вы человек?»: снимаем пометку и паузу.
    pub fn resolve_human_check(&self, source: &str) {
        self.store.set_source_status(source, StoreSourceStatus::Ok);
        {
            let mut state = self.lock();
            state.retry_after.remove(source);
        }
        if let Some(callback) = &self.reset_source {
            callback(source);
        }
    }

    fn now(&self) -> DateTime<FixedOffset> {
        (self.clock)()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }

    #[cfg(test)]
    fn waiters(&self) -> usize {
        self.lock().waiters
    }

    fn finish_run(&self, id: u64, report: &UpdateRunReport) {
        {
            let mut state = self.lock();
            if state
                .running
                .as_ref()
                .is_some_and(|running| running.id == id)
            {
                state.running = None;
            }
            state.last_report = Some(report.clone());
            state.last_completed_run = Some(id);
            if report.cancelled {
                state.last_cancelled_run = Some(id);
            } else {
                state.last_finished = Some(self.now());
                state.last_cancelled_run = None;
            }
        }
        // «Последняя проверка» в окне — время нажатия, даже если все моды отложены.
        if !report.cancelled {
            self.store.set_last_run(self.now());
        }
        self.idle.notify_all();
    }

    fn run_batches(
        &self,
        force: bool,
        cancel: &AtomicBool,
        progress: Option<&mut dyn FnMut(&UpdateProgress)>,
    ) -> UpdateRunReport {
        let options_snapshot = self.options.lock().unwrap().clone();
        if force && let Some(callback) = &self.clear_cache {
            callback();
        }
        let before: HashSet<(String, Option<String>)> =
            UpdateCheckService::pending(&*self.store, false)
                .into_iter()
                .map(|finding| (finding.mod_id, finding.latest_version))
                .collect();

        let mut progress = progress;
        let mut first: Option<UpdateRunReport> = None;
        let mut last: Option<UpdateRunReport> = None;
        let mut merged_sources: Vec<SourceRunReport> = Vec::new();
        let mut requests = 0u32;
        let mut up_to_date = 0usize;

        for batch in 0..options_snapshot.max_batches.max(1) {
            let now = self.now();
            let batch_force = force && batch == 0;
            let mut options = options_snapshot.check.clone().with_force(batch_force);
            // Глубокое чтение ленты — один раз: в следующей пачке те же страницы
            // ничего нового не скажут, а запросы стоят времени.
            options.deep_feed = options.deep_feed && batch == 0;
            let due_before = UpdateCheckPlanner::plan_feeds(
                &UpdateCheckService::tracked(&*self.store),
                &UpdateCheckService::source_statuses(&*self.store),
                now,
                &options,
            )
            .due
            .len();

            let hold = {
                let mut state = self.lock();
                // Принудительная проверка отменяет паузу после сбоя: пользователь настаивает.
                if batch_force {
                    state.retry_after.clear();
                }
                // Пауза не может быть дольше FailureBackoff: иначе часы переводили назад.
                state
                    .retry_after
                    .iter()
                    .filter(|(_, until)| {
                        **until > now && **until - now <= options_snapshot.failure_backoff
                    })
                    .map(|(source, until)| {
                        (
                            source.clone(),
                            format!(
                                "сайт недавно не отвечал, повторим после {}",
                                until.format("%H:%M")
                            ),
                        )
                    })
                    .collect::<BTreeMap<String, String>>()
            };

            // Фабрика вызывается на каждую часть: настройки сайтов могли поменяться.
            let sources = (self.sources)();
            let run = UpdateRun {
                store: &*self.store,
                sources: &sources,
                now,
                options,
                cancel,
                clock: Some(&*self.clock),
                hold_off: hold.clone(),
            };
            let callback = progress
                .as_mut()
                .map(|callback| &mut **callback as &mut dyn FnMut(&UpdateProgress));
            let report = crate::runner::run(&run, callback);
            first.get_or_insert_with(|| report.clone());
            requests += report.requests;
            up_to_date += report.up_to_date;
            for source in &report.sources {
                merge_source_report(&mut merged_sources, source.clone());
            }

            {
                let mut state = self.lock();
                for item in &report.sources {
                    if item.status == SourceStatus::Unavailable && !hold.contains_key(&item.source)
                    {
                        state
                            .retry_after
                            .insert(item.source.clone(), now + options_snapshot.failure_backoff);
                    } else if item.status == SourceStatus::Ok && item.note.is_none() {
                        state.retry_after.remove(&item.source);
                    }
                }
            }

            let unfinished =
                report.stopped_by_time || report.sources.iter().any(|source| source.postponed > 0);
            let final_batch = batch + 1 >= options_snapshot.max_batches.max(1);
            last = Some(report);
            if last.as_ref().is_some_and(|report| report.cancelled) || !unfinished || final_batch {
                break;
            }
            let due_after = UpdateCheckPlanner::plan_feeds(
                &UpdateCheckService::tracked(&*self.store),
                &UpdateCheckService::source_statuses(&*self.store),
                self.now(),
                &options_snapshot.check.clone().with_force(false),
            )
            .due
            .len();
            if due_after >= due_before {
                break; // ничего не продвинулось (сайты отказывают): не крутимся впустую
            }
        }

        let final_report = last.unwrap_or_default();
        let new_findings = final_report
            .findings
            .iter()
            .filter(|finding| {
                !before.contains(&(finding.mod_id.clone(), finding.latest_version.clone()))
            })
            .cloned()
            .collect();
        UpdateRunReport {
            requests,
            up_to_date,
            new_findings,
            fresh_skipped: first.map_or(0, |report| report.fresh_skipped),
            sources: merged_sources,
            ..final_report
        }
    }
}

/// Сайт, упавший в первой пачке, следующая пачка уже не опрашивает (статус сбоя
/// исключает его из плана) — без слияния его сбой и заметка пропали бы из отчёта.
fn merge_source_report(sources: &mut Vec<SourceRunReport>, source: SourceRunReport) {
    let Some(known) = sources
        .iter_mut()
        .find(|known| known.source == source.source)
    else {
        sources.push(source);
        return;
    };
    known.feed_pages_fetched += source.feed_pages_fetched;
    known.cards_fetched += source.cards_fetched;
    // Отложенное считается в каждой пачке заново: последняя пачка знает, что
    // осталось на самом деле, а сумма показала бы давно проверенное.
    known.postponed = source.postponed;
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
    if source.status != SourceStatus::Ok {
        known.status = source.status;
    }
    if source.note.is_some() {
        known.note = source.note;
    }
}

/// Снимает «идёт проверка» даже при панике внутри проверки.
struct ResetGuard<'a> {
    coordinator: &'a UpdateCoordinator,
    id: u64,
    armed: bool,
}

impl Drop for ResetGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        {
            let mut state = self.coordinator.lock();
            if state
                .running
                .as_ref()
                .is_some_and(|running| running.id == self.id)
            {
                state.running = None;
            }
        }
        self.coordinator.idle.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex, mpsc};
    use std::thread;
    use std::time::Duration;

    use chrono::TimeDelta;
    use url::Url;

    use bmm_domain::library::{CheckState, InMemoryLibraryStore, InstalledMod, ModBinding};

    use super::*;
    use crate::runner::UpdateRunReport;

    fn t0() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-13T12:00:00+00:00").unwrap()
    }

    struct Gate {
        open: Mutex<bool>,
        changed: Condvar,
    }

    impl Gate {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                open: Mutex::new(false),
                changed: Condvar::new(),
            })
        }

        fn wait(&self) {
            let mut open = self.open.lock().unwrap();
            while !*open {
                open = self.changed.wait(open).unwrap();
            }
        }

        fn open(&self) {
            *self.open.lock().unwrap() = true;
            self.changed.notify_all();
        }
    }

    struct Site {
        source: &'static str,
        feed: Mutex<Vec<bmm_domain::models::Listing>>,
        feed_fails: Mutex<Option<bmm_sources::sources::SourceError>>,
        gate: Option<Arc<Gate>>,
        started: Mutex<Option<mpsc::Sender<()>>>,
        feed_calls: AtomicUsize,
        card_calls: AtomicUsize,
        card: Mutex<Option<bmm_domain::models::Listing>>,
        on_card: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
        /// Лента на каждой странице, а не только на первой: длинная лента.
        every_page: AtomicBool,
    }

    impl Site {
        fn new() -> Arc<Self> {
            Self::named("x")
        }

        fn named(source: &'static str) -> Arc<Self> {
            Arc::new(Self {
                source,
                feed: Mutex::new(Vec::new()),
                feed_fails: Mutex::new(None),
                gate: None,
                started: Mutex::new(None),
                feed_calls: AtomicUsize::new(0),
                card_calls: AtomicUsize::new(0),
                card: Mutex::new(None),
                on_card: Mutex::new(None),
                every_page: AtomicBool::new(false),
            })
        }

        fn gated() -> (Arc<Self>, Arc<Gate>, mpsc::Receiver<()>) {
            let (sender, receiver) = mpsc::channel();
            let gate = Gate::new();
            let site = Arc::new(Self {
                source: "x",
                feed: Mutex::new(Vec::new()),
                feed_fails: Mutex::new(None),
                gate: Some(Arc::clone(&gate)),
                started: Mutex::new(Some(sender)),
                feed_calls: AtomicUsize::new(0),
                card_calls: AtomicUsize::new(0),
                card: Mutex::new(None),
                on_card: Mutex::new(None),
                every_page: AtomicBool::new(false),
            });
            (site, gate, receiver)
        }

        fn set_feed(&self, feed: Vec<bmm_domain::models::Listing>) {
            *self.feed.lock().unwrap() = feed;
        }

        fn set_feed_failure(&self, error: Option<bmm_sources::sources::SourceError>) {
            *self.feed_fails.lock().unwrap() = error;
        }

        fn set_card(&self, listing: bmm_domain::models::Listing) {
            *self.card.lock().unwrap() = Some(listing);
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
            self.source
        }
        fn display_name(&self) -> &'static str {
            self.source
        }
        fn supports_search(&self) -> bool {
            false
        }
        fn home(&self) -> Url {
            Url::parse("https://x.test/").unwrap()
        }
        fn owns(&self, url: &Url) -> bool {
            url.host_str() == Some("x.test")
        }
        fn is_mod_page(&self, url: &Url) -> bool {
            self.owns(url) && url.path() != "/"
        }
        fn feed_covers(&self, _url: &Url) -> bool {
            true
        }
        fn search(
            &self,
            _query: &str,
            _cancel: &AtomicBool,
        ) -> Result<Vec<bmm_domain::models::Listing>, bmm_sources::sources::SourceError> {
            Ok(Vec::new())
        }
        fn feed_page(
            &self,
            page: u32,
            cancel: &AtomicBool,
        ) -> Result<Vec<bmm_domain::models::Listing>, bmm_sources::sources::SourceError> {
            self.feed_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(sender) = self.started.lock().unwrap().take() {
                let _ = sender.send(());
            }
            if let Some(gate) = &self.gate {
                gate.wait();
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(bmm_sources::sources::SourceError::cancelled());
            }
            if let Some(error) = self.feed_fails.lock().unwrap().clone() {
                return Err(error);
            }
            if page == 1 || self.every_page.load(Ordering::SeqCst) {
                Ok(self.feed.lock().unwrap().clone())
            } else {
                Ok(Vec::new())
            }
        }
        fn listing(
            &self,
            _url: &Url,
            cancel: &AtomicBool,
        ) -> Result<Option<bmm_domain::models::Listing>, bmm_sources::sources::SourceError>
        {
            self.card_calls.fetch_add(1, Ordering::SeqCst);
            if let Some(callback) = self.on_card.lock().unwrap().clone() {
                callback();
            }
            if cancel.load(Ordering::Relaxed) {
                return Err(bmm_sources::sources::SourceError::cancelled());
            }
            Ok(self.card.lock().unwrap().clone())
        }
    }

    fn l(id: &str, version: &str) -> bmm_domain::models::Listing {
        let mut listing = bmm_domain::models::Listing::new(
            "x",
            Url::parse(&format!("https://x.test/{id}")).unwrap(),
            id,
            id,
        );
        listing.mod_version = Some(version.to_string());
        listing
    }

    fn store(count: usize) -> Arc<InMemoryLibraryStore> {
        store_with(&[("x", count)])
    }

    fn store_with(sources: &[(&str, usize)]) -> Arc<InMemoryLibraryStore> {
        let store = Arc::new(InMemoryLibraryStore::new());
        let mut index = 0;
        for (source, count) in sources {
            for _ in 0..*count {
                let id = format!("m{index}");
                index += 1;
                store.upsert_mod(InstalledMod {
                    id: id.clone(),
                    name: id.clone(),
                    file_name: format!("{id}.zip"),
                    version: Some("1.0".to_string()),
                    size_bytes: None,
                    installed_at_utc: t0() - TimeDelta::days(60),
                    enabled: true,
                });
                store
                    .upsert_binding(ModBinding {
                        mod_id: id.clone(),
                        source: source.to_string(),
                        source_id: id.clone(),
                        url: Url::parse(&format!("https://x.test/{id}")).unwrap(),
                        confirmed: true,
                    })
                    .unwrap();
                store.save_check(CheckState {
                    mod_id: id.clone(),
                    source: source.to_string(),
                    last_checked_utc: t0() - TimeDelta::days(2),
                    last_changed_utc: Some(t0() - TimeDelta::days(2)),
                    last_version: Some("1.0".to_string()),
                    last_size: None,
                    miss_count: 0,
                    last_game_version: None,
                    last_title: None,
                });
            }
        }
        store
    }

    fn clock_at(value: DateTime<FixedOffset>) -> Arc<Mutex<DateTime<FixedOffset>>> {
        Arc::new(Mutex::new(value))
    }

    fn clock_of(value: &Arc<Mutex<DateTime<FixedOffset>>>) -> Clock {
        let value = Arc::clone(value);
        Arc::new(move || *value.lock().unwrap())
    }

    fn coordinator(
        store: Arc<InMemoryLibraryStore>,
        site: Arc<Site>,
        clock: Clock,
        options: UpdateCoordinatorOptions,
    ) -> Arc<UpdateCoordinator> {
        coordinator_with_hooks(store, site, clock, options, None, None)
    }

    fn coordinator_many(
        store: Arc<InMemoryLibraryStore>,
        sites: Vec<Arc<Site>>,
        clock: Clock,
        options: UpdateCoordinatorOptions,
    ) -> Arc<UpdateCoordinator> {
        let store: Arc<dyn LibraryStore + Send + Sync> = store;
        let sources: SourcesFactory = Arc::new(move || {
            sites
                .iter()
                .map(|site| Arc::clone(site) as Arc<dyn ModSource>)
                .collect()
        });
        Arc::new(UpdateCoordinator::new(store, sources, clock).with_options(options))
    }

    #[allow(clippy::type_complexity)]
    fn coordinator_with_hooks(
        store: Arc<InMemoryLibraryStore>,
        site: Arc<Site>,
        clock: Clock,
        options: UpdateCoordinatorOptions,
        clear_cache: Option<Callback>,
        reset_source: Option<SourceCallback>,
    ) -> Arc<UpdateCoordinator> {
        let store: Arc<dyn LibraryStore + Send + Sync> = store;
        let sources: SourcesFactory =
            Arc::new(move || vec![Arc::clone(&site) as Arc<dyn ModSource>]);
        let mut coordinator = UpdateCoordinator::new(store, sources, clock).with_options(options);
        if let Some(callback) = clear_cache {
            coordinator = coordinator.with_clear_cache(callback);
        }
        if let Some(callback) = reset_source {
            coordinator = coordinator.with_reset_source(callback);
        }
        Arc::new(coordinator)
    }

    fn spawn_check(
        coordinator: Arc<UpdateCoordinator>,
        force: bool,
    ) -> thread::JoinHandle<UpdateRunReport> {
        thread::spawn(move || coordinator.check(force, None))
    }

    fn wait_started(receiver: &mpsc::Receiver<()>) {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .expect("источник должен начать ленту");
    }

    fn wait_for_waiter(coordinator: &UpdateCoordinator) {
        // По времени, а не по числу уступок: под нагрузкой 10 000 `yield_now` проходили
        // раньше, чем второй поток успевал присоединиться (тест падал через раз).
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            if coordinator.waiters() > 0 {
                return;
            }
            thread::sleep(Duration::from_millis(1));
        }
        panic!("никто не присоединился к идущей проверке");
    }

    #[test]
    fn double_click_joins_the_same_run() {
        let (site, gate, started) = Site::gated();
        site.set_feed(vec![l("m0", "1.0"), l("m1", "1.0"), l("m2", "1.0")]);
        let clock = clock_at(t0());
        let coordinator = coordinator(
            store(3),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
        );

        let first = spawn_check(Arc::clone(&coordinator), false);
        wait_started(&started);
        let second = spawn_check(Arc::clone(&coordinator), false);
        wait_for_waiter(&coordinator);
        assert!(coordinator.is_running());
        gate.open();

        let first = first.join().unwrap();
        let second = second.join().unwrap();
        assert_eq!(site.feed_calls(), 2, "одна проверка, а не две");
        assert_eq!(first.up_to_date, 3);
        assert_eq!(second.up_to_date, 3);
        assert_eq!(first.requests, second.requests);
        assert!(!coordinator.is_running());
    }

    #[test]
    fn cancel_then_new_press_runs_fresh_check() {
        let (site, gate, started) = Site::gated();
        site.set_feed(vec![l("m0", "1.0")]);
        let clock = clock_at(t0());
        let coordinator = coordinator(
            store(1),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
        );

        let first = spawn_check(Arc::clone(&coordinator), false);
        wait_started(&started);
        coordinator.cancel();
        let second = spawn_check(Arc::clone(&coordinator), false);
        wait_for_waiter(&coordinator);
        gate.open();

        let first = first.join().unwrap();
        let second = second.join().unwrap();
        assert!(first.cancelled, "старая прервана");
        assert!(!second.cancelled, "новая доведена до конца");
        assert_eq!(second.up_to_date, 1);
        assert!(!coordinator.is_running());
    }

    #[test]
    fn force_during_normal_check_joins_without_cache_reset() {
        let (site, gate, started) = Site::gated();
        site.set_feed(vec![l("m0", "1.0")]);
        let clock = clock_at(t0());
        let cleared = Arc::new(AtomicUsize::new(0));
        let cleared_for_hook = Arc::clone(&cleared);
        let coordinator = coordinator_with_hooks(
            store(1),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
            Some(Arc::new(move || {
                cleared_for_hook.fetch_add(1, Ordering::SeqCst);
            })),
            None,
        );

        let first = spawn_check(Arc::clone(&coordinator), false);
        wait_started(&started);
        let second = spawn_check(Arc::clone(&coordinator), true);
        wait_for_waiter(&coordinator);
        gate.open();

        let first = first.join().unwrap();
        let second = second.join().unwrap();
        assert_eq!(cleared.load(Ordering::SeqCst), 0);
        assert_eq!(site.feed_calls(), 2);
        assert_eq!(second.up_to_date, 1);
        assert_eq!(first.requests, second.requests);
    }

    #[test]
    fn press_right_after_finish_touches_no_sites() {
        let site = Site::new();
        site.set_feed(vec![l("m0", "1.0"), l("m1", "1.0"), l("m2", "1.0")]);
        let clock = clock_at(t0());
        let library = store(3);
        let coordinator = coordinator(
            Arc::clone(&library),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
        );
        let first = coordinator.check(false, None);
        let calls = site.feed_calls();
        assert_eq!(first.up_to_date, 3);
        assert_eq!(library.last_run(), Some(t0()));

        *clock.lock().unwrap() = t0() + TimeDelta::minutes(10);
        let second = coordinator.check(false, None);
        assert_eq!(site.feed_calls(), calls);
        assert_eq!(second.requests, 0);
        assert_eq!(second.fresh_skipped, 3);
        assert!(second.complete());
        // Сайты не трогали, но «Последняя проверка» — это нажатие.
        assert_eq!(library.last_run(), Some(t0() + TimeDelta::minutes(10)));
    }

    #[test]
    fn force_repeats_return_previous_report_within_a_minute() {
        let site = Site::new();
        site.set_feed(vec![l("m0", "1.0")]);
        let clock = clock_at(t0());
        let cleared = Arc::new(AtomicUsize::new(0));
        let cleared_for_hook = Arc::clone(&cleared);
        let coordinator = coordinator_with_hooks(
            store(1),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
            Some(Arc::new(move || {
                cleared_for_hook.fetch_add(1, Ordering::SeqCst);
            })),
            None,
        );

        coordinator.check(true, None);
        let calls = site.feed_calls();
        assert_eq!(cleared.load(Ordering::SeqCst), 1);

        *clock.lock().unwrap() = t0() + TimeDelta::seconds(20);
        let reused = coordinator.check(true, None);
        assert!(reused.reused);
        assert_eq!(site.feed_calls(), calls);
        assert_eq!(cleared.load(Ordering::SeqCst), 1);

        *clock.lock().unwrap() = t0() + TimeDelta::seconds(90);
        let fresh = coordinator.check(true, None);
        assert!(!fresh.reused);
        assert!(site.feed_calls() > calls);
        assert_eq!(cleared.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn failed_site_rests_for_five_minutes() {
        let site = Site::new();
        site.set_feed_failure(Some(bmm_sources::sources::SourceError::unavailable("502")));
        let clock = clock_at(t0());
        let coordinator = coordinator(
            store(3),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
        );

        let first = coordinator.check(false, None);
        assert_eq!(first.sources[0].status, SourceStatus::Unavailable);
        let calls = site.feed_calls();

        *clock.lock().unwrap() = t0() + TimeDelta::minutes(2);
        let second = coordinator.check(false, None);
        assert_eq!(site.feed_calls(), calls);
        assert!(
            second.sources[0]
                .note
                .as_deref()
                .unwrap()
                .contains("недавно не отвечал")
        );

        *clock.lock().unwrap() = t0() + TimeDelta::minutes(6);
        site.set_feed_failure(None);
        site.set_feed(vec![l("m0", "1.0"), l("m1", "1.0"), l("m2", "1.0")]);
        let third = coordinator.check(false, None);
        assert!(site.feed_calls() > calls, "после паузы пробуем");
        assert_eq!(third.sources[0].status, SourceStatus::Ok);
        assert_eq!(third.up_to_date, 3);
    }

    #[test]
    fn force_clears_failure_backoff() {
        let site = Site::new();
        site.set_feed_failure(Some(bmm_sources::sources::SourceError::unavailable("502")));
        let clock = clock_at(t0());
        let coordinator = coordinator(
            store(3),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
        );
        coordinator.check(false, None);
        let calls = site.feed_calls();
        *clock.lock().unwrap() = t0() + TimeDelta::minutes(2);
        let report = coordinator.check(true, None);
        assert!(site.feed_calls() > calls, "пользователь настоял");
        assert_eq!(report.sources[0].status, SourceStatus::Unavailable);
    }

    #[test]
    fn human_check_skips_until_resolved() {
        let site = Site::new();
        site.set_feed_failure(Some(bmm_sources::sources::SourceError {
            status: SourceStatus::NeedsHumanCheck,
            message: "challenge".to_string(),
        }));
        let clock = clock_at(t0());
        let reset = Arc::new(Mutex::new(Vec::<String>::new()));
        let reset_for_hook = Arc::clone(&reset);
        let coordinator = coordinator_with_hooks(
            store(3),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
            None,
            Some(Arc::new(move |source: &str| {
                reset_for_hook.lock().unwrap().push(source.to_string());
            })),
        );
        coordinator.check(false, None);
        let calls = site.feed_calls();

        *clock.lock().unwrap() = t0() + TimeDelta::hours(8);
        let skipped = coordinator.check(false, None);
        assert_eq!(site.feed_calls(), calls);
        assert_eq!(skipped.sources[0].status, SourceStatus::NeedsHumanCheck);

        coordinator.resolve_human_check("x");
        assert_eq!(*reset.lock().unwrap(), vec!["x".to_string()]);
        site.set_feed_failure(None);
        site.set_feed(vec![l("m0", "1.0"), l("m1", "1.0"), l("m2", "1.0")]);
        let after = coordinator.check(false, None);
        assert!(site.feed_calls() > calls);
        assert_eq!(after.sources[0].status, SourceStatus::Ok);
        assert_eq!(after.up_to_date, 3);
    }

    #[test]
    fn cancellation_returns_report_and_next_press_works() {
        let (site, gate, started) = Site::gated();
        site.set_feed(vec![l("m0", "1.0")]);
        let clock = clock_at(t0());
        let coordinator = coordinator(
            store(1),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
        );
        let first = spawn_check(Arc::clone(&coordinator), false);
        wait_started(&started);
        coordinator.cancel();
        gate.open();
        let report = first.join().unwrap();
        assert!(report.cancelled);
        assert!(!coordinator.is_running());

        let next = coordinator.check(true, None);
        assert!(!next.reused, "отменённая проверка не считается недавней");
        assert!(!next.cancelled);
        assert_eq!(next.up_to_date, 1);
    }

    #[test]
    fn panic_inside_check_does_not_leave_coordinator_busy() {
        let store: Arc<dyn LibraryStore + Send + Sync> = store(1);
        let sources: SourcesFactory = Arc::new(|| panic!("boom"));
        let coordinator = UpdateCoordinator::new(store, sources, clock_of(&clock_at(t0())));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            coordinator.check(false, None)
        }));
        assert!(result.is_err(), "ошибка дошла до вызывающего");
        assert!(!coordinator.is_running(), "флаг сброшен");
    }

    #[test]
    fn library_reads_from_another_thread_are_safe() {
        let store = store(50);
        let site = Site::new();
        site.set_feed(
            (0..50)
                .map(|index| l(&format!("m{index}"), "1.0"))
                .collect(),
        );
        let clock = clock_at(t0());
        let coordinator = coordinator(
            Arc::clone(&store),
            Arc::clone(&site),
            clock_of(&clock),
            UpdateCoordinatorOptions::default(),
        );
        let run = spawn_check(Arc::clone(&coordinator), false);
        let reader_store = Arc::clone(&store);
        let reads = thread::spawn(move || {
            for _ in 0..2000 {
                let _ = reader_store.mods();
                let _ = UpdateCheckService::pending(&*reader_store, false);
            }
        });
        let report = run.join().unwrap();
        reads.join().unwrap();
        assert_eq!(report.up_to_date, 50);
    }

    #[test]
    fn one_press_finishes_work_in_several_batches() {
        let store = store(4);
        let site = Site::new();
        for index in 0..4 {
            site.set_card(l(&format!("m{index}"), "1.0"));
        }
        let tick = clock_at(t0());
        let tick_for_card = Arc::clone(&tick);
        site.on_card(Arc::new(move || {
            *tick_for_card.lock().unwrap() += TimeDelta::minutes(4);
        }));
        let options = UpdateCoordinatorOptions {
            check: UpdateCheckOptions {
                max_duration: Duration::from_secs(10 * 60),
                ..UpdateCheckOptions::default()
            },
            ..UpdateCoordinatorOptions::default()
        };
        let coordinator = coordinator(store, Arc::clone(&site), clock_of(&tick), options);
        let report = coordinator.check(false, None);
        assert!(!report.stopped_by_time, "вторая пачка довела работу");
        let checked = report.findings.len();
        let _ = checked;
        assert_eq!(site.card_calls(), 4);
        assert_eq!(report.requests, 6, "две ленты и четыре карточки");
    }

    #[test]
    fn deep_feed_is_read_only_in_the_first_batch() {
        // Датированная лента, не дошедшая до прошлой проверки: первая пачка читает её
        // вглубь (8 страниц на 8 модов), следующие — только по плану (по 3), а не снова
        // вглубь на оставшиеся моды.
        let store = store(8);
        let site = Site::new();
        let mut fresh = l("other", "2.0");
        fresh.updated_at = Some(t0());
        site.set_feed(vec![fresh]);
        site.every_page.store(true, Ordering::SeqCst);
        site.set_card(l("m0", "1.0"));
        let tick = clock_at(t0());
        let tick_for_card = Arc::clone(&tick);
        site.on_card(Arc::new(move || {
            *tick_for_card.lock().unwrap() += TimeDelta::minutes(4);
        }));
        let options = UpdateCoordinatorOptions {
            check: UpdateCheckOptions {
                max_duration: Duration::from_secs(10 * 60),
                ..UpdateCheckOptions::default()
            },
            ..UpdateCoordinatorOptions::default()
        };
        let coordinator = coordinator(store, Arc::clone(&site), clock_of(&tick), options);
        let report = coordinator.check(false, None);
        assert!(!report.stopped_by_time);
        assert_eq!(site.card_calls(), 8);
        // Три пачки: 8 вглубь, затем по плану 3 и 3 (раньше вторая читала вглубь 5).
        assert_eq!(site.feed_calls(), 8 + 3 + 3);
    }

    #[test]
    fn batches_keep_the_site_that_finished_in_the_first_one() {
        // У сайта a пустая лента: он успевает всё проверить в первой пачке и во вторую
        // уже не попадает, а его «возможно, изменилась вёрстка» должно остаться в итоге.
        let quiet = Site::named("a");
        quiet.set_card(l("m0", "1.0"));
        let busy = Site::named("b");
        busy.set_card(l("m1", "1.0"));
        let tick = clock_at(t0());
        let tick_for_card = Arc::clone(&tick);
        busy.on_card(Arc::new(move || {
            *tick_for_card.lock().unwrap() += TimeDelta::minutes(4);
        }));
        let options = UpdateCoordinatorOptions {
            check: UpdateCheckOptions {
                max_duration: Duration::from_secs(10 * 60),
                ..UpdateCheckOptions::default()
            },
            ..UpdateCoordinatorOptions::default()
        };
        let coordinator = coordinator_many(
            store_with(&[("a", 1), ("b", 4)]),
            vec![Arc::clone(&quiet), Arc::clone(&busy)],
            clock_of(&tick),
            options,
        );

        let report = coordinator.check(false, None);

        let a = report
            .sources
            .iter()
            .find(|source| source.source == "a")
            .expect("отчёт сайта a не должен исчезнуть из итога");
        assert_eq!(a.status, SourceStatus::Ok);
        assert!(a.layout_changed);
        assert!(
            a.note
                .as_deref()
                .is_some_and(|note| note.contains("вёрстка")),
            "{:?}",
            a.note
        );
        let b = report
            .sources
            .iter()
            .find(|source| source.source == "b")
            .expect("отчёт сайта b");
        assert_eq!(b.cards_fetched, 4);
        assert_eq!(b.postponed, 0, "вторая пачка всё доверила");
        assert_eq!(b.feed_pages_fetched, 2);
        assert_eq!(report.requests, 8, "{:?}", report.sources);
        assert!(!report.stopped_by_time);
        assert!(!report.complete(), "заметка сайта a видна в итоге");
    }
}

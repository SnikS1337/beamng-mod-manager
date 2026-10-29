//! Общий поиск: параллельный опрос сайтов, слияние дублей и карточки.

#[cfg(test)]
mod quality;
mod query;
pub mod relevance;

use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use bmm_domain::matches::{ManualMatch, MatchDecisions};
use bmm_domain::matching::{DuplicateMatcher, MatchVerdict};
use bmm_domain::models::{Listing, PriceStatus};
use bmm_domain::text::{ModKind, TitleNormalizer, TitleParser};
use bmm_domain::versioning::{VersionComparer, VersionOrder};

use crate::sources::{ModSource, SourceStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceSummary {
    Unknown,
    Free,
    Paid,
    Mixed,
}

#[derive(Debug, Clone)]
pub struct ModCard {
    pub id: String,
    pub title: String,
    pub listings: Vec<Listing>,
    pub price: PriceSummary,
    pub possible_same_as: Vec<String>,
    pub relevance: f64,
    /// Машина, карта или дополнение: по разделу сайта, иначе по рубрике в названии.
    pub kind: Option<ModKind>,
}

#[derive(Debug, Clone)]
pub struct SourceReport {
    pub source_id: String,
    pub display_name: String,
    pub status: SourceStatus,
    pub count: usize,
    pub note: Option<String>,
    pub pending: bool,
}

#[derive(Debug, Clone)]
pub struct SearchResponse {
    pub cards: Vec<ModCard>,
    pub sources: Vec<SourceReport>,
    pub listings: Vec<Listing>,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub per_source_timeout: Duration,
    pub max_query_length: usize,
    pub max_per_source: usize,
    pub min_query_length: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            per_source_timeout: Duration::from_secs(20),
            max_query_length: 100,
            max_per_source: 30,
            min_query_length: 2,
        }
    }
}

type StatusFn = Arc<dyn Fn(&str) -> SourceStatus + Send + Sync>;

pub struct SearchService {
    sources: Vec<Arc<dyn ModSource>>,
    status_of: StatusFn,
    options: SearchOptions,
    decisions: Option<Arc<dyn MatchDecisions + Send + Sync>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupOrder {
    Relevance,
    Site,
}

#[derive(Clone)]
enum RunKind {
    Search(Vec<String>),
    Browse {
        kind: ModKind,
        first_page: u32,
        pages: u32,
    },
}

impl RunKind {
    fn order(&self) -> GroupOrder {
        match self {
            RunKind::Search(_) => GroupOrder::Relevance,
            RunKind::Browse { .. } => GroupOrder::Site,
        }
    }
}

#[derive(Clone)]
struct SourceRun {
    report: SourceReport,
    listings: Vec<Listing>,
}

impl SearchService {
    pub fn new(
        sources: Vec<Arc<dyn ModSource>>,
        status_of: StatusFn,
        options: SearchOptions,
        decisions: Option<Arc<dyn MatchDecisions + Send + Sync>>,
    ) -> Self {
        Self {
            sources,
            status_of,
            options,
            decisions,
        }
    }

    pub fn sources(&self) -> &[Arc<dyn ModSource>] {
        &self.sources
    }

    fn decisions(&self) -> Option<&dyn MatchDecisions> {
        self.decisions
            .as_ref()
            .map(|decisions| decisions.as_ref() as &dyn MatchDecisions)
    }

    pub fn options(&self) -> &SearchOptions {
        &self.options
    }

    pub fn normalize(&self, query: &str) -> String {
        let trimmed = query.trim();
        if trimmed.chars().count() > self.options.max_query_length {
            trimmed
                .chars()
                .take(self.options.max_query_length)
                .collect::<String>()
                .trim_end()
                .to_string()
        } else {
            trimmed.to_string()
        }
    }

    pub fn min_query_length(&self) -> usize {
        self.options.min_query_length
    }

    pub fn regroup(&self, previous: &SearchResponse, query: &str) -> SearchResponse {
        let listings = if !previous.listings.is_empty() {
            previous.listings.clone()
        } else {
            previous
                .cards
                .iter()
                .flat_map(|card| card.listings.clone())
                .collect()
        };
        SearchResponse {
            cards: self.group(&listings, query.trim()),
            sources: previous.sources.clone(),
            listings,
        }
    }

    /// Пересобрать карточки в порядке сайтов (раздел карт, машин, дополнений).
    pub fn regroup_site_order(&self, previous: &SearchResponse) -> SearchResponse {
        let listings = if !previous.listings.is_empty() {
            previous.listings.clone()
        } else {
            previous
                .cards
                .iter()
                .flat_map(|card| card.listings.clone())
                .collect()
        };
        SearchResponse {
            cards: self.group_site_order(&listings),
            sources: previous.sources.clone(),
            listings,
        }
    }

    pub fn search(
        &self,
        raw_query: &str,
        cancel: &AtomicBool,
        progress: Option<&mut dyn FnMut(&SearchResponse)>,
    ) -> SearchResponse {
        let query = self.normalize(raw_query);
        if query.chars().count() < self.options.min_query_length {
            return SearchResponse {
                cards: Vec::new(),
                sources: Vec::new(),
                listings: Vec::new(),
            };
        }
        let variants = query::variants(&query);
        self.run_sources(&RunKind::Search(variants), &query, cancel, progress)
    }

    /// Открыть раздел сайта без текста: `first_page` и `pages` — диапазон страниц.
    pub fn browse(
        &self,
        kind: ModKind,
        first_page: u32,
        pages: u32,
        cancel: &AtomicBool,
        progress: Option<&mut dyn FnMut(&SearchResponse)>,
    ) -> SearchResponse {
        self.run_sources(
            &RunKind::Browse {
                kind,
                first_page: first_page.max(1),
                pages: pages.max(1),
            },
            "",
            cancel,
            progress,
        )
    }

    fn run_sources(
        &self,
        run: &RunKind,
        query: &str,
        cancel: &AtomicBool,
        mut progress: Option<&mut dyn FnMut(&SearchResponse)>,
    ) -> SearchResponse {
        let order = run.order();
        let runner = Runner {
            status_of: Arc::clone(&self.status_of),
            options: self.options.clone(),
        };
        let total = self.sources.len();
        let mut slots: Vec<Option<SourceRun>> = (0..total).map(|_| None).collect();
        // Копия флага для рабочих потоков: `&AtomicBool` нельзя увезти в `'static`-поток,
        // а внешний флаг координатор опрашивает сам и переводит эту копию.
        let shared_cancel = Arc::new(AtomicBool::new(cancel.load(Ordering::Relaxed)));
        if shared_cancel.load(Ordering::Relaxed) {
            for (index, source) in self.sources.iter().enumerate() {
                slots[index] = Some(runner.stopped(
                    source.as_ref(),
                    SourceStatus::Unavailable,
                    "поиск отменён",
                ));
            }
            return self.finish(slots, query, order);
        }

        let (sender, receiver) = mpsc::channel::<(usize, SourceRun)>();
        let deadline = Instant::now() + self.options.per_source_timeout;
        let mut done = 0usize;
        for (index, source) in self.sources.iter().enumerate() {
            let sender = sender.clone();
            let worker_source = Arc::clone(source);
            let worker_run = run.clone();
            let cancel = Arc::clone(&shared_cancel);
            let worker_runner = runner.clone();
            let thread_name = format!("bmm-search-{}", source.id());
            let spawned = std::thread::Builder::new()
                .name(thread_name)
                .spawn(move || {
                    // Паника одного сайта (кривая разметка, ошибка в адаптере) не должна
                    // ронять весь поиск: остальные источники и их результаты остаются.
                    let run = match catch_unwind(AssertUnwindSafe(|| {
                        worker_runner.run(worker_source.as_ref(), &worker_run, &cancel)
                    })) {
                        Ok(run) => run,
                        Err(payload) => {
                            worker_runner.panicked(worker_source.as_ref(), payload.as_ref())
                        }
                    };
                    let _ = sender.send((index, run));
                });
            if spawned.is_err() {
                slots[index] = Some(runner.stopped(
                    source.as_ref(),
                    SourceStatus::Unavailable,
                    "не удалось запустить поиск",
                ));
                done += 1;
            }
        }
        drop(sender);

        let mut cancelled = false;
        while done < total {
            if cancel.load(Ordering::Relaxed) {
                shared_cancel.store(true, Ordering::Relaxed);
                cancelled = true;
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                // Сначала забираем ответы, которые уже лежат в канале: быстрый сайт
                // не должен считаться опоздавшим из-за планировщика.
                while let Ok((index, run)) = receiver.try_recv() {
                    if slots[index].is_none() {
                        slots[index] = Some(run);
                        done += 1;
                    }
                }
                if done < total {
                    for (index, slot) in slots.iter_mut().enumerate() {
                        if slot.is_none() {
                            *slot = Some(runner.stopped(
                                self.sources[index].as_ref(),
                                SourceStatus::Unavailable,
                                "сайт не ответил вовремя",
                            ));
                        }
                    }
                }
                // Как связанный CancellationToken в legacy: зависший сайт должен
                // по возможности свернуть работу, а не висеть после возврата поиска.
                shared_cancel.store(true, Ordering::Relaxed);
                break;
            }
            let wait = deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(25));
            match receiver.recv_timeout(wait) {
                Ok((index, run)) => {
                    if slots[index].is_none() {
                        slots[index] = Some(run);
                        done += 1;
                        if done < total
                            && let Some(callback) = progress.as_mut()
                        {
                            (**callback)(&self.partial(&slots, query, order));
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        if cancelled {
            while let Ok((index, run)) = receiver.try_recv() {
                if slots[index].is_none() {
                    slots[index] = Some(run);
                }
            }
            for (index, slot) in slots.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(runner.stopped(
                        self.sources[index].as_ref(),
                        SourceStatus::Unavailable,
                        "поиск отменён",
                    ));
                }
            }
        } else if slots.iter().any(Option::is_none) {
            // Канал закрылся, а результат так и не пришёл (например, поток не смог
            // отправить его). Не оставляем источник вечно «в поиске».
            for (index, slot) in slots.iter_mut().enumerate() {
                if slot.is_none() {
                    *slot = Some(runner.stopped(
                        self.sources[index].as_ref(),
                        SourceStatus::Unavailable,
                        "источник не ответил",
                    ));
                }
            }
        }
        self.finish(slots, query, order)
    }

    /// Повторить один сайт после сбоя, сохранив остальную выдачу.
    /// `self.sources` должен содержать только этот сайт (команда собирает такой сервис).
    pub fn retry_source(
        &self,
        previous: &SearchResponse,
        source_id: &str,
        query: &str,
        cancel: &AtomicBool,
    ) -> Option<SearchResponse> {
        let source = self
            .sources
            .iter()
            .find(|source| source.id().eq_ignore_ascii_case(source_id))?;
        let query = self.normalize(query);
        let variants = query::variants(&query);
        let run_kind = RunKind::Search(variants);
        let runner = Runner {
            status_of: Arc::clone(&self.status_of),
            options: self.options.clone(),
        };
        let run = match catch_unwind(AssertUnwindSafe(|| {
            runner.run(source.as_ref(), &run_kind, cancel)
        })) {
            Ok(run) => run,
            Err(payload) => runner.panicked(source.as_ref(), payload.as_ref()),
        };
        let mut listings = previous.listings.clone();
        listings.retain(|listing| !listing.source.eq_ignore_ascii_case(source_id));
        listings.extend(run.listings);
        let mut sources = previous.sources.clone();
        if let Some(report) = sources
            .iter_mut()
            .find(|report| report.source_id.eq_ignore_ascii_case(source_id))
        {
            *report = run.report;
        } else {
            sources.push(run.report);
        }
        Some(SearchResponse {
            cards: self.group(&listings, &query),
            sources,
            listings,
        })
    }

    fn finish(
        &self,
        slots: Vec<Option<SourceRun>>,
        query: &str,
        order: GroupOrder,
    ) -> SearchResponse {
        let reports = slots
            .iter()
            .flatten()
            .map(|run| run.report.clone())
            .collect();
        let listings: Vec<Listing> = slots
            .iter()
            .flatten()
            .flat_map(|run| run.listings.clone())
            .collect();
        SearchResponse {
            cards: self.group_with(&listings, query, order),
            sources: reports,
            listings,
        }
    }

    fn partial(
        &self,
        slots: &[Option<SourceRun>],
        query: &str,
        order: GroupOrder,
    ) -> SearchResponse {
        let mut reports = Vec::new();
        let mut listings = Vec::new();
        for (index, slot) in slots.iter().enumerate() {
            match slot {
                Some(run) => {
                    reports.push(run.report.clone());
                    listings.extend(run.listings.clone());
                }
                None => {
                    let source = &self.sources[index];
                    reports.push(SourceReport {
                        source_id: source.id().to_string(),
                        display_name: source.display_name().to_string(),
                        status: SourceStatus::Ok,
                        count: 0,
                        note: Some("ищем…".to_string()),
                        pending: true,
                    });
                }
            }
        }
        SearchResponse {
            cards: self.group_with(&listings, query, order),
            sources: reports,
            listings,
        }
    }

    pub fn group(&self, listings: &[Listing], query: &str) -> Vec<ModCard> {
        self.group_with(listings, query, GroupOrder::Relevance)
    }

    /// Карточки в порядке сайтов: для разделов, где сайт уже отсортировал по новизне.
    pub fn group_site_order(&self, listings: &[Listing]) -> Vec<ModCard> {
        self.group_with(listings, "", GroupOrder::Site)
    }

    fn group_with(&self, listings: &[Listing], query: &str, order: GroupOrder) -> Vec<ModCard> {
        let mut unique = Vec::new();
        let mut seen = HashSet::new();
        for listing in listings {
            if seen.insert((listing.source.clone(), listing.source_id.clone())) {
                unique.push(listing.clone());
            }
        }

        let mut clusters: Vec<Vec<usize>> = Vec::new();
        for (index, listing) in unique.iter().enumerate() {
            let mut best: Option<(usize, f64, bool)> = None;
            for (cluster_index, cluster) in clusters.iter().enumerate() {
                if cluster
                    .iter()
                    .any(|member| unique[*member].source == listing.source)
                {
                    continue;
                }
                if let Some(decisions) = &self.decisions
                    && cluster.iter().any(|member| {
                        decisions.get(&unique[*member], listing) == Some(ManualMatch::Different)
                    })
                {
                    continue;
                }
                let results: Vec<_> = cluster
                    .iter()
                    .map(|member| {
                        DuplicateMatcher::compare(&unique[*member], listing, self.decisions())
                    })
                    .collect();
                let forced = self.decisions.as_ref().is_some_and(|decisions| {
                    cluster.iter().any(|member| {
                        decisions.get(&unique[*member], listing) == Some(ManualMatch::Same)
                    })
                });
                if !forced
                    && results
                        .iter()
                        .any(|result| result.verdict != MatchVerdict::Same)
                {
                    continue;
                }
                let score =
                    results.iter().map(|result| result.score).sum::<f64>() / results.len() as f64;
                let better = match best {
                    None => true,
                    Some((_, best_score, best_forced)) => {
                        (forced && !best_forced) || (forced == best_forced && score > best_score)
                    }
                };
                if better {
                    best = Some((cluster_index, score, forced));
                }
            }
            match best {
                Some((cluster_index, _, _)) => clusters[cluster_index].push(index),
                None => clusters.push(vec![index]),
            }
        }

        let ids: Vec<String> = clusters
            .iter()
            .map(|cluster| card_id(cluster, &unique))
            .collect();
        let mut cards = Vec::new();
        for (cluster_index, cluster) in clusters.iter().enumerate() {
            let first = &unique[cluster[0]];
            let mut title = TitleParser::parse(&first.title).name;
            if title.is_empty() {
                title = first.title.clone();
            }
            let relevance = cluster
                .iter()
                .map(|&index| TitleNormalizer::similarity(query, &unique[index].title))
                .fold(0.0_f64, f64::max)
                + 0.05 * cluster.len().min(3) as f64;
            let kind = cluster
                .iter()
                .find_map(|&index| TitleParser::parse(&unique[index].title).kind)
                .or_else(|| {
                    cluster.iter().find_map(|&index| {
                        unique[index]
                            .category
                            .as_deref()
                            .and_then(ModKind::from_category)
                    })
                });
            cards.push(ModCard {
                kind,
                id: ids[cluster_index].clone(),
                title,
                listings: cluster.iter().map(|&index| unique[index].clone()).collect(),
                price: summarize(cluster.iter().map(|&index| unique[index].price)),
                possible_same_as: Vec::new(),
                relevance,
            });
        }
        if order == GroupOrder::Relevance {
            cards.sort_by(|left, right| {
                right
                    .relevance
                    .partial_cmp(&left.relevance)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
            });
        }
        self.ask_about_doubtful(&mut cards);
        cards
    }

    /// Вопросы «Возможно, тот же мод». Ручных кликов должно быть как можно меньше, поэтому:
    /// пару спрашиваем один раз (на карточке, что выше в выдаче), на карточке не больше
    /// одного вопроса (самая похожая пара), а пары, которые всё равно нельзя склеить
    /// (в обеих карточках есть один и тот же сайт), не спрашиваем вовсе.
    fn ask_about_doubtful(&self, cards: &mut [ModCard]) {
        let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
        for left in 0..cards.len() {
            for right in left + 1..cards.len() {
                let (a, b) = (&cards[left].listings, &cards[right].listings);
                let shares_site = a.iter().any(|x| b.iter().any(|y| x.source == y.source));
                if shares_site {
                    continue;
                }
                let mut best: Option<f64> = None;
                let mut refused = false;
                for x in a {
                    for y in b {
                        let result = DuplicateMatcher::compare(x, y, self.decisions());
                        if let Some(decisions) = &self.decisions
                            && decisions.get(x, y) == Some(ManualMatch::Different)
                        {
                            refused = true;
                        }
                        if result.verdict == MatchVerdict::Maybe {
                            best = Some(best.map_or(result.score, |score| score.max(result.score)));
                        }
                    }
                }
                if let (Some(score), false) = (best, refused) {
                    pairs.push((left, right, score));
                }
            }
        }
        pairs.sort_by(|x, y| {
            y.2.partial_cmp(&x.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| (x.0, x.1).cmp(&(y.0, y.1)))
        });
        let mut busy = vec![false; cards.len()];
        for (left, right, _) in pairs {
            if busy[left] || busy[right] {
                continue;
            }
            busy[left] = true;
            busy[right] = true;
            let other = cards[right].id.clone();
            cards[left].possible_same_as.push(other);
        }
    }
}

/// Всё, что нужно одному рабочему потоку поиска. `SearchService` целиком в поток
/// не уезжает: его поля клонируются, чтобы поиск можно было бросить по таймауту.
#[derive(Clone)]
struct Runner {
    status_of: StatusFn,
    options: SearchOptions,
}

impl Runner {
    fn run_one(
        &self,
        source: &dyn ModSource,
        queries: &[String],
        cancel: &AtomicBool,
    ) -> SourceRun {
        if !source.supports_search() {
            return self.stopped_with_note(
                source,
                SourceStatus::Ok,
                "поиск на этом сайте пока не поддерживается",
            );
        }
        let status = (self.status_of)(source.id());
        if status != SourceStatus::Ok {
            return self.stopped_with_note(
                source,
                status,
                if status == SourceStatus::NeedsHumanCheck {
                    "нужно пройти проверку на сайте"
                } else {
                    "сайт недоступен"
                },
            );
        }
        let mut last_error = None;
        for query in queries {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            match source.search(query, cancel) {
                Ok(found) if !found.is_empty() => {
                    let listings: Vec<Listing> = newest_only(found)
                        .into_iter()
                        .take(self.options.max_per_source)
                        .collect();
                    return SourceRun {
                        report: SourceReport {
                            source_id: source.id().to_string(),
                            display_name: source.display_name().to_string(),
                            status: SourceStatus::Ok,
                            count: listings.len(),
                            note: None,
                            pending: false,
                        },
                        listings,
                    };
                }
                Ok(_) => {}
                Err(error) => {
                    last_error = Some(error);
                    break;
                }
            }
        }
        match last_error {
            Some(error) => SourceRun {
                report: SourceReport {
                    source_id: source.id().to_string(),
                    display_name: source.display_name().to_string(),
                    status: error.status,
                    count: 0,
                    note: Some(error.message),
                    pending: false,
                },
                listings: Vec::new(),
            },
            None => SourceRun {
                report: SourceReport {
                    source_id: source.id().to_string(),
                    display_name: source.display_name().to_string(),
                    status: SourceStatus::Ok,
                    count: 0,
                    note: None,
                    pending: false,
                },
                listings: Vec::new(),
            },
        }
    }

    fn run(&self, source: &dyn ModSource, run: &RunKind, cancel: &AtomicBool) -> SourceRun {
        match run {
            RunKind::Search(queries) => self.run_one(source, queries, cancel),
            RunKind::Browse {
                kind,
                first_page,
                pages,
            } => self.run_browse(source, *kind, *first_page, *pages, cancel),
        }
    }

    fn run_browse(
        &self,
        source: &dyn ModSource,
        kind: ModKind,
        first_page: u32,
        pages: u32,
        cancel: &AtomicBool,
    ) -> SourceRun {
        if !source.supports_browse(kind) {
            return self.stopped_with_note(source, SourceStatus::Ok, "на сайте нет такого раздела");
        }
        let status = (self.status_of)(source.id());
        if status != SourceStatus::Ok {
            return self.stopped_with_note(
                source,
                status,
                if status == SourceStatus::NeedsHumanCheck {
                    "нужно пройти проверку на сайте"
                } else {
                    "сайт недоступен"
                },
            );
        }
        let mut found = Vec::new();
        let mut last_error = None;
        for page in first_page..first_page.saturating_add(pages) {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            match source.browse(kind, page, cancel) {
                Ok(mut items) => found.append(&mut items),
                Err(error) => {
                    last_error = Some(error);
                    break;
                }
            }
        }
        if found.is_empty() {
            return match last_error {
                Some(error) => SourceRun {
                    report: SourceReport {
                        source_id: source.id().to_string(),
                        display_name: source.display_name().to_string(),
                        status: error.status,
                        count: 0,
                        note: Some(error.message),
                        pending: false,
                    },
                    listings: Vec::new(),
                },
                None => self.stopped_with_note(source, SourceStatus::Ok, "пока ничего нет"),
            };
        }
        let listings: Vec<Listing> = newest_only(found)
            .into_iter()
            .take(self.options.max_per_source)
            .collect();
        SourceRun {
            report: SourceReport {
                source_id: source.id().to_string(),
                display_name: source.display_name().to_string(),
                status: SourceStatus::Ok,
                count: listings.len(),
                note: last_error
                    .map(|error| format!("часть страниц не загрузилась: {}", error.message)),
                pending: false,
            },
            listings,
        }
    }

    fn stopped(&self, source: &dyn ModSource, status: SourceStatus, note: &str) -> SourceRun {
        self.stopped_with_note(source, status, note)
    }

    fn stopped_with_note(
        &self,
        source: &dyn ModSource,
        status: SourceStatus,
        note: &str,
    ) -> SourceRun {
        SourceRun {
            report: SourceReport {
                source_id: source.id().to_string(),
                display_name: source.display_name().to_string(),
                status,
                count: 0,
                note: Some(note.to_string()),
                pending: false,
            },
            listings: Vec::new(),
        }
    }

    fn panicked(&self, source: &dyn ModSource, payload: &(dyn Any + Send)) -> SourceRun {
        let message = payload
            .downcast_ref::<&str>()
            .map(|text| (*text).to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "внутренняя ошибка".to_string());
        SourceRun {
            report: SourceReport {
                source_id: source.id().to_string(),
                display_name: source.display_name().to_string(),
                status: SourceStatus::Unavailable,
                count: 0,
                note: Some(format!("источник упал: {message}")),
                pending: false,
            },
            listings: Vec::new(),
        }
    }
}

pub fn newest_only(found: Vec<Listing>) -> Vec<Listing> {
    let mut result: Vec<Listing> = Vec::with_capacity(found.len());
    let mut at: HashMap<(String, String, String), usize> = HashMap::new();
    for listing in found {
        let name = TitleNormalizer::normalize(&TitleParser::parse(&listing.title).name);
        if name.chars().count() < 3 {
            result.push(listing);
            continue;
        }
        let key = (
            listing.source.to_lowercase(),
            name,
            listing
                .author
                .clone()
                .unwrap_or_default()
                .trim()
                .to_lowercase(),
        );
        if let Some(&index) = at.get(&key) {
            match order(&result[index], &listing) {
                VersionOrder::Newer => result[index] = listing,
                VersionOrder::Older => {}
                _ => result.push(listing),
            }
        } else {
            at.insert(key, result.len());
            result.push(listing);
        }
    }
    result
}

fn order(kept: &Listing, other: &Listing) -> VersionOrder {
    let version =
        VersionComparer::compare(version_of(kept).as_deref(), version_of(other).as_deref());
    if version == VersionOrder::Same {
        VersionComparer::compare(game_of(kept).as_deref(), game_of(other).as_deref())
    } else {
        version
    }
}

fn version_of(listing: &Listing) -> Option<String> {
    listing
        .mod_version
        .clone()
        .or_else(|| TitleParser::parse(&listing.title).mod_version)
}

fn game_of(listing: &Listing) -> Option<String> {
    listing
        .game_version
        .clone()
        .or_else(|| TitleParser::parse(&listing.title).game_version)
}

fn summarize(prices: impl Iterator<Item = PriceStatus>) -> PriceSummary {
    let mut free = false;
    let mut paid = false;
    for price in prices {
        match price {
            PriceStatus::Free => free = true,
            PriceStatus::Paid => paid = true,
            PriceStatus::Unknown => {}
        }
    }
    match (free, paid) {
        (true, true) => PriceSummary::Mixed,
        (true, false) => PriceSummary::Free,
        (false, true) => PriceSummary::Paid,
        (false, false) => PriceSummary::Unknown,
    }
}

fn card_id(cluster: &[usize], listings: &[Listing]) -> String {
    let mut parts: Vec<String> = cluster
        .iter()
        .map(|&index| format!("{}:{}", listings[index].source, listings[index].source_id))
        .collect();
    parts.sort();
    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use url::Url;

    use crate::sources::{ModSource, SourceError};

    fn listing(source: &str, id: &str, title: &str, version: Option<&str>) -> Listing {
        let mut listing = Listing::new(
            source,
            Url::parse(&format!("https://example.org/{source}/{id}")).unwrap(),
            id,
            title,
        );
        listing.mod_version = version.map(str::to_string);
        listing
    }

    fn service() -> SearchService {
        SearchService::new(
            Vec::new(),
            Arc::new(|_| SourceStatus::Ok),
            SearchOptions::default(),
            None,
        )
    }

    struct FakeSource {
        id: &'static str,
        title: &'static str,
    }

    impl ModSource for FakeSource {
        fn id(&self) -> &'static str {
            self.id
        }

        fn display_name(&self) -> &'static str {
            self.id
        }

        fn supports_search(&self) -> bool {
            true
        }

        fn home(&self) -> Url {
            Url::parse(&format!("https://{}.example.org/", self.id)).unwrap()
        }

        fn owns(&self, _url: &Url) -> bool {
            true
        }

        fn is_mod_page(&self, _url: &Url) -> bool {
            true
        }

        fn feed_covers(&self, _url: &Url) -> bool {
            true
        }

        fn search(&self, _query: &str, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            let mut item = Listing::new(
                self.id,
                Url::parse(&format!("https://{}.example.org/mod/1", self.id)).unwrap(),
                "1",
                self.title,
            );
            item.author = Some("Author".to_string());
            item.size_bytes = Some(100);
            Ok(vec![item])
        }

        fn feed_page(&self, _page: u32, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }

        fn listing(
            &self,
            _url: &Url,
            _cancel: &AtomicBool,
        ) -> Result<Option<Listing>, SourceError> {
            Ok(None)
        }
    }

    struct QuerySource {
        calls: Arc<AtomicUsize>,
        answer: &'static str,
    }

    impl ModSource for QuerySource {
        fn id(&self) -> &'static str {
            "query"
        }
        fn display_name(&self) -> &'static str {
            "Query"
        }
        fn supports_search(&self) -> bool {
            true
        }
        fn home(&self) -> Url {
            Url::parse("https://query.example.org/").unwrap()
        }
        fn owns(&self, _url: &Url) -> bool {
            true
        }
        fn is_mod_page(&self, _url: &Url) -> bool {
            true
        }
        fn feed_covers(&self, _url: &Url) -> bool {
            true
        }
        fn search(&self, query: &str, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if query.to_lowercase().contains(self.answer) {
                let mut item = listing("query", "1", "LADA Niva", Some("1.0"));
                item.author = Some("Author".to_string());
                Ok(vec![item])
            } else {
                Ok(Vec::new())
            }
        }
        fn feed_page(&self, _page: u32, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }
        fn listing(
            &self,
            _url: &Url,
            _cancel: &AtomicBool,
        ) -> Result<Option<Listing>, SourceError> {
            Ok(None)
        }
    }

    #[test]
    fn retries_with_transliteration_only_when_query_is_empty() {
        let calls = Arc::new(AtomicUsize::new(0));
        let source = QuerySource {
            calls: calls.clone(),
            answer: "lada",
        };
        let service = SearchService::new(
            vec![Arc::new(source)],
            Arc::new(|_| SourceStatus::Ok),
            SearchOptions::default(),
            None,
        );
        let response = service.search("лада", &AtomicBool::new(false), None);
        assert_eq!(response.cards.len(), 1, "нашлось по транслиту");
        assert!(calls.load(Ordering::SeqCst) >= 2);

        let calls = Arc::new(AtomicUsize::new(0));
        let source = QuerySource {
            calls: calls.clone(),
            answer: "lada",
        };
        let service = SearchService::new(
            vec![Arc::new(source)],
            Arc::new(|_| SourceStatus::Ok),
            SearchOptions::default(),
            None,
        );
        let response = service.search("lada", &AtomicBool::new(false), None);
        assert_eq!(response.cards.len(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1, "без лишнего запроса");
    }

    #[test]
    fn retry_replaces_only_target_source_and_updates_report() {
        let retried = ScriptedSource::new("modland", |_query, _cancel| {
            let mut item = listing("modland", "fresh", "BMW M2 G87", Some("2.0"));
            item.author = Some("Author".to_string());
            item.size_bytes = Some(100);
            Ok(vec![item])
        });
        let service = service_with(vec![retried], SearchOptions::default());
        let mut kept = listing("stmods", "keep", "BMW M2 G87", Some("2.0"));
        kept.author = Some("Author".to_string());
        kept.size_bytes = Some(100);
        let stale = listing("modland", "stale", "BMW M2 G87", Some("1.0"));
        let previous = SearchResponse {
            cards: Vec::new(),
            sources: vec![
                SourceReport {
                    source_id: "modland".to_string(),
                    display_name: "Modland".to_string(),
                    status: SourceStatus::Unavailable,
                    count: 0,
                    note: Some("сайт не ответил вовремя".to_string()),
                    pending: false,
                },
                SourceReport {
                    source_id: "stmods".to_string(),
                    display_name: "STMods".to_string(),
                    status: SourceStatus::Ok,
                    count: 1,
                    note: None,
                    pending: false,
                },
            ],
            listings: vec![stale, kept],
        };

        let response = service
            .retry_source(&previous, "modland", "bmw", &AtomicBool::new(false))
            .expect("сайт есть в сервисе");

        assert_eq!(response.listings.len(), 2, "{:?}", response.listings);
        assert!(
            response
                .listings
                .iter()
                .any(|item| item.source_id == "fresh"),
            "новая выдача упавшего сайта"
        );
        assert!(
            response
                .listings
                .iter()
                .any(|item| item.source_id == "keep"),
            "чужая выдача сохранена"
        );
        assert!(
            !response
                .listings
                .iter()
                .any(|item| item.source_id == "stale"),
            "старая выдача упавшего сайта убрана"
        );
        let report = response
            .sources
            .iter()
            .find(|report| report.source_id == "modland")
            .unwrap();
        assert_eq!(report.status, SourceStatus::Ok);
        assert_eq!(report.count, 1);
        assert!(report.note.is_none());
        assert_eq!(response.sources.len(), 2, "отчёт сайта не задвоился");
        assert_eq!(
            response
                .sources
                .iter()
                .find(|report| report.source_id == "stmods")
                .unwrap()
                .count,
            1
        );
        assert_eq!(response.cards.len(), 1, "карточка склеена заново");
        assert_eq!(response.cards[0].listings.len(), 2);
    }

    #[test]
    fn retry_unknown_source_returns_none() {
        let previous = SearchResponse {
            cards: Vec::new(),
            sources: Vec::new(),
            listings: Vec::new(),
        };
        assert!(
            service()
                .retry_source(&previous, "ghost", "bmw", &AtomicBool::new(false))
                .is_none()
        );
    }

    type BrowseAction = Box<dyn Fn(ModKind, u32) -> Vec<Listing> + Send + Sync>;

    struct BrowseSource {
        id: &'static str,
        kinds: Vec<ModKind>,
        pages: std::sync::Mutex<Vec<u32>>,
        action: BrowseAction,
    }

    impl BrowseSource {
        fn new<F>(id: &'static str, kinds: Vec<ModKind>, action: F) -> Arc<Self>
        where
            F: Fn(ModKind, u32) -> Vec<Listing> + Send + Sync + 'static,
        {
            Arc::new(Self {
                id,
                kinds,
                pages: std::sync::Mutex::new(Vec::new()),
                action: Box::new(action),
            })
        }
    }

    impl ModSource for BrowseSource {
        fn id(&self) -> &'static str {
            self.id
        }

        fn display_name(&self) -> &'static str {
            self.id
        }

        fn supports_search(&self) -> bool {
            true
        }

        fn home(&self) -> Url {
            Url::parse(&format!("https://{}.example.org/", self.id)).unwrap()
        }

        fn owns(&self, _url: &Url) -> bool {
            true
        }

        fn is_mod_page(&self, _url: &Url) -> bool {
            true
        }

        fn feed_covers(&self, _url: &Url) -> bool {
            true
        }

        fn search(&self, _query: &str, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }

        fn feed_page(&self, _page: u32, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }

        fn listing(
            &self,
            _url: &Url,
            _cancel: &AtomicBool,
        ) -> Result<Option<Listing>, SourceError> {
            Ok(None)
        }

        fn supports_browse(&self, kind: ModKind) -> bool {
            self.kinds.contains(&kind)
        }

        fn browse(
            &self,
            kind: ModKind,
            page: u32,
            _cancel: &AtomicBool,
        ) -> Result<Vec<Listing>, SourceError> {
            self.pages.lock().unwrap().push(page);
            Ok((self.action)(kind, page))
        }
    }

    #[test]
    fn browse_fetches_pages_and_keeps_site_order() {
        let first = BrowseSource::new("one", vec![ModKind::Map], |_kind, page| {
            vec![listing(
                "one",
                &format!("z{page}"),
                &format!("Zulu Map {page}"),
                None,
            )]
        });
        let second = BrowseSource::new("two", vec![ModKind::Map], |_kind, page| {
            vec![listing(
                "two",
                &format!("a{page}"),
                &format!("Alpha Map {page}"),
                None,
            )]
        });
        let service = service_with(
            vec![first.clone(), second.clone()],
            SearchOptions::default(),
        );
        let response = service.browse(ModKind::Map, 1, 2, &AtomicBool::new(false), None);
        assert_eq!(response.listings.len(), 4);
        assert!(
            response.cards[0].title.starts_with("Zulu"),
            "порядок сайтов: {:?}",
            response
                .cards
                .iter()
                .map(|card| &card.title)
                .collect::<Vec<_>>()
        );
        assert_eq!(*first.pages.lock().unwrap(), vec![1, 2]);
        assert_eq!(*second.pages.lock().unwrap(), vec![1, 2]);
        assert_eq!(response.sources[0].count, 2);

        let more = service.browse(ModKind::Map, 3, 2, &AtomicBool::new(false), None);
        assert_eq!(*first.pages.lock().unwrap(), vec![1, 2, 3, 4]);
        assert_eq!(more.listings.len(), 4);
    }

    #[test]
    fn browse_tells_which_site_has_no_section() {
        let maps = BrowseSource::new("maps", vec![ModKind::Map], |_kind, _page| {
            vec![listing("maps", "1", "Map", None)]
        });
        let cars = BrowseSource::new("cars", vec![ModKind::Vehicle], |_kind, _page| Vec::new());
        let service = service_with(vec![maps, cars], SearchOptions::default());
        let response = service.browse(ModKind::Map, 1, 2, &AtomicBool::new(false), None);
        let report = response
            .sources
            .iter()
            .find(|report| report.source_id == "cars")
            .unwrap();
        assert_eq!(report.status, SourceStatus::Ok);
        assert!(
            report.note.as_deref().unwrap_or("").contains("раздела"),
            "{report:?}"
        );
    }

    #[test]
    fn groups_same_mod_across_sites() {
        let mut first = listing("modland", "a", "BMW M2 G87", Some("1.0"));
        first.author = Some("Author".to_string());
        first.size_bytes = Some(100);
        let mut second = listing("stmods", "b", "BMW M2 G87", Some("1.0"));
        second.author = Some("Author".to_string());
        second.size_bytes = Some(100);
        let cards = service().group(&[first, second], "bmw");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].listings.len(), 2);
        assert_eq!(cards[0].id, "modland:a+stmods:b");
    }

    #[test]
    fn card_kind_from_section_then_title_and_title_is_clean() {
        let by_title = listing("modsgaming", "1", "автомобиль LADA 2106", None);
        let mut by_section = listing("modland", "2", "Sumner Road", None);
        by_section.category = Some("Maps".to_string());
        let unknown = listing("stmods", "3", "Dynamic Dirt", None);
        let cards = service().group(&[by_title, by_section, unknown], "x");
        let find = |id: &str| cards.iter().find(|card| card.id.contains(id)).unwrap();
        let lada = find("modsgaming:1");
        assert_eq!(lada.title, "LADA 2106", "рубрика ушла из названия");
        assert_eq!(lada.kind, Some(ModKind::Vehicle));
        assert_eq!(find("modland:2").kind, Some(ModKind::Map));
        assert_eq!(find("stmods:3").kind, None);
    }

    #[test]
    fn title_kind_wins_over_weak_description_hint() {
        // Карта STMods: описание сказало «cars» из-за слова «конфигурации»,
        // а название начинается с «Карта».
        let mut map = listing(
            "stmods",
            "karta_gambler_hills",
            "Карта «Gambler Hills» версия 1.3",
            None,
        );
        map.category = Some("cars".to_string());
        let cards = service().group(&[map], "gambler");
        assert_eq!(cards[0].kind, Some(ModKind::Map));
    }

    #[test]
    fn same_site_never_merges() {
        let first = listing("modland", "a", "BMW M2 G87", Some("1.0"));
        let second = listing("modland", "b", "BMW M2 G87", Some("1.0"));
        assert_eq!(service().group(&[first, second], "bmw").len(), 2);
    }

    #[test]
    fn newest_only_keeps_newest_version() {
        let old = listing("modsgaming", "1", "Temporal AA v1.0", Some("1.0"));
        let new = listing("modsgaming", "2", "Temporal AA v1.7", Some("1.7"));
        let result = newest_only(vec![old, new]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].mod_version.as_deref(), Some("1.7"));
    }

    #[test]
    fn price_summary_mixed() {
        let mut free = listing("a", "1", "BMW M2 G87", Some("1.0"));
        free.author = Some("Author".to_string());
        free.size_bytes = Some(100);
        free.price = PriceStatus::Free;
        let mut paid = listing("b", "2", "BMW M2 G87", Some("1.0"));
        paid.author = Some("Author".to_string());
        paid.size_bytes = Some(100);
        paid.price = PriceStatus::Paid;
        let cards = service().group(&[free, paid], "bmw");
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].price, PriceSummary::Mixed);
    }

    #[test]
    fn search_streams_progress_and_merges_sources() {
        let service = SearchService::new(
            vec![
                Arc::new(FakeSource {
                    id: "one",
                    title: "BMW M2 G87",
                }),
                Arc::new(FakeSource {
                    id: "two",
                    title: "BMW M2 G87",
                }),
            ],
            Arc::new(|_| SourceStatus::Ok),
            SearchOptions::default(),
            None,
        );
        let cancel = AtomicBool::new(false);
        let mut partials = 0;
        let mut callback = |response: &SearchResponse| {
            partials += 1;
            assert_eq!(response.sources.len(), 2);
            assert!(response.sources.iter().any(|report| report.pending));
        };
        let response = service.search(
            "bmw",
            &cancel,
            Some(&mut callback as &mut dyn FnMut(&SearchResponse)),
        );
        assert_eq!(partials, 1);
        assert_eq!(response.sources.len(), 2);
        assert_eq!(response.listings.len(), 2);
        assert_eq!(response.cards.len(), 1);
        assert_eq!(response.cards[0].listings.len(), 2);
        assert_eq!(response.cards[0].price, PriceSummary::Unknown);
    }

    type Action = Box<dyn Fn(&str, &AtomicBool) -> Result<Vec<Listing>, SourceError> + Send + Sync>;

    struct ScriptedSource {
        id: &'static str,
        action: Action,
    }

    impl ScriptedSource {
        fn new<F>(id: &'static str, action: F) -> Arc<Self>
        where
            F: Fn(&str, &AtomicBool) -> Result<Vec<Listing>, SourceError> + Send + Sync + 'static,
        {
            Arc::new(Self {
                id,
                action: Box::new(action),
            })
        }
    }

    impl ModSource for ScriptedSource {
        fn id(&self) -> &'static str {
            self.id
        }

        fn display_name(&self) -> &'static str {
            self.id
        }

        fn supports_search(&self) -> bool {
            true
        }

        fn home(&self) -> Url {
            Url::parse(&format!("https://{}.example.org/", self.id)).unwrap()
        }

        fn owns(&self, _url: &Url) -> bool {
            true
        }

        fn is_mod_page(&self, _url: &Url) -> bool {
            true
        }

        fn feed_covers(&self, _url: &Url) -> bool {
            true
        }

        fn search(&self, query: &str, cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            (self.action)(query, cancel)
        }

        fn feed_page(&self, _page: u32, _cancel: &AtomicBool) -> Result<Vec<Listing>, SourceError> {
            Ok(Vec::new())
        }

        fn listing(
            &self,
            _url: &Url,
            _cancel: &AtomicBool,
        ) -> Result<Option<Listing>, SourceError> {
            Ok(None)
        }
    }

    fn service_with(sources: Vec<Arc<dyn ModSource>>, options: SearchOptions) -> SearchService {
        SearchService::new(sources, Arc::new(|_| SourceStatus::Ok), options, None)
    }

    fn file_offer(id: &str) -> bmm_domain::models::Offer {
        bmm_domain::models::Offer {
            kind: bmm_domain::models::OfferKind::ExternalHost,
            host: "modsfire".to_string(),
            url: Url::parse(&format!("https://modsfire.com/{id}")).unwrap(),
            host_file_id: Some(id.to_string()),
            size_bytes: None,
        }
    }

    #[test]
    fn maybe_cards_point_at_each_other_without_merging() {
        let first = listing("modland", "a", "Toyota Sienna 2011-2017 2.2", None);
        let second = listing(
            "stmods",
            "b",
            "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE",
            None,
        );
        let cards = service().group(&[first, second], "sienna");
        assert_eq!(cards.len(), 2);
        // Вопрос задаётся один раз — на карточке, что выше в выдаче.
        assert_eq!(cards[0].possible_same_as, vec![cards[1].id.clone()]);
        assert!(cards[1].possible_same_as.is_empty());
    }

    #[test]
    fn one_question_per_card_even_with_many_lookalikes() {
        // Как на «rolls royce»: одна карточка ModsGaming и три похожих со STMods.
        let main = listing("modsgaming", "m", "Rolls Royce Phantom", None);
        let others = [
            listing("stmods", "1", "Rolls-Royce Phantom", None),
            listing("stmods", "2", "Rolls Royce Phantom VIII", None),
            listing("modland", "3", "Rolls Royce Phantom 2018", None),
        ];
        let mut all = vec![main];
        all.extend(others);
        let cards = service().group(&all, "rolls royce phantom");
        let questions: usize = cards.iter().map(|card| card.possible_same_as.len()).sum();
        assert!(
            cards.iter().all(|card| card.possible_same_as.len() <= 1),
            "{cards:?}"
        );
        // Каждая карточка участвует не больше чем в одном вопросе.
        let mut mentioned = std::collections::HashMap::<&str, usize>::new();
        for card in &cards {
            for other in &card.possible_same_as {
                *mentioned.entry(card.id.as_str()).or_default() += 1;
                *mentioned.entry(other.as_str()).or_default() += 1;
            }
        }
        assert!(mentioned.values().all(|count| *count == 1), "{mentioned:?}");
        assert!(questions >= 1);
    }

    #[test]
    fn no_question_when_cards_cannot_merge_anyway() {
        // У обеих карточек есть STMods: склеить их нельзя, значит и спрашивать незачем.
        let mut a = listing("stmods", "a", "Toyota Sienna 2011-2017 2.2", None);
        a.author = Some("Author".to_string());
        a.size_bytes = Some(100);
        let mut b = listing("modland", "b", "Toyota Sienna 2011-2017 2.2", None);
        b.author = Some("Author".to_string());
        b.size_bytes = Some(100);
        let c = listing(
            "stmods",
            "c",
            "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE",
            None,
        );
        let cards = service().group(&[a, b, c], "sienna");
        assert_eq!(cards.len(), 2, "{cards:?}");
        assert!(
            cards.iter().all(|card| card.possible_same_as.is_empty()),
            "{cards:?}"
        );
    }

    #[test]
    fn same_links_do_not_chain_three_sites_into_one_card() {
        // A~B делят файл, B~C делят другой, A и C — разные моды.
        let mut a = listing("s1", "a", "Alpha Roadster", None);
        a.offers = vec![file_offer("AAAAAAAAAA111")];
        let mut b = listing("s2", "b", "Beta Hauler", None);
        b.offers = vec![file_offer("AAAAAAAAAA111"), file_offer("BBBBBBBBBB222")];
        let mut c = listing("s3", "c", "Gamma Sedan", None);
        c.offers = vec![file_offer("BBBBBBBBBB222")];
        let cards = service().group(&[a, b, c], "car");
        assert!(cards.len() >= 2, "{cards:?}");
        assert!(cards.iter().all(|card| card.listings.len() <= 2));
    }

    #[test]
    fn search_keeps_one_card_for_repeated_record() {
        let source = ScriptedSource::new("stmods", |_query, _cancel| {
            let item = listing("stmods", "a", "Car X", None);
            Ok(vec![item.clone(), item])
        });
        let response = service_with(vec![source], SearchOptions::default()).search(
            "car",
            &AtomicBool::new(false),
            None,
        );
        assert_eq!(response.cards.len(), 1);
        assert_eq!(response.cards[0].listings.len(), 1);
    }

    #[test]
    fn search_limits_results_per_source() {
        let source = ScriptedSource::new("many", |_query, _cancel| {
            Ok(vec![
                listing("many", "1", "Car One", None),
                listing("many", "2", "Car Two", None),
                listing("many", "3", "Car Three", None),
            ])
        });
        let options = SearchOptions {
            max_per_source: 2,
            ..SearchOptions::default()
        };
        let response =
            service_with(vec![source], options).search("car", &AtomicBool::new(false), None);
        assert_eq!(response.listings.len(), 2);
        assert_eq!(response.sources[0].count, 2);
    }

    #[test]
    fn search_sorts_exact_title_first() {
        let source = ScriptedSource::new("modland", |_query, _cancel| {
            Ok(vec![
                listing("modland", "a", "Honda Civic Type R Pack Deluxe", None),
                listing("modland", "b", "Civic", None),
            ])
        });
        let response = service_with(vec![source], SearchOptions::default()).search(
            "civic",
            &AtomicBool::new(false),
            None,
        );
        assert_eq!(response.cards[0].title, "Civic");
    }

    #[test]
    fn search_reports_partial_source_failure() {
        let broken = ScriptedSource::new("modland", |_query, _cancel| {
            Err(SourceError::unavailable("разметка изменилась"))
        });
        let good = Arc::new(FakeSource {
            id: "stmods",
            title: "Car X",
        });
        let response = service_with(vec![broken, good], SearchOptions::default()).search(
            "car",
            &AtomicBool::new(false),
            None,
        );
        assert_eq!(response.cards.len(), 1);
        let report = response
            .sources
            .iter()
            .find(|report| report.source_id == "modland")
            .unwrap();
        assert_eq!(report.status, SourceStatus::Unavailable);
        assert!(report.note.as_deref().unwrap_or("").contains("разметка"));
    }

    #[test]
    fn search_times_out_hung_source_without_waiting() {
        let slow = ScriptedSource::new("slow", |_query, cancel| {
            let started = Instant::now();
            while started.elapsed() < Duration::from_secs(2) && !cancel.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(SourceError::cancelled())
        });
        let good = Arc::new(FakeSource {
            id: "stmods",
            title: "Car X",
        });
        let options = SearchOptions {
            per_source_timeout: Duration::from_millis(100),
            ..SearchOptions::default()
        };
        let started = Instant::now();
        let response =
            service_with(vec![slow, good], options).search("car", &AtomicBool::new(false), None);
        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");
        assert_eq!(response.cards.len(), 1);
        let report = response
            .sources
            .iter()
            .find(|report| report.source_id == "slow")
            .unwrap();
        assert_eq!(report.status, SourceStatus::Unavailable);
        assert!(report.note.as_deref().unwrap_or("").contains("вовремя"));
    }

    #[test]
    fn search_cancellation_returns_control() {
        let cancel = Arc::new(AtomicBool::new(false));
        let slow = ScriptedSource::new("slow", |_query, _cancel| {
            std::thread::sleep(Duration::from_secs(1));
            Ok(vec![listing("slow", "1", "Car X", None)])
        });
        let service = service_with(
            vec![slow],
            SearchOptions {
                per_source_timeout: Duration::from_secs(5),
                ..SearchOptions::default()
            },
        );
        let flag = Arc::clone(&cancel);
        let flip = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(40));
            flag.store(true, Ordering::Relaxed);
        });
        let started = Instant::now();
        let response = service.search("car", &cancel, None);
        let elapsed = started.elapsed();
        flip.join().unwrap();
        assert!(elapsed < Duration::from_millis(700), "{elapsed:?}");
        assert_eq!(response.sources[0].status, SourceStatus::Unavailable);
        assert!(
            response.sources[0]
                .note
                .as_deref()
                .unwrap_or("")
                .contains("отменён")
        );
    }

    #[test]
    fn search_cancelled_before_start_does_not_call_sources() {
        let called = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&called);
        let source = ScriptedSource::new("slow", move |_query, _cancel| {
            flag.store(true, Ordering::Relaxed);
            Ok(Vec::new())
        });
        let cancel = AtomicBool::new(true);
        let response =
            service_with(vec![source], SearchOptions::default()).search("car", &cancel, None);
        assert!(!called.load(Ordering::Relaxed));
        assert_eq!(response.sources[0].status, SourceStatus::Unavailable);
        assert!(
            response.sources[0]
                .note
                .as_deref()
                .unwrap_or("")
                .contains("отменён")
        );
    }

    #[test]
    fn search_isolates_source_panic() {
        let panicking = ScriptedSource::new("broken", |_query, _cancel| {
            panic!("boom in source");
        });
        let good = Arc::new(FakeSource {
            id: "stmods",
            title: "Car X",
        });
        let response = service_with(vec![panicking, good], SearchOptions::default()).search(
            "car",
            &AtomicBool::new(false),
            None,
        );
        assert_eq!(response.cards.len(), 1);
        let report = response
            .sources
            .iter()
            .find(|report| report.source_id == "broken")
            .unwrap();
        assert_eq!(report.status, SourceStatus::Unavailable);
        assert!(report.note.as_deref().unwrap_or("").contains("boom"));
    }

    #[test]
    fn search_skips_source_waiting_for_human_check() {
        let called = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&called);
        let source = ScriptedSource::new("modland", move |_query, _cancel| {
            flag.store(true, Ordering::Relaxed);
            Ok(Vec::new())
        });
        let good = Arc::new(FakeSource {
            id: "stmods",
            title: "Car X",
        });
        let service = SearchService::new(
            vec![source, good],
            Arc::new(|id| {
                if id == "modland" {
                    SourceStatus::NeedsHumanCheck
                } else {
                    SourceStatus::Ok
                }
            }),
            SearchOptions::default(),
            None,
        );
        let response = service.search("car", &AtomicBool::new(false), None);
        assert!(!called.load(Ordering::Relaxed));
        assert_eq!(response.cards.len(), 1);
        let report = response
            .sources
            .iter()
            .find(|report| report.source_id == "modland")
            .unwrap();
        assert_eq!(report.status, SourceStatus::NeedsHumanCheck);
    }
}

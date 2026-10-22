//! Общий поиск: параллельный опрос сайтов, слияние дублей и карточки.

pub mod relevance;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use bmm_domain::matches::{ManualMatch, MatchDecisions};
use bmm_domain::matching::{DuplicateMatcher, MatchVerdict};
use bmm_domain::models::{Listing, PriceStatus};
use bmm_domain::text::{TitleNormalizer, TitleParser};
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

    pub fn search(
        &self,
        raw_query: &str,
        cancel: &AtomicBool,
        mut progress: Option<&mut dyn FnMut(&SearchResponse)>,
    ) -> SearchResponse {
        let query = self.normalize(raw_query);
        if query.chars().count() < self.options.min_query_length {
            return SearchResponse {
                cards: Vec::new(),
                sources: Vec::new(),
                listings: Vec::new(),
            };
        }

        let (sender, receiver) = mpsc::channel::<(usize, SourceRun)>();
        let mut slots: Vec<Option<SourceRun>> = (0..self.sources.len()).map(|_| None).collect();
        std::thread::scope(|scope| {
            for (index, source) in self.sources.iter().enumerate() {
                let sender = sender.clone();
                let source = Arc::clone(source);
                let query = query.clone();
                scope.spawn(move || {
                    let run = self.run_one(source.as_ref(), &query, cancel);
                    let _ = sender.send((index, run));
                });
            }
            drop(sender);
            let mut done = 0;
            while done < self.sources.len() {
                let Ok((index, run)) = receiver.recv() else {
                    break;
                };
                slots[index] = Some(run);
                done += 1;
                if done < self.sources.len()
                    && let Some(callback) = progress.as_mut()
                {
                    (**callback)(&self.partial(&slots, &query));
                }
            }
        });

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
            cards: self.group(&listings, &query),
            sources: reports,
            listings,
        }
    }

    fn partial(&self, slots: &[Option<SourceRun>], query: &str) -> SearchResponse {
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
            cards: self.group(&listings, query),
            sources: reports,
            listings,
        }
    }

    fn run_one(&self, source: &dyn ModSource, query: &str, cancel: &AtomicBool) -> SourceRun {
        if !source.supports_search() {
            return SourceRun {
                report: SourceReport {
                    source_id: source.id().to_string(),
                    display_name: source.display_name().to_string(),
                    status: SourceStatus::Ok,
                    count: 0,
                    note: Some("поиск на этом сайте пока не поддерживается".to_string()),
                    pending: false,
                },
                listings: Vec::new(),
            };
        }
        let status = (self.status_of)(source.id());
        if status != SourceStatus::Ok {
            return SourceRun {
                report: SourceReport {
                    source_id: source.id().to_string(),
                    display_name: source.display_name().to_string(),
                    status,
                    count: 0,
                    note: Some(if status == SourceStatus::NeedsHumanCheck {
                        "нужно пройти проверку на сайте".to_string()
                    } else {
                        "сайт недоступен".to_string()
                    }),
                    pending: false,
                },
                listings: Vec::new(),
            };
        }
        match source.search(query, cancel) {
            Ok(found) => {
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
                        note: None,
                        pending: false,
                    },
                    listings,
                }
            }
            Err(error) => SourceRun {
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
        }
    }

    pub fn group(&self, listings: &[Listing], query: &str) -> Vec<ModCard> {
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

        let cards_meta: Vec<(String, usize)> = clusters
            .iter()
            .enumerate()
            .map(|(index, cluster)| (card_id(cluster, &unique), index))
            .collect();
        let mut cards = Vec::new();
        for (cluster_index, cluster) in clusters.iter().enumerate() {
            let id = cards_meta[cluster_index].0.clone();
            let mut possible_same_as = Vec::new();
            for (other_id, other_index) in &cards_meta {
                if *other_index == cluster_index {
                    continue;
                }
                let other = &clusters[*other_index];
                let maybe = cluster.iter().any(|&a| {
                    other.iter().any(|&b| {
                        unique[a].source != unique[b].source
                            && DuplicateMatcher::compare(&unique[a], &unique[b], self.decisions())
                                .verdict
                                == MatchVerdict::Maybe
                    })
                });
                if maybe {
                    possible_same_as.push(other_id.clone());
                }
            }
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
            cards.push(ModCard {
                id,
                title,
                listings: cluster.iter().map(|&index| unique[index].clone()).collect(),
                price: summarize(cluster.iter().map(|&index| unique[index].price)),
                possible_same_as,
                relevance,
            });
        }
        cards.sort_by(|left, right| {
            right
                .relevance
                .partial_cmp(&left.relevance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| left.title.to_lowercase().cmp(&right.title.to_lowercase()))
        });
        cards
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
    use std::sync::atomic::AtomicBool;
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
}

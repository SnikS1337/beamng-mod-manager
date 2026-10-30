//! Планировщик проверки: сколько страниц ленты и карточек открыть.
//!
//! Сети не касается. Сначала решает, каким сайтам пора обновлять ленты,
//! затем — какие карточки всё-таки нужно открыть, если лента про мод молчит.

use std::collections::{BTreeMap, HashMap};

use bmm_domain::versioning::SizeComparer;
use chrono::{DateTime, FixedOffset, TimeDelta};

use crate::options::UpdateCheckOptions;

pub use bmm_sources::sources::SourceStatus;

/// Привязка установленного мода к записи на сайте плюс то, что видели при прошлой проверке.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedBinding {
    pub mod_id: String,
    pub source: String,
    pub source_id: String,
    /// Включён ли мод сейчас; выключенные можно не проверять настройкой.
    pub enabled: bool,
    pub last_checked_utc: Option<DateTime<FixedOffset>>,
    pub last_changed_utc: Option<DateTime<FixedOffset>>,
    pub last_version: Option<String>,
    pub last_size: Option<i64>,
    pub miss_count: i32,
}

impl TrackedBinding {
    pub fn new(
        mod_id: impl Into<String>,
        source: impl Into<String>,
        source_id: impl Into<String>,
    ) -> Self {
        Self {
            mod_id: mod_id.into(),
            source: source.into(),
            source_id: source_id.into(),
            enabled: true,
            last_checked_utc: None,
            last_changed_utc: None,
            last_version: None,
            last_size: None,
            miss_count: 0,
        }
    }
}

/// Строка из ленты «новое / обновлённое» сайта (дешёвый источник сведений).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedEntry {
    pub source_id: String,
    pub version: Option<String>,
    pub size_bytes: Option<i64>,
    pub updated: Option<DateTime<FixedOffset>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedSource {
    pub source: String,
    pub status: SourceStatus,
}

/// Что читаем в этот запуск: сколько страниц ленты на сайт и какие привязки пора проверять.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FeedPlan {
    pub feed_pages: BTreeMap<String, u32>,
    pub due: Vec<TrackedBinding>,
    pub skipped_sources: Vec<SkippedSource>,
    pub total_requests: u32,
}

/// Какие карточки открывать, какие подтверждены лентой и какие отложены лимитом.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CardPlan {
    pub to_fetch: BTreeMap<String, Vec<TrackedBinding>>,
    pub confirmed_unchanged: Vec<TrackedBinding>,
    pub postponed: Vec<TrackedBinding>,
}

impl CardPlan {
    pub fn total_requests(&self) -> usize {
        self.to_fetch.values().map(Vec::len).sum()
    }
}

/// «Докуда дошла лента»: если лента отсортирована по дате обновления и у каждой
/// записи есть дата, всё непрочитанное обновлялось не позже конца прочитанного.
///
/// Любое сомнение (нет дат, порядок нарушен, записей мало, лента пуста) даёт
/// «покрытия нет», и работает обычная проверка карточек.
#[derive(Debug, Clone, Copy, Default)]
pub struct FeedCoverage;

impl FeedCoverage {
    /// Первые записи ленты могут быть «закреплёнными» и стоять вне порядка.
    pub const PINNED_ALLOWANCE: usize = 5;
    /// Запас на часовые пояса и округление дат сайта.
    pub const MARGIN: TimeDelta = TimeDelta::days(1);
    /// Меньше записей — о порядке судить нельзя.
    pub const MIN_ENTRIES: usize = 20;

    /// Дата, до которой прочитанная лента надёжно покрывает изменения; `None` — покрытия нет.
    pub fn compute(ordered: &[FeedEntry]) -> Option<DateTime<FixedOffset>> {
        if ordered.len() < Self::MIN_ENTRIES {
            return None;
        }
        if ordered.iter().any(|entry| entry.updated.is_none()) {
            return None;
        }

        // Порядок «от новых к старым»: после закреплённых даты не должны расти.
        let mut previous: Option<DateTime<FixedOffset>> = None;
        for entry in ordered.iter().skip(Self::PINNED_ALLOWANCE) {
            let date = entry.updated?;
            if let Some(previous) = previous
                && date > previous + Self::MARGIN
            {
                return None;
            }
            previous = Some(date);
        }

        // Самая молодая из дат в хвосте: всё непрочитанное не новее неё (консервативно).
        let start = Self::PINNED_ALLOWANCE.max(ordered.len().saturating_sub(10));
        ordered[start..]
            .iter()
            .filter_map(|entry| entry.updated)
            .max()
    }

    /// Привязка проверена достаточно давно после границы покрытия, чтобы считать её неизменной.
    pub fn proves(
        coverage: DateTime<FixedOffset>,
        last_checked: Option<DateTime<FixedOffset>>,
        now: DateTime<FixedOffset>,
    ) -> bool {
        last_checked.is_some_and(|checked| checked <= now && checked - coverage >= Self::MARGIN)
    }
}

/// Решает, сколько запросов потратить на проверку обновлений.
pub struct UpdateCheckPlanner;

impl UpdateCheckPlanner {
    /// Сколько проверок подряд страница мода не находится, прежде чем привязку считают потерянной.
    pub const DEAD_AFTER_MISSES: i32 = 2;

    const FIRST_MISS_RECHECK: TimeDelta = TimeDelta::days(1);
    const DEAD_RECHECK: TimeDelta = TimeDelta::days(7);

    /// Как часто открывать карточку мода, про который лента молчит.
    pub fn fallback_interval(binding: &TrackedBinding, now: DateTime<FixedOffset>) -> TimeDelta {
        let Some(changed) = binding.last_changed_utc else {
            return TimeDelta::days(1);
        };
        let age = now - changed;
        if age < TimeDelta::days(30) {
            TimeDelta::days(1)
        } else if age < TimeDelta::days(180) {
            TimeDelta::days(3)
        } else {
            TimeDelta::days(14)
        }
    }

    /// Отбирает привязки, которым пора обновить ленту, и считает глубину ленты на сайт.
    pub fn plan_feeds(
        bindings: &[TrackedBinding],
        statuses: &BTreeMap<String, SourceStatus>,
        now: DateTime<FixedOffset>,
        options: &UpdateCheckOptions,
    ) -> FeedPlan {
        let mut due = Vec::new();
        let mut skipped: BTreeMap<String, SourceStatus> = BTreeMap::new();

        for binding in bindings {
            if !options.check_disabled && !binding.enabled {
                continue;
            }
            let status = statuses
                .get(&binding.source)
                .copied()
                .unwrap_or(SourceStatus::Ok);
            if status != SourceStatus::Ok {
                skipped.insert(binding.source.clone(), status);
                continue;
            }
            // Время проверки «из будущего» (часы переводили назад) за свежее не считаем.
            if !options.force
                && let Some(checked) = binding.last_checked_utc
                && checked <= now
                && now - checked < options.ttl
            {
                continue;
            }
            due.push(binding.clone());
        }

        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for binding in &due {
            *counts.entry(binding.source.as_str()).or_default() += 1;
        }
        let mut feed_pages = BTreeMap::new();
        for (source, count) in counts {
            let extra = count / options.bindings_per_extra_feed_page.max(1);
            let pages = options
                .max_feed_pages_per_source
                .min(options.feed_pages_base + extra as u32);
            feed_pages.insert(source.to_string(), pages);
        }

        let total_requests = feed_pages.values().sum();
        FeedPlan {
            feed_pages,
            due,
            skipped_sources: skipped
                .into_iter()
                .map(|(source, status)| SkippedSource { source, status })
                .collect(),
            total_requests,
        }
    }

    /// Выбирает карточки к открытию: изменившиеся по ленте — первыми,
    /// затем те, про кого лента молчит и чей срок проверки вышел.
    ///
    /// `coverage` — границы покрытия по сайтам из [`FeedCoverage::compute`];
    /// `in_feed` отвечает, мог ли мод вообще попасть в ленту этого сайта.
    pub fn plan_cards(
        due: &[TrackedBinding],
        feed_by_source: &HashMap<String, HashMap<String, FeedEntry>>,
        now: DateTime<FixedOffset>,
        options: &UpdateCheckOptions,
        coverage: Option<&HashMap<String, DateTime<FixedOffset>>>,
        in_feed: Option<&dyn Fn(&TrackedBinding) -> bool>,
    ) -> CardPlan {
        let mut unchanged = Vec::new();
        let mut postponed = Vec::new();
        let mut to_fetch: BTreeMap<String, Vec<TrackedBinding>> = BTreeMap::new();

        // Порядок сайтов — по первому появлению в `due`.
        let mut order: Vec<String> = Vec::new();
        for binding in due {
            if !order.contains(&binding.source) {
                order.push(binding.source.clone());
            }
        }

        for source in order {
            let group: Vec<&TrackedBinding> = due
                .iter()
                .filter(|binding| binding.source == source)
                .collect();
            let feed = feed_by_source.get(&source);
            let mut changed: Vec<TrackedBinding> = Vec::new();
            let mut fallback: Vec<(TrackedBinding, i64)> = Vec::new();

            for binding in group {
                if let Some(entry) = feed.and_then(|feed| feed.get(&binding.source_id)) {
                    // Без прошлых данных уверенности нет — открываем карточку.
                    let same = binding.last_version.is_some()
                        && entry.version == binding.last_version
                        && !matches!(entry.size_bytes, Some(0))
                        && !matches!(binding.last_size, Some(0))
                        && SizeComparer::same(entry.size_bytes, binding.last_size) != Some(false);
                    if same {
                        unchanged.push(binding.clone());
                    } else {
                        changed.push(binding.clone());
                    }
                    continue;
                }

                // Лента датирована и дошла глубже последней проверки мода: его в ленте нет,
                // значит, он не менялся. Принудительная проверка этого доказательства не использует.
                if !options.force
                    && let Some(coverage) = coverage.and_then(|coverage| coverage.get(&source))
                    && binding.last_version.is_some()
                    && binding.miss_count == 0
                    && FeedCoverage::proves(*coverage, binding.last_checked_utc, now)
                    && in_feed.is_none_or(|in_feed| in_feed(binding))
                {
                    unchanged.push(binding.clone());
                    continue;
                }

                let since = match binding.last_checked_utc {
                    Some(checked) if checked <= now => (now - checked).num_seconds(),
                    _ => i64::MAX,
                };
                let interval = Self::fallback_interval(binding, now);
                let mut interval = interval;
                // Первый промах быстро перепроверяем (сбой сайта или правда удалили?),
                // а подтверждённо мёртвую страницу — раз в неделю.
                if binding.miss_count == 1 && interval > Self::FIRST_MISS_RECHECK {
                    interval = Self::FIRST_MISS_RECHECK;
                }
                if binding.miss_count >= Self::DEAD_AFTER_MISSES && interval < Self::DEAD_RECHECK {
                    interval = Self::DEAD_RECHECK;
                }
                let interval = interval.num_seconds();
                if options.force || since >= interval {
                    let overdue = if since == i64::MAX {
                        i64::MAX
                    } else {
                        since - interval
                    };
                    fallback.push((binding.clone(), overdue));
                } else {
                    postponed.push(binding.clone());
                }
            }

            // `sort_by_key` устойчива: при равном «просрочено» порядок остаётся прежним.
            fallback.sort_by_key(|(_, overdue)| std::cmp::Reverse(*overdue));
            let mut ordered = changed;
            ordered.extend(fallback.into_iter().map(|(binding, _)| binding));
            let take = options.max_card_checks_per_source;
            let rest = if ordered.len() > take {
                ordered.split_off(take)
            } else {
                Vec::new()
            };
            postponed.extend(rest);
            if !ordered.is_empty() {
                to_fetch.insert(source, ordered);
            }
        }

        CardPlan {
            to_fetch,
            confirmed_unchanged: unchanged,
            postponed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-13T12:00:00+00:00").unwrap()
    }

    fn binding(
        index: usize,
        source: &str,
        last_checked: Option<DateTime<FixedOffset>>,
        last_changed: Option<DateTime<FixedOffset>>,
        version: Option<&str>,
        size: Option<i64>,
    ) -> TrackedBinding {
        TrackedBinding {
            mod_id: format!("m{index}"),
            source: source.to_string(),
            source_id: format!("id{index}"),
            enabled: true,
            last_checked_utc: last_checked,
            last_changed_utc: last_changed,
            last_version: version.map(str::to_string),
            last_size: size,
            miss_count: 0,
        }
    }

    fn mods(count: usize) -> Vec<TrackedBinding> {
        let sources = ["modland", "modsgaming", "stmods"];
        (0..count)
            .map(|index| binding(index, sources[index % 3], None, None, None, None))
            .collect()
    }

    fn feeds(items: &[(&str, FeedEntry)]) -> HashMap<String, HashMap<String, FeedEntry>> {
        let mut result: HashMap<String, HashMap<String, FeedEntry>> = HashMap::new();
        for (source, entry) in items {
            result
                .entry((*source).to_string())
                .or_default()
                .insert(entry.source_id.clone(), entry.clone());
        }
        result
    }

    fn feed_entry(id: &str, version: Option<&str>, size: Option<i64>) -> FeedEntry {
        FeedEntry {
            source_id: id.to_string(),
            version: version.map(str::to_string),
            size_bytes: size,
            updated: None,
        }
    }

    fn ok_statuses() -> BTreeMap<String, SourceStatus> {
        BTreeMap::new()
    }

    #[test]
    fn two_hundred_mods_need_few_feed_requests() {
        let plan = UpdateCheckPlanner::plan_feeds(
            &mods(200),
            &ok_statuses(),
            now(),
            &UpdateCheckOptions::default(),
        );
        assert_eq!(plan.due.len(), 200);
        assert!(
            plan.total_requests <= 30,
            "запросов {}",
            plan.total_requests
        );
        assert!(plan.feed_pages.values().all(|pages| *pages <= 10));
    }

    #[test]
    fn two_hundred_mods_without_feed_respect_card_limit() {
        let options = UpdateCheckOptions {
            max_card_checks_per_source: 25,
            ..UpdateCheckOptions::default()
        };
        let plan = UpdateCheckPlanner::plan_feeds(&mods(200), &ok_statuses(), now(), &options);
        let cards =
            UpdateCheckPlanner::plan_cards(&plan.due, &feeds(&[]), now(), &options, None, None);
        assert!(cards.to_fetch.values().all(|list| list.len() <= 25));
        assert!(cards.total_requests() <= 75);
        assert_eq!(
            200,
            cards.total_requests() + cards.postponed.len() + cards.confirmed_unchanged.len()
        );
    }

    #[test]
    fn two_hundred_mods_fit_in_request_budget() {
        let options = UpdateCheckOptions {
            max_card_checks_per_source: 25,
            ..UpdateCheckOptions::default()
        };
        let plan = UpdateCheckPlanner::plan_feeds(&mods(200), &ok_statuses(), now(), &options);
        let cards =
            UpdateCheckPlanner::plan_cards(&plan.due, &feeds(&[]), now(), &options, None, None);
        assert!(usize::try_from(plan.total_requests).unwrap() + cards.total_requests() <= 105);
    }

    #[test]
    fn fresh_check_within_ttl_is_skipped() {
        let bindings: Vec<_> = (0..3)
            .map(|index| {
                binding(
                    index,
                    "stmods",
                    Some(now() - TimeDelta::hours(1)),
                    None,
                    None,
                    None,
                )
            })
            .collect();
        let plan = UpdateCheckPlanner::plan_feeds(
            &bindings,
            &ok_statuses(),
            now(),
            &UpdateCheckOptions::default(),
        );
        assert!(plan.due.is_empty());
    }

    #[test]
    fn disabled_mods_are_skipped_only_when_asked() {
        let mut off = binding(0, "stmods", None, None, None, None);
        off.enabled = false;
        let on = binding(1, "stmods", None, None, None, None);
        let bindings = vec![off, on];

        let default = UpdateCheckPlanner::plan_feeds(
            &bindings,
            &ok_statuses(),
            now(),
            &UpdateCheckOptions::default(),
        );
        assert_eq!(default.due.len(), 2);

        let options = UpdateCheckOptions {
            check_disabled: false,
            ..UpdateCheckOptions::default()
        };
        let skipped = UpdateCheckPlanner::plan_feeds(&bindings, &ok_statuses(), now(), &options);
        assert_eq!(skipped.due.len(), 1);
        assert_eq!(skipped.due[0].mod_id, "m1");
    }

    #[test]
    fn force_ignores_ttl() {
        let bindings: Vec<_> = (0..3)
            .map(|index| {
                binding(
                    index,
                    "stmods",
                    Some(now() - TimeDelta::hours(1)),
                    None,
                    None,
                    None,
                )
            })
            .collect();
        let options = UpdateCheckOptions::default().with_force(true);
        let plan = UpdateCheckPlanner::plan_feeds(&bindings, &ok_statuses(), now(), &options);
        assert_eq!(plan.due.len(), 3);
    }

    #[test]
    fn human_check_source_is_skipped_but_others_go() {
        let statuses = BTreeMap::from([("modland".to_string(), SourceStatus::NeedsHumanCheck)]);
        let plan = UpdateCheckPlanner::plan_feeds(
            &mods(30),
            &statuses,
            now(),
            &UpdateCheckOptions::default(),
        );
        assert!(!plan.feed_pages.contains_key("modland"));
        assert!(plan.feed_pages.contains_key("stmods"));
        assert_eq!(plan.skipped_sources.len(), 1);
        assert_eq!(
            plan.skipped_sources[0].status,
            SourceStatus::NeedsHumanCheck
        );
        assert!(plan.due.iter().all(|binding| binding.source != "modland"));
    }

    #[test]
    fn unavailable_sources_spend_no_requests() {
        let statuses = BTreeMap::from([
            ("stmods".to_string(), SourceStatus::Unavailable),
            ("modland".to_string(), SourceStatus::Unavailable),
            ("modsgaming".to_string(), SourceStatus::Unavailable),
        ]);
        let plan = UpdateCheckPlanner::plan_feeds(
            &mods(30),
            &statuses,
            now(),
            &UpdateCheckOptions::default(),
        );
        assert_eq!(plan.total_requests, 0);
        assert!(plan.due.is_empty());
    }

    #[test]
    fn feed_same_version_needs_no_card() {
        let binding = binding(
            0,
            "stmods",
            Some(now() - TimeDelta::days(2)),
            Some(now() - TimeDelta::days(10)),
            Some("1.0"),
            Some(100),
        );
        let cards = UpdateCheckPlanner::plan_cards(
            &[binding],
            &feeds(&[("stmods", feed_entry("id0", Some("1.0"), Some(100)))]),
            now(),
            &UpdateCheckOptions::default(),
            None,
            None,
        );
        assert_eq!(cards.total_requests(), 0);
        assert_eq!(cards.confirmed_unchanged.len(), 1);
    }

    #[test]
    fn feed_changed_version_opens_card() {
        let binding = binding(
            0,
            "stmods",
            Some(now() - TimeDelta::days(2)),
            Some(now() - TimeDelta::days(10)),
            Some("1.0"),
            Some(100),
        );
        let cards = UpdateCheckPlanner::plan_cards(
            &[binding],
            &feeds(&[("stmods", feed_entry("id0", Some("1.1"), Some(100)))]),
            now(),
            &UpdateCheckOptions::default(),
            None,
            None,
        );
        assert_eq!(cards.to_fetch["stmods"].len(), 1);
    }

    #[test]
    fn changed_cards_go_before_silent_ones() {
        let silent = binding(
            0,
            "stmods",
            Some(now() - TimeDelta::days(30)),
            Some(now() - TimeDelta::days(5)),
            Some("1"),
            Some(1),
        );
        let changed = binding(
            1,
            "stmods",
            Some(now() - TimeDelta::days(1)),
            Some(now() - TimeDelta::days(5)),
            Some("1"),
            Some(1),
        );
        let cards = UpdateCheckPlanner::plan_cards(
            &[silent, changed],
            &feeds(&[("stmods", feed_entry("id1", Some("2"), Some(1)))]),
            now(),
            &UpdateCheckOptions::default(),
            None,
            None,
        );
        assert_eq!(cards.to_fetch["stmods"][0].mod_id, "m1");
    }

    #[test]
    fn feed_does_not_prove_anything_without_history() {
        let binding = binding(0, "stmods", None, None, None, None);
        let cards = UpdateCheckPlanner::plan_cards(
            &[binding],
            &feeds(&[("stmods", feed_entry("id0", Some("1.0"), Some(100)))]),
            now(),
            &UpdateCheckOptions::default(),
            None,
            None,
        );
        assert_eq!(cards.total_requests(), 1);
    }

    #[test]
    fn old_unchanged_mod_is_rarely_checked() {
        let old = binding(
            0,
            "stmods",
            Some(now() - TimeDelta::days(2)),
            Some(now() - TimeDelta::days(400)),
            Some("1"),
            Some(1),
        );
        let options = UpdateCheckOptions {
            ttl: TimeDelta::zero(),
            ..UpdateCheckOptions::default()
        };
        let cards =
            UpdateCheckPlanner::plan_cards(&[old], &feeds(&[]), now(), &options, None, None);
        assert_eq!(cards.total_requests(), 0);
        assert_eq!(cards.postponed.len(), 1);
    }

    #[test]
    fn recently_changed_mod_is_checked_daily() {
        let fresh = binding(
            0,
            "stmods",
            Some(now() - TimeDelta::days(2)),
            Some(now() - TimeDelta::days(3)),
            Some("1"),
            Some(1),
        );
        let cards = UpdateCheckPlanner::plan_cards(
            &[fresh],
            &feeds(&[]),
            now(),
            &UpdateCheckOptions::default(),
            None,
            None,
        );
        assert_eq!(cards.total_requests(), 1);
    }

    #[test]
    fn most_overdue_mod_is_taken_first() {
        let bindings: Vec<_> = (0..30)
            .map(|index| {
                binding(
                    index,
                    "stmods",
                    Some(now() - TimeDelta::days(2 + index as i64)),
                    Some(now() - TimeDelta::days(5)),
                    Some("1"),
                    Some(1),
                )
            })
            .collect();
        let options = UpdateCheckOptions {
            max_card_checks_per_source: 25,
            ..UpdateCheckOptions::default()
        };
        let cards =
            UpdateCheckPlanner::plan_cards(&bindings, &feeds(&[]), now(), &options, None, None);
        assert_eq!(cards.to_fetch["stmods"].len(), 25);
        assert_eq!(cards.to_fetch["stmods"][0].mod_id, "m29");
        assert_eq!(cards.postponed.len(), 5);
    }

    #[test]
    fn force_opens_cards_without_interval() {
        let old = binding(
            0,
            "stmods",
            Some(now() - TimeDelta::days(2)),
            Some(now() - TimeDelta::days(400)),
            Some("1"),
            Some(1),
        );
        let options = UpdateCheckOptions::default().with_force(true);
        let cards =
            UpdateCheckPlanner::plan_cards(&[old], &feeds(&[]), now(), &options, None, None);
        assert_eq!(cards.total_requests(), 1);
    }

    fn dated_entry(id: &str, day: i64) -> FeedEntry {
        FeedEntry {
            source_id: id.to_string(),
            version: Some("1.0".to_string()),
            size_bytes: None,
            updated: Some(now() - TimeDelta::days(day)),
        }
    }

    #[test]
    fn coverage_requires_enough_dated_ordered_entries() {
        let ordered: Vec<_> = (0..20)
            .map(|index| dated_entry(&index.to_string(), index))
            .collect();
        assert!(FeedCoverage::compute(&ordered).is_some());

        let short: Vec<_> = (0..19)
            .map(|index| dated_entry(&index.to_string(), index))
            .collect();
        assert_eq!(FeedCoverage::compute(&short), None);
    }

    #[test]
    fn coverage_rejects_missing_dates_and_broken_order() {
        let mut no_dates: Vec<_> = (0..20)
            .map(|index| dated_entry(&index.to_string(), index))
            .collect();
        no_dates[7].updated = None;
        assert_eq!(FeedCoverage::compute(&no_dates), None);

        let mut broken: Vec<_> = (0..20)
            .map(|index| dated_entry(&index.to_string(), index))
            .collect();
        broken[15] = dated_entry("jump", 0); // новее, чем запись выше
        assert_eq!(FeedCoverage::compute(&broken), None);
    }

    #[test]
    fn coverage_proves_only_after_margin() {
        let coverage = now() - TimeDelta::days(10);
        assert!(FeedCoverage::proves(
            coverage,
            Some(now() - TimeDelta::days(2)),
            now()
        ));
        assert!(!FeedCoverage::proves(
            coverage,
            Some(now() - TimeDelta::days(20)),
            now()
        ));
        assert!(!FeedCoverage::proves(
            coverage,
            Some(now() + TimeDelta::days(1)),
            now()
        ));
        assert!(!FeedCoverage::proves(coverage, None, now()));
    }
}

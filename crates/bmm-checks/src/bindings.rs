//! Привязки: подсказки, отказ «Не проверять» и починка потерянных страниц.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;

use url::Url;

use bmm_domain::filename::{GuessedName, from_file_name};
use bmm_domain::library::{CheckState, InstalledMod, LibraryStore, ModBinding};
use bmm_domain::matching::{DuplicateMatcher, MatchResult, MatchVerdict};
use bmm_domain::models::Listing;
use bmm_sources::search::{SearchService, SourceReport};

use crate::planner::UpdateCheckPlanner;

/// Единая защёлка для действий, которые меняют несколько записей хранилища разом.
/// Хранилище защищает каждую запись отдельно; здесь важно, чтобы «прочитать —
/// решить — записать» не перемешалось с параллельным действием.
static GATE: Mutex<()> = Mutex::new(());

pub(crate) fn gate() -> std::sync::MutexGuard<'static, ()> {
    GATE.lock().unwrap_or_else(|error| error.into_inner())
}

/// Подсказка привязки: запись на сайте и результат сравнения с установленным модом.
#[derive(Debug, Clone, PartialEq)]
pub struct BindingSuggestion {
    pub listing: Listing,
    pub matched: MatchResult,
}

impl BindingSuggestion {
    /// Только при уверенном совпадении (`Same`); остальное подтверждает пользователь.
    pub fn auto_confirmable(&self) -> bool {
        self.matched.verdict == MatchVerdict::Same
    }
}

/// Подбор привязок: к какой записи на сайте относится установленный файл.
pub struct BindingSuggester;

impl BindingSuggester {
    /// Достаёт читаемое название (и, если есть, автора и версию) из имени файла.
    pub fn from_file_name(file_name: &str) -> GuessedName {
        from_file_name(file_name)
    }

    /// Подсказки с отсевом заведомо разных записей.
    pub fn suggest(
        installed: &InstalledMod,
        candidates: &[Listing],
        max: usize,
    ) -> Vec<BindingSuggestion> {
        Self::rank(installed, candidates, usize::MAX)
            .into_iter()
            .filter(|suggestion| suggestion.matched.verdict != MatchVerdict::Different)
            .take(max)
            .collect()
    }

    /// Все кандидаты по убыванию сходства, без отсева. Для окна «Указать страницу»:
    /// записи уже найдены поиском сайта по названию мода, человек выбирает сам.
    pub fn rank(
        installed: &InstalledMod,
        candidates: &[Listing],
        max: usize,
    ) -> Vec<BindingSuggestion> {
        let guess = from_file_name(&installed.file_name);
        let title = if installed.name.trim().is_empty() {
            guess.name.clone()
        } else {
            installed.name.clone()
        };
        let mut probe = Listing::new(
            "installed",
            Url::parse("https://installed.invalid/mod").expect("статический адрес"),
            installed.id.clone(),
            title,
        );
        probe.author = guess.author;
        probe.size_bytes = installed.size_bytes;
        probe.mod_version = installed.version.clone().or(guess.version);

        let mut ranked: Vec<BindingSuggestion> = candidates
            .iter()
            .map(|candidate| BindingSuggestion {
                listing: candidate.clone(),
                matched: DuplicateMatcher::compare(&probe, candidate, None),
            })
            .collect();
        // Устойчивая сортировка по убыванию, как `OrderByDescending`.
        ranked.sort_by(|a, b| b.matched.score.total_cmp(&a.matched.score));
        ranked.truncate(max);
        ranked
    }

    /// Подсказки с учётом отказа «Не проверять этот мод».
    pub fn suggest_for_store(
        store: &dyn LibraryStore,
        installed: &InstalledMod,
        candidates: &[Listing],
        max: usize,
    ) -> Vec<BindingSuggestion> {
        if store.is_checking_declined(&installed.id) {
            return Vec::new();
        }
        Self::suggest(installed, candidates, max)
    }

    /// Чем искать мод: название из имени файла, иначе запись библиотеки.
    pub fn query_for(installed: &InstalledMod) -> String {
        let guessed = from_file_name(&installed.file_name).name;
        if guessed.trim().is_empty() {
            installed.name.clone()
        } else {
            guessed
        }
    }

    /// Ищет кандидатов общим поиском и ранжирует.
    pub fn find(
        installed: &InstalledMod,
        search: &SearchService,
        cancel: &AtomicBool,
    ) -> Vec<BindingSuggestion> {
        let query = Self::query_for(installed);
        let response = search.search(&query, cancel, None);
        let listings: Vec<Listing> = response
            .cards
            .iter()
            .flat_map(|card| card.listings.clone())
            .collect();
        Self::suggest(installed, &listings, 5)
    }

    /// Как [`Self::find`], но моду с отказом «Не проверять» ничего не ищет и не предлагает.
    pub fn find_for_store(
        store: &dyn LibraryStore,
        installed: &InstalledMod,
        search: &SearchService,
        cancel: &AtomicBool,
    ) -> Vec<BindingSuggestion> {
        if store.is_checking_declined(&installed.id) {
            return Vec::new();
        }
        Self::find(installed, search, cancel)
    }

    /// Убирает записи, которые уже отслеживаются (та же страница или тот же сайт с тем же id),
    /// — для окна «Указать страницу» и замены пропавшей.
    pub fn without_bound(listings: Vec<Listing>, bound: &[ModBinding]) -> Vec<Listing> {
        let mut seen: HashSet<(String, String)> = HashSet::new();
        listings
            .into_iter()
            .filter(|listing| seen.insert((listing.source.clone(), listing.source_id.clone())))
            .filter(|listing| {
                !bound.iter().any(|binding| {
                    binding.source == listing.source
                        && (binding.source_id == listing.source_id || binding.url == listing.url)
                })
            })
            .collect()
    }
}

/// Что снято при «Не проверять этот мод»: нужно, чтобы «Отменить» вернуло всё как было.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclineUndo {
    pub mod_id: String,
    pub bindings: Vec<ModBinding>,
    pub checks: Vec<CheckState>,
    pub dismissed_update: Option<String>,
}

/// Действия пользователя над привязками, которые должны менять несколько вещей разом.
pub struct BindingActions;

impl BindingActions {
    /// «Не проверять этот мод»: снимает все привязки вместе с состоянием проверок
    /// и пропущенной версией и запоминает отказ, чтобы подсказки привязки не
    /// предлагали страницу заново при следующей сверке.
    pub fn decline(store: &dyn LibraryStore, mod_id: &str) -> Option<DeclineUndo> {
        let _gate = gate();
        store.get_mod(mod_id)?;
        let bindings: Vec<ModBinding> = store
            .bindings()
            .into_iter()
            .filter(|binding| binding.mod_id == mod_id)
            .collect();
        let checks: Vec<CheckState> = store
            .checks()
            .into_iter()
            .filter(|check| check.mod_id == mod_id)
            .collect();
        let dismissed = store.get_dismissed_update(mod_id);
        for binding in &bindings {
            store.remove_binding(mod_id, &binding.source);
        }
        store.set_dismissed_update(mod_id, None);
        store.set_checking_declined(mod_id, true);
        Some(DeclineUndo {
            mod_id: mod_id.to_string(),
            bindings,
            checks,
            dismissed_update: dismissed,
        })
    }

    /// То же, что [`Self::decline`], но возвращает только число снятых привязок.
    pub fn decline_checking(store: &dyn LibraryStore, mod_id: &str) -> usize {
        Self::decline(store, mod_id)
            .map(|undo| undo.bindings.len())
            .unwrap_or(0)
    }

    /// «Отменить» после «Не проверять этот мод»: возвращает привязки, состояние
    /// проверок и пропущенную версию и снимает отказ. Если мод удалён или у него
    /// уже есть новая привязка к тому же сайту, она не перезаписывается.
    pub fn undo(store: &dyn LibraryStore, undo: &DeclineUndo) -> bool {
        let _gate = gate();
        if store.get_mod(&undo.mod_id).is_none() {
            return false;
        }
        let now: HashSet<String> = store
            .bindings()
            .into_iter()
            .filter(|binding| binding.mod_id == undo.mod_id)
            .map(|binding| binding.source.to_lowercase())
            .collect();
        for binding in undo
            .bindings
            .iter()
            .filter(|binding| !now.contains(&binding.source.to_lowercase()))
        {
            let _ = store.upsert_binding(binding.clone());
        }
        for check in undo
            .checks
            .iter()
            .filter(|check| !now.contains(&check.source.to_lowercase()))
        {
            store.save_check(check.clone());
        }
        if store.get_dismissed_update(&undo.mod_id).is_none()
            && let Some(dismissed) = &undo.dismissed_update
        {
            store.set_dismissed_update(&undo.mod_id, Some(dismissed));
        }
        store.set_checking_declined(&undo.mod_id, false);
        true
    }

    /// Возвращает моду обычное поведение: подсказки снова предлагаются.
    /// Привязку это само не создаёт.
    pub fn resume_checking(store: &dyn LibraryStore, mod_id: &str) {
        store.set_checking_declined(mod_id, false);
    }
}

/// Привязка, страница которой несколько проверок подряд не находится.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeadBinding {
    pub installed: InstalledMod,
    pub binding: ModBinding,
    pub misses: i32,
}

/// Найденные замены для потерянной страницы. Ничего не привязывается само.
#[derive(Debug, Clone)]
pub struct RepairProposal {
    pub dead: DeadBinding,
    pub candidates: Vec<BindingSuggestion>,
    pub sources: Vec<SourceReport>,
}

/// Починка привязок: если страницы мода больше нет, предлагаем похожие записи
/// на сайтах, а пользователь выбирает.
pub struct BindingRepair;

impl BindingRepair {
    /// Одна неудача может быть сбоем сайта; потерянной привязку считаем после двух проверок подряд.
    pub const DEAD_AFTER_MISSES: i32 = UpdateCheckPlanner::DEAD_AFTER_MISSES;

    pub fn dead(store: &dyn LibraryStore) -> Vec<DeadBinding> {
        let mods: HashMap<String, InstalledMod> = store
            .mods()
            .into_iter()
            .map(|installed| (installed.id.clone(), installed))
            .collect();
        let by_key: HashMap<(String, String), ModBinding> = store
            .bindings()
            .into_iter()
            .filter(|binding| binding.confirmed)
            .map(|binding| ((binding.mod_id.clone(), binding.source.clone()), binding))
            .collect();
        let mut result = Vec::new();
        for check in store.checks() {
            if check.miss_count < Self::DEAD_AFTER_MISSES {
                continue;
            }
            let Some(installed) = mods.get(&check.mod_id) else {
                continue;
            };
            let Some(binding) = by_key.get(&(check.mod_id.clone(), check.source.clone())) else {
                continue;
            };
            result.push(DeadBinding {
                installed: installed.clone(),
                binding: binding.clone(),
                misses: check.miss_count,
            });
        }
        result.sort_by(|a, b| {
            a.installed
                .name
                .to_lowercase()
                .cmp(&b.installed.name.to_lowercase())
                .then_with(|| a.binding.source.cmp(&b.binding.source))
                .then_with(|| a.installed.id.cmp(&b.installed.id))
        });
        result
    }

    /// Ищет замену по названию. Исключает саму потерянную страницу и сайты,
    /// где у мода уже есть другая живая привязка.
    pub fn propose(
        store: &dyn LibraryStore,
        dead: &DeadBinding,
        search: &SearchService,
        cancel: &AtomicBool,
    ) -> RepairProposal {
        let query = BindingSuggester::query_for(&dead.installed);
        let response = search.search(&query, cancel, None);
        let other_sources = other_bound_sources(store, dead);
        let listings: Vec<Listing> = response
            .cards
            .iter()
            .flat_map(|card| card.listings.clone())
            .collect();
        let candidates = BindingSuggester::suggest(&dead.installed, &listings, 5)
            .into_iter()
            .filter(|suggestion| {
                !is_same_page(&suggestion.listing, &dead.binding)
                    && !other_sources.contains(&suggestion.listing.source)
            })
            .collect();
        RepairProposal {
            dead: dead.clone(),
            candidates,
            sources: response.sources,
        }
    }

    /// Заменяет потерянную привязку выбранной записью. Возвращает `false`, если
    /// за время выбора что-то изменилось (мод удалён, привязку уже починили или
    /// у мода появилась другая привязка на этом сайте): тогда ничего не трогаем.
    pub fn apply(store: &dyn LibraryStore, dead: &DeadBinding, chosen: &Listing) -> bool {
        let _gate = gate();
        if store.get_mod(&dead.installed.id).is_none() {
            return false;
        }
        let current = store.bindings().into_iter().find(|binding| {
            binding.mod_id == dead.installed.id && binding.source == dead.binding.source
        });
        let Some(current) = current else {
            return false; // уже убрана
        };
        if current.source_id != dead.binding.source_id {
            return false; // уже изменена
        }
        if is_same_page(chosen, &dead.binding) {
            return false; // ту же мёртвую страницу выбирать незачем
        }
        if other_bound_sources(store, dead).contains(&chosen.source) {
            return false; // перезаписали бы живую привязку
        }

        // Сначала убираем старую вместе с её состоянием проверок (иначе при замене
        // на том же сайте остались бы старые промахи).
        store.remove_binding(&dead.installed.id, &dead.binding.source);
        store
            .upsert_binding(ModBinding {
                mod_id: dead.installed.id.clone(),
                source: chosen.source.clone(),
                source_id: chosen.source_id.clone(),
                url: chosen.url.clone(),
                confirmed: true,
            })
            .is_ok()
    }
}

fn other_bound_sources(store: &dyn LibraryStore, dead: &DeadBinding) -> HashSet<String> {
    store
        .bindings()
        .into_iter()
        .filter(|binding| {
            binding.mod_id == dead.installed.id && binding.source != dead.binding.source
        })
        .map(|binding| binding.source)
        .collect()
}

fn is_same_page(listing: &Listing, binding: &ModBinding) -> bool {
    listing.source == binding.source
        && (listing.source_id == binding.source_id || listing.url == binding.url)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, TimeDelta};

    use bmm_domain::library::InMemoryLibraryStore;

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-13T12:00:00+00:00").unwrap()
    }

    fn mod_with(id: &str, file: &str, name: &str) -> InstalledMod {
        InstalledMod {
            id: id.to_string(),
            name: name.to_string(),
            file_name: file.to_string(),
            version: None,
            size_bytes: None,
            installed_at_utc: now() - TimeDelta::days(1),
            enabled: true,
        }
    }

    fn listing(source: &str, id: &str, title: &str) -> Listing {
        Listing::new(
            source,
            Url::parse(&format!("https://{source}.test/{id}")).unwrap(),
            id,
            title,
        )
    }

    fn add(store: &InMemoryLibraryStore, id: &str) {
        store.upsert_mod(mod_with(id, &format!("{id}.zip"), id));
        store
            .upsert_binding(ModBinding {
                mod_id: id.to_string(),
                source: "stmods".to_string(),
                source_id: id.to_string(),
                url: Url::parse(&format!("https://stmods.test/{id}")).unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            mod_id: id.to_string(),
            source: "stmods".to_string(),
            last_checked_utc: now(),
            last_changed_utc: Some(now()),
            last_version: Some("1.0".to_string()),
            last_size: Some(10),
            miss_count: 3,
            last_game_version: None,
            last_title: None,
        });
    }

    #[test]
    fn file_name_guess_matches_legacy() {
        let first = BindingSuggester::from_file_name("BMW_M3_E30_v1.2.zip");
        assert_eq!(first.name, "BMW M3 E30");
        assert_eq!(first.version.as_deref(), Some("1.2"));
        let engine = BindingSuggester::from_file_name("Ford_V8.zip");
        assert_eq!(engine.name, "Ford V8");
        assert!(engine.version.is_none());
        assert_eq!(
            BindingSuggester::from_file_name("stmods_car.zip").name,
            "car"
        );
        assert_eq!(
            BindingSuggester::from_file_name("xstmods.zip").name,
            "xstmods"
        );
    }

    #[test]
    fn rank_and_suggest_keep_same_and_maybe() {
        let installed = mod_with("a", "toyota_sienna_2017_v1.0.zip", "Toyota Sienna 2017");
        let candidates = [
            listing(
                "stmods",
                "1",
                "МОД Toyota Sienna 2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE",
            ),
            listing("stmods", "2", "Ford Mustang"),
        ];
        let ranked = BindingSuggester::rank(&installed, &candidates, 5);
        assert_eq!(ranked.len(), 2);
        assert!(ranked[0].matched.score >= ranked[1].matched.score);

        let suggested = BindingSuggester::suggest(&installed, &candidates, 5);
        assert_eq!(suggested.len(), 1);
        assert_eq!(suggested[0].listing.source_id, "1");
    }

    #[test]
    fn decline_and_undo_restore_bindings_checks_and_dismissed() {
        let store = InMemoryLibraryStore::new();
        add(&store, "a");
        store.set_dismissed_update("a", Some("1.0"));
        let undo = BindingActions::decline(&store, "a").expect("мод есть");
        assert_eq!(undo.bindings.len(), 1);
        assert_eq!(undo.checks.len(), 1);
        assert!(store.bindings().is_empty());
        assert!(store.get_check("a", "stmods").is_none());
        assert!(store.get_dismissed_update("a").is_none());
        assert!(store.is_checking_declined("a"));
        assert!(crate::UpdateCheckService::tracked(&store).is_empty());

        assert!(BindingActions::undo(&store, &undo));
        assert_eq!(store.bindings().len(), 1);
        assert!(store.get_check("a", "stmods").is_some());
        assert_eq!(store.get_dismissed_update("a").as_deref(), Some("1.0"));
        assert!(!store.is_checking_declined("a"));
        assert_eq!(crate::UpdateCheckService::tracked(&store).len(), 1);
    }

    #[test]
    fn decline_missing_mod_returns_none_and_resume_clears_flag() {
        let store = InMemoryLibraryStore::new();
        assert!(BindingActions::decline(&store, "нет").is_none());
        assert_eq!(BindingActions::decline_checking(&store, "нет"), 0);

        store.upsert_mod(mod_with("a", "a.zip", "a"));
        store.set_checking_declined("a", true);
        BindingActions::resume_checking(&store, "a");
        assert!(!store.is_checking_declined("a"));
    }

    #[test]
    fn undo_does_not_overwrite_new_binding_on_same_source() {
        let store = InMemoryLibraryStore::new();
        add(&store, "a");
        let undo = BindingActions::decline(&store, "a").unwrap();
        store
            .upsert_binding(ModBinding {
                mod_id: "a".to_string(),
                source: "stmods".to_string(),
                source_id: "new".to_string(),
                url: Url::parse("https://stmods.test/new").unwrap(),
                confirmed: true,
            })
            .unwrap();
        assert!(BindingActions::undo(&store, &undo));
        assert_eq!(store.bindings()[0].source_id, "new");
    }

    #[test]
    fn dead_lists_only_confirmed_with_two_misses() {
        let store = InMemoryLibraryStore::new();
        add(&store, "b");
        store.upsert_mod(mod_with("c", "c.zip", "a"));
        store
            .upsert_binding(ModBinding {
                mod_id: "c".to_string(),
                source: "stmods".to_string(),
                source_id: "c".to_string(),
                url: Url::parse("https://stmods.test/c").unwrap(),
                confirmed: true,
            })
            .unwrap();
        store.save_check(CheckState {
            mod_id: "c".to_string(),
            source: "stmods".to_string(),
            last_checked_utc: now(),
            last_changed_utc: None,
            last_version: Some("1".to_string()),
            last_size: None,
            miss_count: 1,
            last_game_version: None,
            last_title: None,
        });
        let dead = BindingRepair::dead(&store);
        assert_eq!(dead.len(), 1);
        assert_eq!(dead[0].installed.id, "b");
        assert_eq!(dead[0].misses, 3);
    }

    #[test]
    fn repair_apply_replaces_dead_page_and_rejects_stale_choice() {
        let store = InMemoryLibraryStore::new();
        add(&store, "a");
        let mut deads = BindingRepair::dead(&store);
        let dead = deads.remove(0);
        let chosen = listing("stmods", "new", "A");

        // Та же мёртвая страница не принимается.
        assert!(!BindingRepair::apply(
            &store,
            &dead,
            &listing("stmods", "a", "A")
        ));

        assert!(BindingRepair::apply(&store, &dead, &chosen));
        assert!(store.get_check("a", "stmods").is_none(), "промахи ушли");
        assert_eq!(store.bindings()[0].source_id, "new");
        assert!(store.bindings()[0].confirmed);

        // Повторное применение устарело: привязка уже другая.
        assert!(!BindingRepair::apply(
            &store,
            &dead,
            &listing("modland", "x", "A")
        ));
    }

    #[test]
    fn repair_rejects_choice_on_another_live_source() {
        let store = InMemoryLibraryStore::new();
        add(&store, "a");
        store
            .upsert_binding(ModBinding {
                mod_id: "a".to_string(),
                source: "modland".to_string(),
                source_id: "live".to_string(),
                url: Url::parse("https://modland.test/live").unwrap(),
                confirmed: true,
            })
            .unwrap();
        let dead = BindingRepair::dead(&store).swap_remove(0);
        assert!(!BindingRepair::apply(
            &store,
            &dead,
            &listing("modland", "other", "A")
        ));
    }

    #[test]
    fn without_bound_drops_duplicates_and_bound_pages() {
        let bound = vec![ModBinding {
            mod_id: "a".to_string(),
            source: "stmods".to_string(),
            source_id: "1".to_string(),
            url: Url::parse("https://stmods.test/1").unwrap(),
            confirmed: true,
        }];
        let listings = vec![
            listing("stmods", "1", "A"),
            listing("stmods", "1", "A duplicate"),
            listing("stmods", "2", "A"),
        ];
        let left = BindingSuggester::without_bound(listings, &bound);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].source_id, "2");
    }
}

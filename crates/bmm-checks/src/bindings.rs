//! Привязки: отказ «Не проверять» и потерянные страницы.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use bmm_domain::filename::from_file_name;
use bmm_domain::library::{CheckState, InstalledMod, LibraryStore, ModBinding};

use crate::planner::UpdateCheckPlanner;

/// Единая защёлка для действий, которые меняют несколько записей хранилища разом.
/// Хранилище защищает каждую запись отдельно; здесь важно, чтобы «прочитать —
/// решить — записать» не перемешалось с параллельным действием.
static GATE: Mutex<()> = Mutex::new(());

pub(crate) fn gate() -> std::sync::MutexGuard<'static, ()> {
    GATE.lock().unwrap_or_else(|error| error.into_inner())
}

/// Чем искать мод на сайтах: название из имени файла, иначе запись библиотеки.
pub fn query_for(installed: &InstalledMod) -> String {
    let guessed = from_file_name(&installed.file_name).name;
    if guessed.trim().is_empty() {
        installed.name.clone()
    } else {
        guessed
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

/// Потерянные страницы: их показывает «Обновления», замену человек указывает сам.
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, TimeDelta};
    use url::Url;

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
            last_file: None,
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
            last_file: None,
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
}

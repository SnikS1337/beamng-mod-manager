//! Одинаковые и повторяющиеся моды среди включённых.
//!
//! Игра загружает каждый архив из папки модов: две копии одного файла
//! («weather_fx.zip» и «weather_fx (1).zip») или две версии одной машины
//! работают одновременно, и какие файлы победят — непредсказуемо. Сравниваем
//! по составу архива (папки второго уровня, см. [`crate::incoming::content_keys`]),
//! не открывая файлы заново: состав кэшируется вместе с названием.

use std::collections::{BTreeMap, HashMap, HashSet};

/// Мод для сравнения.
#[derive(Debug, Clone)]
pub struct DupItem<'a> {
    pub id: &'a str,
    pub file_name: &'a str,
    pub size: i64,
    /// Состав архива: «vehicles/rs3», «lua/ge», …
    pub content: &'a [String],
}

/// Мод повторяет другой: `exact` — та же копия (состав и размер совпали),
/// иначе другая версия тех же машин или карт.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub of: String,
    pub exact: bool,
}

/// Повторы среди переданных модов (передавайте только включённые: выключенная копия
/// игре не мешает). Основным в группе остаётся мод с самым «чистым» именем файла —
/// без « (1)» и меток раздачи, самым коротким; остальные указывают на него.
pub fn find(items: &[DupItem]) -> HashMap<String, Duplicate> {
    let mut result = HashMap::new();

    let mut exact: BTreeMap<(i64, Vec<String>), Vec<&DupItem>> = BTreeMap::new();
    for item in items {
        if item.size <= 0 || item.content.is_empty() {
            continue;
        }
        let mut content: Vec<String> = item.content.iter().map(|key| key.to_lowercase()).collect();
        content.sort();
        content.dedup();
        exact.entry((item.size, content)).or_default().push(item);
    }
    for group in exact.values().filter(|group| group.len() > 1) {
        let main = main_of(group);
        for item in group.iter().filter(|item| item.id != main.id) {
            result.insert(
                item.id.to_string(),
                Duplicate {
                    of: main.id.to_string(),
                    exact: true,
                },
            );
        }
    }

    let mut same_cars: BTreeMap<Vec<String>, Vec<&DupItem>> = BTreeMap::new();
    for item in items {
        if result.contains_key(item.id) {
            continue; // уже копия — второй раз не помечаем
        }
        let keys: HashSet<String> = item.content.iter().map(|key| key.to_lowercase()).collect();
        let mut identity: Vec<String> = crate::incoming::identity_keys(&keys).into_iter().collect();
        if identity.is_empty() {
            continue;
        }
        identity.sort();
        same_cars.entry(identity).or_default().push(item);
    }
    for group in same_cars.values().filter(|group| group.len() > 1) {
        mark_versions(group, &mut result);
    }

    // Скрипты и эффекты без своих машин и карт: тот же состав архива и то же имя
    // («weather_fx_tg_m0dsbeamng.zip», перепакованный каналом, рядом с «weather_fx.zip»).
    // Одного состава мало: «lua/ge» есть у сотен разных модов.
    let mut same_scripts: BTreeMap<(Vec<String>, String), Vec<&DupItem>> = BTreeMap::new();
    for item in items {
        if result.contains_key(item.id) || item.content.is_empty() {
            continue;
        }
        let name = crate::incoming::name_key(item.file_name);
        if name.is_empty() {
            continue;
        }
        let mut content: Vec<String> = item.content.iter().map(|key| key.to_lowercase()).collect();
        content.sort();
        content.dedup();
        same_scripts.entry((content, name)).or_default().push(item);
    }
    for group in same_scripts.values().filter(|group| group.len() > 1) {
        // Основной мог стать копией в первом проходе — тогда он уже указан у других.
        mark_versions(group, &mut result);
    }
    result
}

fn mark_versions(group: &[&DupItem], result: &mut HashMap<String, Duplicate>) {
    let main = main_of(group);
    for item in group.iter().filter(|item| item.id != main.id) {
        result.entry(item.id.to_string()).or_insert(Duplicate {
            of: main.id.to_string(),
            exact: false,
        });
    }
}

fn main_of<'a>(group: &[&'a DupItem<'a>]) -> &'a DupItem<'a> {
    group
        .iter()
        .copied()
        .min_by_key(|item| {
            let lower = item.file_name.to_lowercase();
            (is_copy(&lower), lower.chars().count(), lower)
        })
        .expect("группа не пустая")
}

/// «weather_fx (1).zip» — повторная загрузка браузером.
fn is_copy(lower_name: &str) -> bool {
    let stem = lower_name
        .rsplit_once('.')
        .map_or(lower_name, |(stem, _)| stem);
    stem.trim_end()
        .strip_suffix(')')
        .and_then(|rest| rest.rsplit_once('('))
        .is_some_and(|(_, number)| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|key| key.to_string()).collect()
    }

    #[test]
    fn browser_copies_point_to_the_clean_name() {
        // Реальный набор: weather_fx в четырёх копиях, acm_percheron в двух.
        let weather = keys(&["lua/ge", "scripts/weather_fx"]);
        let percheron = keys(&["vehicles/percheron", "vehicles/eu_trailers"]);
        let items = [
            DupItem {
                id: "w1",
                file_name: "weather_fx (1).zip",
                size: 700_000,
                content: &weather,
            },
            DupItem {
                id: "w2",
                file_name: "weather_fx_tg_m0dsbeamng.zip",
                size: 700_000,
                content: &weather,
            },
            DupItem {
                id: "w3",
                file_name: "weather_fx.zip",
                size: 700_000,
                content: &weather,
            },
            DupItem {
                id: "w4",
                file_name: "weather_fx_modland.zip",
                size: 700_000,
                content: &weather,
            },
            DupItem {
                id: "p1",
                file_name: "acm_percheron_addon_GR_v1-0 (1).zip",
                size: 800_000,
                content: &percheron,
            },
            DupItem {
                id: "p2",
                file_name: "acm_percheron_addon_GR_v1-0.zip",
                size: 800_000,
                content: &percheron,
            },
        ];
        let found = find(&items);
        for id in ["w1", "w2", "w4"] {
            assert_eq!(
                found[id],
                Duplicate {
                    of: "w3".to_string(),
                    exact: true
                },
                "{id}"
            );
        }
        assert!(!found.contains_key("w3"));
        assert_eq!(
            found["p1"],
            Duplicate {
                of: "p2".to_string(),
                exact: true
            }
        );
        assert!(!found.contains_key("p2"));
    }

    #[test]
    fn repacked_script_with_the_same_name_is_a_second_version() {
        // weather_fx_tg_m0dsbeamng.zip перепакован каналом: размер другой, состав тот же.
        let weather = keys(&["lua/ge", "scripts/weather_fx"]);
        let items = [
            DupItem {
                id: "w",
                file_name: "weather_fx.zip",
                size: 700_000,
                content: &weather,
            },
            DupItem {
                id: "w1",
                file_name: "weather_fx (1).zip",
                size: 700_000,
                content: &weather,
            },
            DupItem {
                id: "tg",
                file_name: "weather_fx_tg_m0dsbeamng.zip",
                size: 690_000,
                content: &weather,
            },
        ];
        let found = find(&items);
        assert_eq!(
            found["w1"],
            Duplicate {
                of: "w".to_string(),
                exact: true
            }
        );
        assert_eq!(
            found["tg"],
            Duplicate {
                of: "w".to_string(),
                exact: false
            }
        );
        assert!(!found.contains_key("w"));

        // Разные скрипты с одинаково бедным составом — не повторы.
        let ge = keys(&["lua/ge"]);
        let others = [
            DupItem {
                id: "a",
                file_name: "traffic_tool.zip",
                size: 10,
                content: &ge,
            },
            DupItem {
                id: "b",
                file_name: "camera_tweaks.zip",
                size: 20,
                content: &ge,
            },
        ];
        assert!(find(&others).is_empty());
    }

    #[test]
    fn two_versions_of_one_car_are_marked() {
        let rs3 = keys(&["vehicles/rs3y8", "vehicles/common"]);
        let items = [
            DupItem {
                id: "old",
                file_name: "Audi_RS3_v1.0.zip",
                size: 100,
                content: &rs3,
            },
            DupItem {
                id: "new",
                file_name: "Audi_RS3_v1.1.zip",
                size: 120,
                content: &rs3,
            },
        ];
        let found = find(&items);
        assert_eq!(found.len(), 1);
        let (id, duplicate) = found.iter().next().unwrap();
        assert!(!duplicate.exact);
        assert_ne!(id, &duplicate.of);
    }

    #[test]
    fn stock_car_tunings_and_different_mods_are_not_duplicates() {
        let tuning_a = keys(&["vehicles/etk800", "vehicles/pickup"]);
        let tuning_b = keys(&["vehicles/etk800", "vehicles/pickup", "lua/ge"]);
        let car = keys(&["vehicles/rs3y8"]);
        let other = keys(&["vehicles/laurel"]);
        let items = [
            DupItem {
                id: "a",
                file_name: "a.zip",
                size: 10,
                content: &tuning_a,
            },
            DupItem {
                id: "b",
                file_name: "b.zip",
                size: 20,
                content: &tuning_b,
            },
            DupItem {
                id: "c",
                file_name: "c.zip",
                size: 30,
                content: &car,
            },
            DupItem {
                id: "d",
                file_name: "d.zip",
                size: 40,
                content: &other,
            },
            DupItem {
                id: "e",
                file_name: "e.zip",
                size: 50,
                content: &[],
            },
            DupItem {
                id: "f",
                file_name: "f.zip",
                size: 50,
                content: &[],
            },
        ];
        assert!(find(&items).is_empty());
    }

    #[test]
    fn copy_suffix_is_recognised() {
        assert!(is_copy("weather_fx (1).zip"));
        assert!(is_copy("car (12).zip"));
        assert!(!is_copy("lada 2106 (remaster).zip"));
        assert!(!is_copy("car.zip"));
    }
}

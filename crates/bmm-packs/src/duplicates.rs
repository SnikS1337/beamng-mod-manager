//! Одинаковые и повторяющиеся моды и их группы.
//!
//! Игра загружает каждый архив из папки модов: две копии одного файла
//! («weather_fx.zip» и «weather_fx (1).zip») или две версии одной машины
//! работают одновременно, и какие файлы победят — непредсказуемо. Сравниваем
//! по составу архива (папки второго уровня, см. [`crate::incoming::content_keys`]),
//! не открывая файлы заново: состав кэшируется вместе с названием.
//!
//! Повторы собираются в группу; в списке модов группа — одна карточка самого
//! свежего мода с числом копий.

use std::collections::{BTreeMap, HashMap, HashSet};

use bmm_domain::versioning::{VersionComparer, VersionOrder};

/// Мод для сравнения.
#[derive(Debug, Clone)]
pub struct DupItem<'a> {
    pub id: &'a str,
    pub file_name: &'a str,
    pub size: i64,
    /// Состав архива: «vehicles/rs3», «lua/ge», …
    pub content: &'a [String],
    pub version: Option<&'a str>,
    /// Время изменения файла (секунды), чтобы выбрать самый свежий.
    pub modified: Option<i64>,
}

/// Мод повторяет другой — основной в своей группе. `exact` — та же копия
/// (состав и размер совпали), иначе другая версия тех же машин, карт или скрипта.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub of: String,
    pub exact: bool,
}

/// Повторы среди переданных модов: для каждого неосновного — на какой основной
/// он указывает. Основной в группе — самый свежий: старшая версия, затем более новый
/// файл, затем самое чистое имя (без « (1)» и меток раздачи).
pub fn find(items: &[DupItem]) -> HashMap<String, Duplicate> {
    let mut groups = UnionFind::new(items.len());
    let normalized: Vec<Vec<String>> = items
        .iter()
        .map(|item| {
            let mut content: Vec<String> =
                item.content.iter().map(|key| key.to_lowercase()).collect();
            content.sort();
            content.dedup();
            content
        })
        .collect();

    // 1. Та же копия: тот же состав и тот же размер.
    let mut exact: BTreeMap<(i64, &Vec<String>), Vec<usize>> = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        if item.size > 0 && !normalized[index].is_empty() {
            exact
                .entry((item.size, &normalized[index]))
                .or_default()
                .push(index);
        }
    }
    join_all(&mut groups, exact.values());

    // 2. Те же свои машины или карты — другая версия того же мода.
    let mut cars: BTreeMap<Vec<String>, Vec<usize>> = BTreeMap::new();
    for (index, content) in normalized.iter().enumerate() {
        let keys: HashSet<String> = content.iter().cloned().collect();
        let mut identity: Vec<String> = crate::incoming::identity_keys(&keys).into_iter().collect();
        if identity.is_empty() {
            continue;
        }
        identity.sort();
        cars.entry(identity).or_default().push(index);
    }
    join_all(&mut groups, cars.values());

    // 3. Скрипты и эффекты без своих машин: тот же состав и то же имя файла
    // («weather_fx_tg_m0dsbeamng.zip», перепакованный каналом). Одного состава мало:
    // «lua/ge» есть у сотен разных модов.
    let mut scripts: BTreeMap<(&Vec<String>, String), Vec<usize>> = BTreeMap::new();
    for (index, item) in items.iter().enumerate() {
        if normalized[index].is_empty() {
            continue;
        }
        let name = crate::incoming::name_key(item.file_name);
        if !name.is_empty() {
            scripts
                .entry((&normalized[index], name))
                .or_default()
                .push(index);
        }
    }
    join_all(&mut groups, scripts.values());

    let mut members: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for index in 0..items.len() {
        members.entry(groups.root(index)).or_default().push(index);
    }
    let mut result = HashMap::new();
    for group in members.values().filter(|group| group.len() > 1) {
        let main = *group
            .iter()
            .min_by(|a, b| freshness(&items[**a], &items[**b]))
            .expect("группа не пустая");
        for &index in group.iter().filter(|index| **index != main) {
            result.insert(
                items[index].id.to_string(),
                Duplicate {
                    of: items[main].id.to_string(),
                    exact: items[index].size == items[main].size
                        && normalized[index] == normalized[main],
                },
            );
        }
    }
    result
}

fn join_all<'a>(groups: &mut UnionFind, lists: impl Iterator<Item = &'a Vec<usize>>) {
    for list in lists {
        for pair in list.windows(2) {
            groups.join(pair[0], pair[1]);
        }
    }
}

/// Порядок «свежее — раньше».
fn freshness(a: &DupItem, b: &DupItem) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match VersionComparer::compare(a.version, b.version) {
        VersionOrder::Newer => return Ordering::Greater, // у `b` версия старше
        VersionOrder::Older => return Ordering::Less,
        _ => {}
    }
    // Без даты — как самый старый: порядок должен быть полным, иначе выбор зависит
    // от того, в каком порядке пришли моды.
    let (x, y) = (
        a.modified.unwrap_or(i64::MIN),
        b.modified.unwrap_or(i64::MIN),
    );
    if x != y {
        return y.cmp(&x);
    }
    let (left, right) = (a.file_name.to_lowercase(), b.file_name.to_lowercase());
    (is_copy(&left), left.chars().count(), &left).cmp(&(
        is_copy(&right),
        right.chars().count(),
        &right,
    ))
}

struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn root(&mut self, mut index: usize) -> usize {
        while self.parent[index] != index {
            self.parent[index] = self.parent[self.parent[index]];
            index = self.parent[index];
        }
        index
    }

    fn join(&mut self, a: usize, b: usize) {
        let (a, b) = (self.root(a), self.root(b));
        if a != b {
            self.parent[b] = a;
        }
    }
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

/// Что за мод по составу архива: своя машина, своя карта или дополнение
/// (детали для машин игры, скрипт, погода, звук).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Vehicle,
    Map,
    Addon,
}

impl Role {
    pub fn code(self) -> &'static str {
        match self {
            Role::Vehicle => "vehicle",
            Role::Map => "map",
            Role::Addon => "addon",
        }
    }
}

/// `None` — состав пуст (архив ещё не прочитан или не читается).
pub fn role_of(content: &[String]) -> Option<Role> {
    role_of_with_hint(content, None)
}

/// Как [`role_of`], но с подсказкой из `mod_info` (`tag_line`). Карьерные паки
/// (`gameplay/`) и человечки-реквизит (ragdoll/humanoid) — дополнения, даже если
/// внутри лежит папка `vehicles/`.
pub fn role_of_with_hint(content: &[String], hint: Option<&str>) -> Option<Role> {
    if content.is_empty() {
        return None;
    }
    let keys: HashSet<String> = content.iter().map(|key| key.to_lowercase()).collect();
    if keys.iter().any(|key| key.starts_with("gameplay/")) || hint.is_some_and(is_humanoid_hint) {
        return Some(Role::Addon);
    }
    let identity = crate::incoming::identity_keys(&keys);
    if identity.iter().any(|key| key.starts_with("vehicles/")) {
        Some(Role::Vehicle)
    } else if identity.iter().any(|key| key.starts_with("levels/")) {
        Some(Role::Map)
    } else {
        Some(Role::Addon)
    }
}

fn is_humanoid_hint(hint: &str) -> bool {
    let hint = hint.to_lowercase();
    ["ragdoll", "humanoid", "pedestrian", "crash test dummy"]
        .iter()
        .any(|word| hint.contains(word))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|key| key.to_string()).collect()
    }

    fn item<'a>(id: &'a str, file: &'a str, size: i64, content: &'a [String]) -> DupItem<'a> {
        DupItem {
            id,
            file_name: file,
            size,
            content,
            version: None,
            modified: None,
        }
    }

    #[test]
    fn copies_and_repacks_form_one_group_around_the_freshest() {
        // Реальный набор: weather_fx в четырёх файлах, один перепакован каналом.
        let weather = keys(&["lua/ge", "scripts/weather_fx"]);
        let mut items = vec![
            item("w1", "weather_fx (1).zip", 700_000, &weather),
            item("tg", "weather_fx_tg_m0dsbeamng.zip", 690_000, &weather),
            item("w", "weather_fx.zip", 700_000, &weather),
            item("ml", "weather_fx_modland.zip", 700_000, &weather),
        ];
        let found = find(&items);
        assert_eq!(found.len(), 3, "одна группа, один основной");
        assert!(
            !found.contains_key("w"),
            "без даты основной — самое чистое имя"
        );
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

        // Свежий файл становится основным.
        items[1].modified = Some(200);
        items[2].modified = Some(100);
        let found = find(&items);
        assert!(!found.contains_key("tg"));
        assert_eq!(
            found["w"],
            Duplicate {
                of: "tg".to_string(),
                exact: false
            }
        );
    }

    #[test]
    fn newer_version_is_the_main_one() {
        let rs3 = keys(&["vehicles/rs3y8", "vehicles/common"]);
        let mut old = item("old", "Audi_RS3_v1.0.zip", 100, &rs3);
        old.version = Some("1.0");
        old.modified = Some(500);
        let mut new = item("new", "Audi_RS3_v1.1.zip", 120, &rs3);
        new.version = Some("1.1");
        new.modified = Some(100);
        let found = find(&[old, new]);
        assert_eq!(
            found["old"],
            Duplicate {
                of: "new".to_string(),
                exact: false
            }
        );
    }

    #[test]
    fn stock_car_tunings_and_different_mods_are_not_duplicates() {
        let tuning_a = keys(&["vehicles/etk800", "vehicles/pickup"]);
        let tuning_b = keys(&["vehicles/etk800", "vehicles/pickup", "lua/ge"]);
        let car = keys(&["vehicles/rs3y8"]);
        let other = keys(&["vehicles/laurel"]);
        let ge = keys(&["lua/ge"]);
        let items = [
            item("a", "a.zip", 10, &tuning_a),
            item("b", "b.zip", 20, &tuning_b),
            item("c", "c.zip", 30, &car),
            item("d", "d.zip", 40, &other),
            item("e", "e.zip", 50, &[]),
            item("f", "f.zip", 50, &[]),
            item("t", "traffic_tool.zip", 60, &ge),
            item("k", "camera_tweaks.zip", 70, &ge),
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

    #[test]
    fn roles_by_content() {
        assert_eq!(
            role_of(&keys(&["vehicles/rs3y8", "vehicles/common"])),
            Some(Role::Vehicle)
        );
        assert_eq!(
            role_of(&keys(&["levels/poland", "art/sound"])),
            Some(Role::Map)
        );
        // Тюнинг стандартных машин, погода, двигатели — дополнения.
        assert_eq!(
            role_of(&keys(&["vehicles/etk800", "vehicles/pickup"])),
            Some(Role::Addon)
        );
        assert_eq!(
            role_of(&keys(&["lua/ge", "scripts/weather_fx"])),
            Some(Role::Addon)
        );
        assert_eq!(role_of(&[]), None);
    }

    #[test]
    fn gameplay_pack_with_vehicles_is_addon() {
        // RLS Career: в архиве есть и vehicles/, и levels/, но это карьерный оверхол.
        assert_eq!(
            role_of(&keys(&[
                "gameplay/overhaul/a.lua",
                "vehicles/etk800/a.jbeam",
                "levels/west_coast_usa/a.json",
            ])),
            Some(Role::Addon)
        );
    }

    #[test]
    fn humanoid_hint_turns_single_vehicle_into_addon() {
        // Crash Test Dummy: подпись из mod_info говорит, что это Prop, а не машина.
        assert_eq!(
            role_of_with_hint(
                &keys(&["vehicles/dummy/dummy.jbeam"]),
                Some("75KG Ragdoll Humanoid"),
            ),
            Some(Role::Addon)
        );
    }

    #[test]
    fn ordinary_vehicle_ignores_unrelated_hint() {
        assert_eq!(
            role_of_with_hint(&keys(&["vehicles/rs3y8/a.jbeam"]), Some("Sport sedan"),),
            Some(Role::Vehicle)
        );
    }
}

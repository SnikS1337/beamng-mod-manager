use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use serde_json::Value;

use crate::zip_index::{self, ZipError};

const MAX_INFO_BYTES: u64 = 256 * 1024;
const MAX_TITLE: usize = 80;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchiveOverview {
    pub vehicles: Vec<String>,
    pub levels: Vec<String>,
    pub has_scripts: bool,
    pub lua_files: usize,
    pub files: usize,
    pub unpacked_bytes: u64,
    pub warnings: Vec<String>,
    pub unreadable: bool,
    pub title: Option<String>,
}

impl ArchiveOverview {
    pub fn is_pack(&self) -> bool {
        self.vehicles.len() + self.levels.len() >= 2
    }
}

pub fn read_overview(path: &Path) -> ArchiveOverview {
    let index = match zip_index::read_index(path) {
        Ok(index) => index,
        Err(ZipError::Zip(_)) => {
            return ArchiveOverview {
                unreadable: true,
                warnings: vec!["Архив не читается: возможно, он повреждён.".to_string()],
                ..ArchiveOverview::default()
            };
        }
        Err(error) => {
            return ArchiveOverview {
                unreadable: true,
                warnings: vec![error.to_string()],
                ..ArchiveOverview::default()
            };
        }
    };

    let prefix = wrapper_prefix(&index.entries);
    let mut vehicles = BTreeSet::new();
    let mut levels = BTreeSet::new();
    let mut lua_files = 0;
    let mut unpacked_bytes = 0u64;

    for entry in &index.entries {
        let name = entry.name.strip_prefix(&prefix).unwrap_or(&entry.name);
        let parts: Vec<&str> = name.split('/').collect();
        match parts.as_slice() {
            [root, vehicle, rest @ ..]
                if root.eq_ignore_ascii_case("vehicles") && !rest.is_empty() =>
            {
                if !vehicle.eq_ignore_ascii_case("common") && !vehicle.is_empty() {
                    vehicles.insert(vehicle.to_string());
                }
            }
            [root, level, rest @ ..]
                if root.eq_ignore_ascii_case("levels") && !rest.is_empty() && !level.is_empty() =>
            {
                levels.insert(level.to_string());
            }
            _ => {}
        }
        if name.to_lowercase().ends_with(".lua") {
            lua_files += 1;
        }
        unpacked_bytes = unpacked_bytes.saturating_add(entry.size);
    }

    let mut warnings = Vec::new();
    let vehicle_titles = titles_for(path, &prefix, "vehicles", &vehicles);
    let level_titles = titles_for(path, &prefix, "levels", &levels);
    // Название мода — только из его info.json. Имя папки («am», «etkhatch») —
    // служебное: лучше имя файла, чем такое.
    // Правка стандартной машины несёт её info.json — это название машины игры, не мода.
    let named_title = if vehicles.len() == 1 && levels.is_empty() {
        vehicles
            .iter()
            .next()
            .filter(|name| !crate::stock::is_stock_vehicle(name))
            .and_then(|name| vehicle_titles.get(name).cloned())
    } else if levels.len() == 1 && vehicles.is_empty() {
        levels
            .iter()
            .next()
            .filter(|name| !crate::stock::is_stock_level(name))
            .and_then(|name| level_titles.get(name).cloned())
    } else {
        None
    };
    let vehicles: Vec<String> = vehicles
        .iter()
        .map(|name| {
            vehicle_titles
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.clone())
        })
        .collect();
    let levels: Vec<String> = levels
        .iter()
        .map(|name| {
            level_titles
                .get(name)
                .cloned()
                .unwrap_or_else(|| name.clone())
        })
        .collect();

    let title = named_title;

    if index.entries.is_empty() {
        warnings.push("Архив пуст или не читается.".to_string());
    }

    ArchiveOverview {
        vehicles,
        levels,
        has_scripts: lua_files > 0,
        lua_files,
        files: index.entries.len(),
        unpacked_bytes,
        warnings,
        unreadable: false,
        title,
    }
}

/// Что архив говорит о себе: название и автор из `info.json` его единственной машины
/// или карты.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ArchiveInfo {
    pub title: Option<String>,
    pub author: Option<String>,
}

/// Название мода из его архива — для карточки и для поиска страницы на сайтах.
///
/// Только когда мод кладёт ровно одну свою машину (или одну свою карту) и в её
/// `info.json` есть название: «Brand Name» машины или `title` карты. Пак из
/// нескольких машин, покраска стандартной машины, мод без `info.json` — названия нет.
///
/// `Ok(None)` — названия в архиве нет (в том числе архив не zip или повреждён);
/// `Err` — архив сейчас не открыть (занят, нет доступа): спросить в другой раз.
/// Читает только оглавление и один маленький файл: годится для тысячи модов подряд.
pub fn archive_title(path: &Path) -> Result<Option<String>, std::io::Error> {
    archive_info(path).map(|info| info.title)
}

/// Название и автор из архива (см. [`archive_title`]). Автор нужен, чтобы не принять
/// за этот мод одноимённую страницу другого автора.
pub fn archive_info(path: &Path) -> Result<ArchiveInfo, std::io::Error> {
    let index = match zip_index::read_index(path) {
        Ok(index) => index,
        Err(ZipError::Io(error)) => return Err(error),
        Err(ZipError::Zip(_)) => return Ok(ArchiveInfo::default()),
    };
    let prefix = wrapper_prefix(&index.entries);
    let mut vehicles = BTreeSet::new();
    let mut levels = BTreeSet::new();
    for entry in &index.entries {
        let name = entry.name.strip_prefix(&prefix).unwrap_or(&entry.name);
        let parts: Vec<&str> = name.split('/').collect();
        if let [root, folder, rest @ ..] = parts.as_slice()
            && !rest.is_empty()
            && !folder.is_empty()
        {
            if root.eq_ignore_ascii_case("vehicles") && !folder.eq_ignore_ascii_case("common") {
                vehicles.insert(folder.to_string());
            } else if root.eq_ignore_ascii_case("levels") {
                levels.insert(folder.to_string());
            }
        }
    }
    let (root, folder) = match (vehicles.len(), levels.len()) {
        (1, 0) => ("vehicles", vehicles.first().cloned()),
        (0, 1) => ("levels", levels.first().cloned()),
        _ => return Ok(ArchiveInfo::default()),
    };
    let Some(folder) = folder else {
        return Ok(ArchiveInfo::default());
    };
    let stock = if root == "vehicles" {
        crate::stock::is_stock_vehicle(&folder)
    } else {
        crate::stock::is_stock_level(&folder)
    };
    if stock {
        return Ok(ArchiveInfo::default());
    }
    let one = BTreeSet::from([folder.clone()]);
    let title = titles_for(path, &prefix, root, &one).remove(&folder);
    let author = info_value(path, &format!("{prefix}{root}/{folder}/info.json"))
        .and_then(|value| {
            ["Author", "author", "Authors", "authors"]
                .iter()
                .find_map(|key| value.get(*key).and_then(Value::as_str).map(str::to_string))
        })
        .map(|author| author.trim().to_string())
        .filter(|author| !author.is_empty() && author.chars().count() <= MAX_TITLE);
    Ok(ArchiveInfo { title, author })
}

/// `mod_info/<ID>/info.json` из архива: так помечает мод репозиторий BeamNG (с номером
/// страницы и версией), а иногда и сам автор. То же самое игра потом пишет в db.json,
/// но читать архив можно сразу, не дожидаясь запуска игры. `None` — такой записи нет.
pub fn mod_info(path: &Path) -> Option<(String, Value)> {
    let index = zip_index::read_index(path).ok()?;
    let entry = index.entries.iter().find_map(|entry| {
        let parts: Vec<&str> = entry.name.split('/').collect();
        match parts.as_slice() {
            [root, id, file]
                if root.eq_ignore_ascii_case("mod_info")
                    && !id.is_empty()
                    && file.eq_ignore_ascii_case("info.json") =>
            {
                Some((id.to_string(), entry.name.clone()))
            }
            _ => None,
        }
    })?;
    let value = info_value(path, &entry.1)?;
    value.is_object().then_some((entry.0, value))
}

fn info_value(path: &Path, entry: &str) -> Option<Value> {
    let mut archive = crate::zip_index::open_archive(path).ok()?;
    let mut info = archive.by_name(entry).ok()?;
    if info.size() > MAX_INFO_BYTES {
        return None;
    }
    let mut text = String::new();
    info.read_to_string(&mut text).ok()?;
    json5::from_str::<Value>(&text).ok()
}

fn wrapper_prefix(entries: &[zip_index::Entry]) -> String {
    let Some(first) = entries
        .iter()
        .filter_map(|entry| entry.name.split('/').next())
        .next()
    else {
        return String::new();
    };
    let all_share = entries
        .iter()
        .all(|entry| entry.name.split('/').next() == Some(first));
    let known = [
        "vehicles", "levels", "mod_info", "ui", "scripts", "lua", "art",
    ];
    if all_share
        && !known.iter().any(|known| first.eq_ignore_ascii_case(known))
        && entries.iter().any(|entry| entry.name.contains('/'))
    {
        format!("{first}/")
    } else {
        String::new()
    }
}

fn titles_for(
    path: &Path,
    prefix: &str,
    root: &str,
    names: &BTreeSet<String>,
) -> std::collections::HashMap<String, String> {
    let mut result = std::collections::HashMap::new();
    let Ok(mut archive) = crate::zip_index::open_archive(path) else {
        return result;
    };
    for name in names {
        let entry = format!("{prefix}{root}/{name}/info.json");
        let Ok(mut info) = archive.by_name(&entry) else {
            continue;
        };
        if info.size() > MAX_INFO_BYTES {
            continue;
        }
        let mut text = String::new();
        if info.read_to_string(&mut text).is_err() {
            continue;
        }
        let Ok(value) = json5::from_str::<Value>(&text) else {
            continue;
        };
        let title = if root == "vehicles" {
            let brand = value.get("Brand").and_then(Value::as_str).unwrap_or("");
            let model = value.get("Name").and_then(Value::as_str).unwrap_or("");
            if model.is_empty() {
                None
            } else if brand.is_empty() || model.to_lowercase().starts_with(&brand.to_lowercase()) {
                Some(model.to_string())
            } else {
                Some(format!("{brand} {model}"))
            }
        } else {
            value
                .get("title")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        // «LADA 2104/2105/2107  », «Toyota  Mark II»: лишние пробелы авторов убираем.
        let title = title.map(|title| title.split_whitespace().collect::<Vec<_>>().join(" "));
        if let Some(title) = title
            && is_usable_title(&title)
        {
            result.insert(name.clone(), title);
        }
    }
    result
}

fn is_usable_title(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.chars().count() <= MAX_TITLE
        && !(trimmed.contains('.') && !trimmed.contains(' '))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    /// info.json модов пишут руками: комментарии, запятые в конце, ключи без
    /// кавычек, BOM, NaN, огромные числа. Тест фиксирует, что разбор это
    /// переживает, и ловит изменения поведения при обновлении json5.
    #[test]
    fn json5_reads_hand_written_info_files() {
        let ok = |text: &str| json5::from_str::<Value>(text).expect(text);
        assert_eq!(
            ok("{ // c\n \"Name\": \"A\", /* b */ Brand: 'B', }")["Brand"],
            "B"
        );
        assert_eq!(ok("\u{feff}{\"Name\": \"A\"}")["Name"], "A");
        assert_eq!(
            ok("{\r\n\"Years\": {\"min\": 1990, \"max\": 2000,},\r\n}\r\n")["Years"]["max"],
            2000
        );
        assert_eq!(ok("{\"d\": \"a\\\nb\", \"t\": \"a\tb\"}")["d"], "ab");
        assert!(ok("{\"a\": NaN, \"b\": Infinity}")["a"].is_null());
        assert_eq!(ok("{\"a\": .5, \"c\": 0x1F, \"d\": +3}")["c"], 31);
        assert!(ok("{\"e\": 12345678901234567890}")["e"].is_number());
        assert_eq!(ok("{\"a\": 1, \"a\": 2}")["a"], 2);
        assert_eq!(ok("{\"a\": \"\\ud83d\\ude00\"}")["a"], "😀");
        for broken in ["", "  \n", "{\"d\": \"a\nb\"}", "{\"a\": 1} x"] {
            assert!(json5::from_str::<Value>(broken).is_err(), "{broken:?}");
        }
    }

    fn build_zip(path: &Path, files: &[(&str, &[u8])]) {
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        for (name, bytes) in files {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    #[test]
    fn archive_title_of_single_vehicle_and_map() {
        let temp = tempfile::tempdir().unwrap();
        let car = temp.path().join("daciasanderomk1_tbdiesel.zip");
        build_zip(
            &car,
            &[
                ("vehicles/common/x.jbeam", b"{}"),
                ("vehicles/sandero1/sandero.jbeam", b"{}"),
                (
                    "vehicles/sandero1/info.json",
                    br#"{"Name":"Sandero (B90)", "Brand":"Dacia", /* loose json */ "Years":{"min":2008,},}"#,
                ),
                ("vehicles/sandero1/info_diesel.json", br#"{"Configuration":"TB"}"#),
            ],
        );
        let laurel = temp.path().join("laurel_c33.zip");
        build_zip(
            &laurel,
            &[(
                "vehicles/stvol_c33/info.json",
                br#"{"Author":"Ruzskye Stvoly","Brand":"Nissan","Name":"Laurel C33"}"#,
            )],
        );
        assert_eq!(
            archive_info(&laurel).unwrap(),
            ArchiveInfo {
                title: Some("Nissan Laurel C33".to_string()),
                author: Some("Ruzskye Stvoly".to_string()),
            }
        );
        assert_eq!(
            archive_title(&car).unwrap().as_deref(),
            Some("Dacia Sandero (B90)")
        );

        let map = temp.path().join("map.zip");
        build_zip(
            &map,
            &[
                ("Wrapper/levels/my_map/main.level.json", b"{}"),
                (
                    "Wrapper/levels/my_map/info.json",
                    br#"{"title":"Lake Town"}"#,
                ),
            ],
        );
        assert_eq!(archive_title(&map).unwrap().as_deref(), Some("Lake Town"));
    }

    #[test]
    fn archive_title_is_absent_for_packs_stock_tuning_and_junk() {
        let temp = tempfile::tempdir().unwrap();
        let pack = temp.path().join("pack.zip");
        build_zip(
            &pack,
            &[
                ("vehicles/a/info.json", br#"{"Name":"A"}"#),
                ("vehicles/b/info.json", br#"{"Name":"B"}"#),
            ],
        );
        assert_eq!(archive_title(&pack).unwrap(), None);

        let tuning = temp.path().join("tuning.zip");
        build_zip(
            &tuning,
            &[(
                "vehicles/pessima/info.json",
                br#"{"Brand":"Ibishu","Name":"Pessima"}"#,
            )],
        );
        assert_eq!(
            archive_title(&tuning).unwrap(),
            None,
            "стандартная машина игры"
        );

        let no_info = temp.path().join("noinfo.zip");
        build_zip(&no_info, &[("vehicles/car/car.jbeam", b"{}")]);
        assert_eq!(archive_title(&no_info).unwrap(), None);

        let junk = temp.path().join("junk.zip");
        std::fs::write(&junk, b"not a zip").unwrap();
        assert_eq!(archive_title(&junk).unwrap(), None);

        assert!(archive_title(&temp.path().join("missing.zip")).is_err());
    }

    #[test]
    fn reads_vehicle_and_level_with_info() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        build_zip(
            &path,
            &[
                (
                    "vehicles/car/info.json",
                    br#"{"Brand":"Gavril","Name":"D-Series"}"#,
                ),
                ("vehicles/car/main.lua", b"x"),
                ("levels/map/info.json", br#"{"title":"Jungle Run"}"#),
            ],
        );
        let overview = read_overview(&path);
        assert_eq!(overview.vehicles, vec!["Gavril D-Series"]);
        assert_eq!(overview.levels, vec!["Jungle Run"]);
        assert!(overview.has_scripts);
        assert_eq!(overview.lua_files, 1);
        assert_eq!(overview.files, 3);
        assert!(overview.is_pack());
        assert_eq!(overview.title, None);
    }

    #[test]
    fn single_vehicle_gets_title() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        build_zip(
            &path,
            &[(
                "vehicles/car/info.json",
                br#"{"Brand":"Ibishu","Name":"Covet"}"#,
            )],
        );
        let overview = read_overview(&path);
        assert_eq!(overview.title.as_deref(), Some("Ibishu Covet"));
    }

    #[test]
    fn reads_mod_info_left_by_the_repository() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("dummy.zip");
        build_zip(
            &path,
            &[
                ("vehicles/dummy/a.jbeam", b"x"),
                (
                    "mod_info/M9EL2DOCR/info.json",
                    br#"{"tagid":"M9EL2DOCR","title":"Crash Test Dummy","version_string":"1.1","resource_id":906}"#,
                ),
                ("mod_info/M9EL2DOCR/icon.jpg", b"x"),
            ],
        );
        let (id, value) = mod_info(&path).expect("mod_info есть");
        assert_eq!(id, "M9EL2DOCR");
        assert_eq!(value["resource_id"], 906);

        let plain = temp.path().join("plain.zip");
        build_zip(&plain, &[("vehicles/car/a.jbeam", b"x")]);
        assert!(mod_info(&plain).is_none());
    }

    #[test]
    fn folder_name_is_not_a_title() {
        // AstonMartinValhalla.zip: vehicles/am без названия в info.json.
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("AstonMartinValhalla.zip");
        build_zip(
            &path,
            &[
                ("vehicles/am/a.jbeam", b"x"),
                ("vehicles/am/info.json", br#"{"Author":"x"}"#),
            ],
        );
        assert_eq!(read_overview(&path).title, None);
    }

    #[test]
    fn extra_spaces_in_titles_are_removed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mark2.zip");
        build_zip(
            &path,
            &[(
                "vehicles/mark2/info.json",
                br#"{"Name":"  Toyota  Mark II (X90)  "}"#,
            )],
        );
        assert_eq!(
            read_overview(&path).title.as_deref(),
            Some("Toyota Mark II (X90)")
        );
        assert_eq!(
            archive_title(&path).unwrap().as_deref(),
            Some("Toyota Mark II (X90)")
        );
    }

    #[test]
    fn stock_car_edit_has_no_title() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("pickup_tune.zip");
        build_zip(
            &path,
            &[(
                "vehicles/pickup/info.json",
                br#"{"Brand":"Gavril","Name":"D-Series"}"#,
            )],
        );
        assert_eq!(read_overview(&path).title, None);
    }

    #[test]
    fn wrapper_folder_and_common_are_handled() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("mod.zip");
        build_zip(
            &path,
            &[
                ("wrap/vehicles/common/info.json", b"{}"),
                ("wrap/vehicles/car/info.json", b"{}"),
                ("wrap/levels/map/info.json", b"{}"),
            ],
        );
        let overview = read_overview(&path);
        assert_eq!(overview.vehicles, vec!["car"]);
        assert_eq!(overview.levels, vec!["map"]);
        assert!(overview.is_pack());
    }

    #[test]
    fn broken_zip_is_unreadable() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("bad.zip");
        std::fs::write(&path, b"not a zip").unwrap();
        let overview = read_overview(&path);
        assert!(overview.unreadable);
    }
}

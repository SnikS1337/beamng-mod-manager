use std::collections::BTreeSet;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde_json::Value;
use zip::ZipArchive;

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

    let title = if vehicles.len() == 1 && levels.is_empty() {
        Some(vehicles[0].clone())
    } else if levels.len() == 1 && vehicles.is_empty() {
        Some(levels[0].clone())
    } else {
        None
    };

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

fn info_value(path: &Path, entry: &str) -> Option<Value> {
    let file = File::open(path).ok()?;
    let mut archive = ZipArchive::new(file).ok()?;
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
    let Ok(file) = File::open(path) else {
        return result;
    };
    let Ok(mut archive) = ZipArchive::new(file) else {
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
    use std::io::Write;

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

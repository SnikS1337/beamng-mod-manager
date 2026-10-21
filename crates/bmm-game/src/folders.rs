use std::fs;
use std::path::{Path, PathBuf};

use bmm_domain::settings::AppSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderProblem {
    None,
    Empty,
    NotFound,
    NotADirectory,
    NoModsFolder,
    NotWritable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderCheck {
    pub problem: FolderProblem,
    pub mods_dir: Option<PathBuf>,
    pub message: String,
}

impl FolderCheck {
    pub fn ok(&self) -> bool {
        self.problem == FolderProblem::None
    }
}

pub fn default_user_roots() -> Vec<PathBuf> {
    let Some(local) = std::env::var_os("LOCALAPPDATA") else {
        return Vec::new();
    };
    let local = PathBuf::from(local);
    vec![
        local.join("BeamNG").join("BeamNG.drive"),
        local.join("BeamNG.drive"),
    ]
}

pub fn default_user_root() -> PathBuf {
    let roots = default_user_roots();
    roots
        .iter()
        .find(|root| root.is_dir())
        .cloned()
        .unwrap_or_else(|| roots.into_iter().next().unwrap_or_default())
}

pub fn find_mods_folders(user_root: &Path) -> Vec<PathBuf> {
    if !user_root.is_dir() {
        return Vec::new();
    }
    let mut result: Vec<PathBuf> = Vec::new();
    let mut add = |path: PathBuf| {
        if path.is_dir() && !result.iter().any(|existing| same_path(existing, &path)) {
            result.push(path);
        }
    };

    add(user_root.join("mods"));
    add(user_root.join("current").join("mods"));

    let mut versions: Vec<(String, PathBuf)> = fs::read_dir(user_root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            version_key(&name).map(|key| (key, entry.path()))
        })
        .collect();
    versions.sort_by(|a, b| b.0.cmp(&a.0));
    for (_, dir) in versions {
        add(dir.join("mods"));
    }

    if result.is_empty() {
        let nested = user_root.join("BeamNG.drive");
        if nested.is_dir() {
            result.extend(find_mods_folders(&nested));
        }
    }
    result
}

pub fn auto_detect(game_dirs: &[PathBuf], user_roots: &[PathBuf]) -> Option<PathBuf> {
    let mut roots: Vec<PathBuf> = game_dirs
        .iter()
        .filter_map(|dir| crate::info::user_root_from_startup_ini(dir))
        .collect();
    roots.extend(user_roots.iter().cloned());

    for root in roots {
        if let Some(found) = find_mods_folders(&root).into_iter().next() {
            return Some(found);
        }
    }
    None
}

pub fn clean_path(raw: &str) -> String {
    let mut path = raw.trim();
    if path.len() >= 2 {
        let bytes = path.as_bytes();
        let quoted = (bytes[0] == b'"' && bytes[path.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[path.len() - 1] == b'\'');
        if quoted {
            path = path[1..path.len() - 1].trim();
        }
    }
    expand_env_vars(path, |name| std::env::var(name).ok())
}

fn expand_env_vars(input: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut result = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        result.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('%') {
            let name = &after[..end];
            if !name.is_empty()
                && let Some(value) = lookup(name)
            {
                result.push_str(&value);
                rest = &after[end + 1..];
                continue;
            }
        }
        result.push('%');
        rest = after;
    }
    result.push_str(rest);
    result
}

pub fn resolve(selected: Option<&str>, check_writable: bool) -> FolderCheck {
    let Some(selected) = selected else {
        return check(FolderProblem::Empty, None, "Папка не выбрана.");
    };
    if selected.trim().is_empty() {
        return check(FolderProblem::Empty, None, "Папка не выбрана.");
    }
    let path = clean_path(selected);
    if path.is_empty() {
        return check(FolderProblem::Empty, None, "Папка не выбрана.");
    }
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return check(
            FolderProblem::NotFound,
            None,
            "Нужен полный путь к папке (например, C:\\Users\\...).",
        );
    }
    if path.is_file() {
        return check(
            FolderProblem::NotADirectory,
            None,
            "Выбран файл, а нужна папка.",
        );
    }
    if !path.is_dir() {
        return check(FolderProblem::NotFound, None, "Папка не найдена.");
    }

    let mods = if path
        .file_name()
        .map(|name| name.to_string_lossy().eq_ignore_ascii_case("mods"))
        .unwrap_or(false)
    {
        path
    } else {
        match find_mods_folders(&path).into_iter().next() {
            Some(found) => found,
            None => {
                return check(
                    FolderProblem::NoModsFolder,
                    None,
                    "Внутри нет папки mods. Выберите папку mods или папку пользователя BeamNG.",
                );
            }
        }
    };

    if check_writable && !can_write(&mods) {
        return check(
            FolderProblem::NotWritable,
            Some(mods),
            "В папку mods нельзя записывать (нет прав или она только для чтения).",
        );
    }
    check(FolderProblem::None, Some(mods), "Папка подходит.")
}

pub fn from_settings(settings: &AppSettings, check_writable: bool) -> FolderCheck {
    if let Some(folder) = settings.mods_folder.as_deref() {
        return resolve(Some(folder), check_writable);
    }
    if let Some(folder) = settings.user_folder.as_deref() {
        return resolve(Some(folder), check_writable);
    }
    check(FolderProblem::Empty, None, "Папка игры не выбрана.")
}

pub fn newer_mods_folder(mods_dir: &Path) -> Option<PathBuf> {
    let name = mods_dir.file_name()?.to_string_lossy();
    if !name.eq_ignore_ascii_case("mods") {
        return None;
    }
    let version_dir = mods_dir.parent()?;
    let version_name = version_dir.file_name()?.to_string_lossy();
    let mine = version_key(&version_name)?;
    let root = version_dir.parent()?;

    let current = root.join("current").join("mods");
    if current.is_dir() {
        return Some(current);
    }

    let mut candidates: Vec<(String, PathBuf)> = fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let key = version_key(&name)?;
            let mods = entry.path().join("mods");
            if key > mine && mods.is_dir() {
                Some((key, mods))
            } else {
                None
            }
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    candidates.into_iter().next().map(|(_, path)| path)
}

fn check(problem: FolderProblem, mods_dir: Option<PathBuf>, message: &str) -> FolderCheck {
    FolderCheck {
        problem,
        mods_dir,
        message: message.to_string(),
    }
}

fn can_write(dir: &Path) -> bool {
    let probe = dir.join(format!(
        ".bmm-probe-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn version_key(name: &str) -> Option<String> {
    if name.is_empty()
        || !name
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    let mut parts: Vec<i32> = name
        .split('.')
        .take(4)
        .map(|part| part.parse::<i32>().map(|v| v.min(100_000)).unwrap_or(0))
        .collect();
    while parts.len() < 4 {
        parts.push(0);
    }
    Some(format!(
        "{:06}.{:06}.{:06}.{:06}",
        parts[0], parts[1], parts[2], parts[3]
    ))
}

fn same_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn dirs(root: &Path) -> Vec<PathBuf> {
        find_mods_folders(root)
    }

    #[test]
    fn finds_mods_in_expected_order() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("mods")).unwrap();
        fs::create_dir_all(root.join("current/mods")).unwrap();
        fs::create_dir_all(root.join("0.39/mods")).unwrap();
        fs::create_dir_all(root.join("0.100/mods")).unwrap();
        fs::create_dir_all(root.join("0.38/mods")).unwrap();
        let found = dirs(root);
        assert_eq!(found[0], root.join("mods"));
        assert_eq!(found[1], root.join("current/mods"));
        assert_eq!(found[2], root.join("0.100/mods"));
        assert_eq!(found[3], root.join("0.39/mods"));
        assert_eq!(found[4], root.join("0.38/mods"));
    }

    #[test]
    fn searches_nested_beamng_folder_when_root_is_empty() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("BeamNG.drive/mods")).unwrap();
        assert_eq!(dirs(root), vec![root.join("BeamNG.drive/mods")]);
    }

    #[test]
    fn resolves_mods_and_user_folder() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("mods")).unwrap();
        let check = resolve(Some(root.to_str().unwrap()), true);
        assert!(check.ok(), "{:?}", check);
        assert_eq!(check.mods_dir, Some(root.join("mods")));

        let check = resolve(Some(root.join("mods").join("..").to_str().unwrap()), true);
        assert!(check.ok());
    }

    #[test]
    fn resolution_errors_have_old_messages() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("file.txt");
        fs::write(&file, b"x").unwrap();

        let check = resolve(Some(file.to_str().unwrap()), true);
        assert_eq!(check.problem, FolderProblem::NotADirectory);
        assert_eq!(check.message, "Выбран файл, а нужна папка.");

        let missing = temp.path().join("missing");
        let check = resolve(Some(missing.to_str().unwrap()), true);
        assert_eq!(check.problem, FolderProblem::NotFound);

        let empty = temp.path().join("empty");
        fs::create_dir(&empty).unwrap();
        let check = resolve(Some(empty.to_str().unwrap()), true);
        assert_eq!(check.problem, FolderProblem::NoModsFolder);

        let check = resolve(None, true);
        assert_eq!(check.problem, FolderProblem::Empty);
        assert_eq!(check.message, "Папка не выбрана.");
    }

    #[test]
    fn clean_path_removes_quotes_and_expands_variables() {
        assert_eq!(clean_path(" \"C:\\Games\\BeamNG\" "), "C:\\Games\\BeamNG");
        assert_eq!(
            expand_env_vars("%ROOT%\\mods", |name| (name == "ROOT")
                .then(|| "C:\\Game".to_string())),
            "C:\\Game\\mods"
        );
        assert_eq!(expand_env_vars("100%\\mods", |_| None), "100%\\mods");
    }

    #[test]
    fn newer_folder_is_found() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let old = root.join("0.36/mods");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(root.join("0.38/mods")).unwrap();
        fs::create_dir_all(root.join("current/mods")).unwrap();
        assert_eq!(newer_mods_folder(&old), Some(root.join("current/mods")));
        fs::remove_dir_all(root.join("current")).unwrap();
        assert_eq!(newer_mods_folder(&old), Some(root.join("0.38/mods")));
    }

    #[test]
    fn from_settings_respects_explicit_folder_first() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        fs::create_dir_all(root.join("mods")).unwrap();
        let settings = AppSettings {
            mods_folder: Some(root.to_str().unwrap().to_string()),
            user_folder: Some("C:\\Missing".to_string()),
            ..AppSettings::default()
        };
        let check = from_settings(&settings, true);
        assert!(check.ok(), "{:?}", check);
        assert_eq!(
            from_settings(&AppSettings::default(), true).problem,
            FolderProblem::Empty
        );
    }
}

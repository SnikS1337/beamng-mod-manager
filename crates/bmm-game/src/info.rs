use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use fancy_regex::Regex;

fn log_version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"Log started\s*-\s*v\s*(\d+(?:\.\d+){1,3})").unwrap())
}

fn ini_version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?im)^[ \t]*version[ \t]*=[ \t]*(\d+(?:\.\d+){1,3})").unwrap())
}

fn compat_version_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?<![\d.])(\d{1,3})\.(\d{1,3})(?:\.\d+)*(?![\d])").unwrap())
}

pub fn version_from_log(log_text: &str) -> Option<String> {
    // 4096 может попасть в середину кириллического символа: берём весь лог,
    // если граница не по char boundary (шапка лога обычно короткая).
    let head = log_text.get(..log_text.len().min(4096)).unwrap_or(log_text);
    let captures = log_version_re().captures(head).ok().flatten()?;
    Some(captures.get(1)?.as_str().to_string())
}

pub fn version_from_ini(ini_text: &str) -> Option<String> {
    let captures = ini_version_re().captures(ini_text).ok().flatten()?;
    Some(captures.get(1)?.as_str().to_string())
}

pub fn detect_version(user_folder: &Path) -> Option<String> {
    // Лог пишется при каждом запуске и прямо называет версию; ini может
    // относиться к другой установке и содержит служебные числа.
    let log = user_folder.join("beamng.log");
    if let Ok(mut file) = fs::File::open(log) {
        let mut buffer = [0u8; 4096];
        if let Ok(read) = file.read(&mut buffer)
            && let Some(version) = version_from_log(&String::from_utf8_lossy(&buffer[..read]))
        {
            return Some(version);
        }
    }

    let root = user_folder.parent().and_then(Path::parent)?;
    let ini = root.join("BeamNG.drive.ini");
    if ini.is_file()
        && fs::metadata(&ini)
            .map(|meta| meta.len() < 64 * 1024)
            .unwrap_or(false)
        && let Ok(text) = fs::read_to_string(&ini)
    {
        return version_from_ini(&text);
    }
    None
}

pub fn user_root_from_startup_ini(game_dir: &Path) -> Option<PathBuf> {
    let ini = game_dir.join("startup.ini");
    let metadata = fs::metadata(&ini).ok()?;
    if metadata.len() > 64 * 1024 {
        return None;
    }
    let text = fs::read_to_string(ini).ok()?;
    user_root_from_startup_ini_text(&text, game_dir)
}

pub fn user_root_from_startup_ini_text(text: &str, game_dir: &Path) -> Option<PathBuf> {
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if let Some(name) = section_name(line) {
            section = name.to_lowercase();
            continue;
        }
        if section != "filesystem" {
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with(';') || trimmed.starts_with('#') {
            continue;
        }
        let Some(after_key) = trimmed.strip_prefix("UserPath") else {
            continue;
        };
        let after_key = after_key.trim_start();
        let Some(value) = after_key.strip_prefix('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            return None;
        }
        let path = Path::new(value);
        return Some(if path.is_absolute() {
            path.to_path_buf()
        } else {
            game_dir.join(path)
        });
    }
    None
}

fn section_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    let inner = inner.trim();
    if inner.is_empty() { None } else { Some(inner) }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatLevel {
    Unknown,
    Same,
    ModOlder,
    ModNewer,
}

pub fn parse_compat_version(text: Option<&str>) -> Option<(i32, i32)> {
    let text = text?;
    if text.trim().is_empty() {
        return None;
    }
    let mut best: Option<(i32, i32)> = None;
    for captures in compat_version_re().captures_iter(text) {
        let Ok(captures) = captures else {
            continue;
        };
        let (Some(major), Some(minor)) = (captures.get(1), captures.get(2)) else {
            continue;
        };
        let Ok(major) = major.as_str().parse::<i32>() else {
            continue;
        };
        let Ok(minor) = minor.as_str().parse::<i32>() else {
            continue;
        };
        let value = (major, minor);
        if best.is_none_or(|current| value > current) {
            best = Some(value);
        }
    }
    best
}

pub fn compare_compat(mod_for: Option<&str>, game: Option<&str>) -> CompatLevel {
    let (Some(mod_for), Some(game)) = (parse_compat_version(mod_for), parse_compat_version(game))
    else {
        return CompatLevel::Unknown;
    };
    if mod_for == game {
        CompatLevel::Same
    } else if mod_for < game {
        CompatLevel::ModOlder
    } else {
        CompatLevel::ModNewer
    }
}

pub fn short_version(version: Option<&str>) -> Option<String> {
    parse_compat_version(version).map(|(major, minor)| format!("{major}.{minor}"))
}

pub fn describe_compat(mod_for: Option<&str>, game: Option<&str>) -> Option<String> {
    let short_mod = short_version(mod_for);
    let short_game = short_version(game);
    match compare_compat(mod_for, game) {
        CompatLevel::Same => Some(format!("{}, как у вас", short_mod?)),
        CompatLevel::ModOlder => Some(format!(
            "{} — у вас {}, может не работать",
            short_mod?, short_game?
        )),
        CompatLevel::ModNewer => Some(format!(
            "{} — у вас {}, нужна игра новее",
            short_mod?, short_game?
        )),
        CompatLevel::Unknown => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const STARTUP: &str = include_str!("../../../fixtures/game/startup-0.39.4.ini");

    #[test]
    fn reads_versions_from_log_and_ini() {
        assert_eq!(
            version_from_log("Log started - v 0.39.4.0 - x64 - build 20972").as_deref(),
            Some("0.39.4.0")
        );
        assert_eq!(
            version_from_ini(
                "[game]
version = 0.39.4.0
notes: 0.1"
            )
            .as_deref(),
            Some("0.39.4.0")
        );
        assert_eq!(
            version_from_ini("notes=0.39.4.0"),
            None,
            "постороннее 0.x — не версия игры"
        );
        assert_eq!(version_from_ini("мусор"), None);
    }

    #[test]
    fn log_version_survives_multibyte_boundary() {
        let mut log = "a".repeat(4095);
        log.push('д');
        log.push_str(" Log started - v 0.39.4.0 - x64");
        assert_eq!(version_from_log(&log).as_deref(), Some("0.39.4.0"));
    }

    #[test]
    fn startup_ini_fixture_has_no_user_path() {
        let game_dir = Path::new(r"C:\Games\BeamNG.drive");
        assert_eq!(user_root_from_startup_ini_text(STARTUP, game_dir), None);
    }

    // «D:\\BeamNG User» абсолютен только в Windows: игра и программа живут там.
    #[cfg(windows)]
    #[test]
    fn startup_ini_absolute_relative_and_comments() {
        let game_dir = Path::new(r"C:\Games\BeamNG.drive");
        let absolute = "[filesystem]\nUserPath = D:\\BeamNG User\n";
        assert_eq!(
            user_root_from_startup_ini_text(absolute, game_dir),
            Some(PathBuf::from(r"D:\BeamNG User"))
        );

        let relative = "[filesystem]\nUserPath = ..\\user\n";
        assert_eq!(
            user_root_from_startup_ini_text(relative, game_dir),
            Some(game_dir.join("..\\user"))
        );

        let commented = "[filesystem]\n;UserPath = D:\\Ignored\n#UserPath = D:\\Ignored\n";
        assert_eq!(user_root_from_startup_ini_text(commented, game_dir), None);

        let other = "[other]\nUserPath = D:\\Ignored\n";
        assert_eq!(user_root_from_startup_ini_text(other, game_dir), None);

        let empty = "[filesystem]\nUserPath =\n";
        assert_eq!(user_root_from_startup_ini_text(empty, game_dir), None);
    }

    #[test]
    fn log_version_wins_over_ini() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("BeamNG");
        let user = root.join("BeamNG.drive").join("current");
        fs::create_dir_all(&user).unwrap();
        fs::write(root.join("BeamNG.drive.ini"), "version=0.39.4.0").unwrap();
        fs::write(user.join("beamng.log"), "Log started - v 0.38.0.0").unwrap();
        assert_eq!(detect_version(&user).as_deref(), Some("0.38.0.0"));
    }

    #[test]
    fn detects_version_from_log_without_ini() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("BeamNG/BeamNG.drive/current");
        fs::create_dir_all(&user).unwrap();
        fs::write(user.join("beamng.log"), "Log started - v 0.39.4.0 - x64").unwrap();
        assert_eq!(detect_version(&user).as_deref(), Some("0.39.4.0"));
    }

    #[test]
    fn compatibility_uses_first_two_numbers() {
        assert_eq!(
            compare_compat(Some("0.38"), Some("0.39")),
            CompatLevel::ModOlder
        );
        assert_eq!(
            compare_compat(Some("0.38.3"), Some("0.38.5")),
            CompatLevel::Same
        );
        assert_eq!(
            compare_compat(Some("0.40"), Some("0.39")),
            CompatLevel::ModNewer
        );
        assert_eq!(
            compare_compat(Some("0.36–0.38"), Some("0.38")),
            CompatLevel::Same
        );
        assert_eq!(
            compare_compat(Some("мусор"), Some("0.39")),
            CompatLevel::Unknown
        );
        assert_eq!(
            describe_compat(Some("0.38"), Some("0.39")).as_deref(),
            Some("0.38 — у вас 0.39, может не работать")
        );
        assert_eq!(
            describe_compat(Some("0.39.4.0"), Some("0.39.1")).as_deref(),
            Some("0.39, как у вас")
        );
    }
}

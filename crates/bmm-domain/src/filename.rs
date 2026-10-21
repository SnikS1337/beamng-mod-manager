use std::path::Path;
use std::sync::OnceLock;

use fancy_regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuessedName {
    pub name: String,
    pub author: Option<String>,
    pub version: Option<String>,
}

struct Patterns {
    by_for: Regex,
    site_tag: Regex,
    trailing_version: Regex,
    dotted_trailing: Regex,
    v_prefixed: Regex,
    engine_like: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        by_for: Regex::new(
            r"(?i)[_\-\s]by[_\-\s](?<a>.+?)[_\-\s]for[_\-\s]beamng.*$",
        )
        .unwrap(),
        site_tag: Regex::new(
            r"(?i)(?<![\p{L}\p{Nd}])[_\-\s]?(?:modland[_\-\s]?net|modsgaming|stmods)(?![\p{L}\p{Nd}])",
        )
        .unwrap(),
        trailing_version: Regex::new(
            r"(?i)\s(?:v\s*)?(?<v>\d{1,3}(?:\s\d{1,3})+)$",
        )
        .unwrap(),
        dotted_trailing: Regex::new(
            r"(?i)\s(?:v\s*)?(?<v>\d{1,3}(?:\.\d{1,3})+[a-z]?)$",
        )
        .unwrap(),
        v_prefixed: Regex::new(
            r"(?i)\sv(?<v>\d{1,3}(?:\s\d{1,3})*)$",
        )
        .unwrap(),
        engine_like: Regex::new(r"(?i)^\sv\s*(?:4|6|8|10|12|16)$").unwrap(),
    })
}

pub fn from_file_name(file_name: &str) -> GuessedName {
    let stem = Path::new(file_name.trim())
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| file_name.trim().to_string());
    let p = patterns();

    let mut text = replace_all(&p.site_tag, &stem);

    let mut author = None;
    if let Ok(Some(captures)) = p.by_for.captures(&text)
        && let Some(found) = captures.get(0)
        && let Some(author_match) = captures.name("a")
    {
        author = Some(
            author_match
                .as_str()
                .replace(['_', '-'], " ")
                .trim()
                .to_string(),
        );
        text.truncate(found.start());
    }

    let mut name = text.replace(['_', '-'], " ").trim().to_string();

    let mut version = None;
    for regex in [&p.dotted_trailing, &p.trailing_version, &p.v_prefixed] {
        let Ok(Some(captures)) = regex.captures(&name) else {
            continue;
        };
        let Some(found) = captures.get(0) else {
            continue;
        };
        let Some(value) = captures.name("v") else {
            continue;
        };
        if p.engine_like.is_match(found.as_str()).unwrap_or(false) {
            continue;
        }
        version = Some(
            value
                .as_str()
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("."),
        );
        name.truncate(found.start());
        name = name.trim().to_string();
        break;
    }

    GuessedName {
        name,
        author,
        version,
    }
}

fn replace_all(regex: &Regex, text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last = 0;
    for found in regex.find_iter(text) {
        let Ok(found) = found else {
            continue;
        };
        result.push_str(&text[last..found.start()]);
        last = found.end();
    }
    result.push_str(&text[last..]);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_version_and_cleans_underscores() {
        let guessed = from_file_name("BMW_M3_G87_v1.2.zip");
        assert_eq!(guessed.name, "BMW M3 G87");
        assert_eq!(guessed.version.as_deref(), Some("1.2"));
    }

    #[test]
    fn reads_trailing_dotted_version() {
        let guessed = from_file_name("Toyota Sienna 2011-2017 2.2.zip");
        assert_eq!(guessed.name, "Toyota Sienna 2011 2017");
        assert_eq!(guessed.version.as_deref(), Some("2.2"));
    }

    #[test]
    fn keeps_engine_marks_in_name() {
        let guessed = from_file_name("Ford_V8_Coupe.zip");
        assert_eq!(guessed.name, "Ford V8 Coupe");
        assert_eq!(guessed.version, None);
    }

    #[test]
    fn extracts_author() {
        let guessed = from_file_name("car_by_John_for_beamng.zip");
        assert_eq!(guessed.name, "car");
        assert_eq!(guessed.author.as_deref(), Some("John"));
    }

    #[test]
    fn removes_site_tags() {
        let guessed = from_file_name("modland_net_BMW_M3.zip");
        assert_eq!(guessed.name, "BMW M3");
        let guessed = from_file_name("thing_STMods.zip");
        assert_eq!(guessed.name, "thing");
    }

    #[test]
    fn reads_space_separated_version() {
        let guessed = from_file_name("Car 1 2.zip");
        assert_eq!(guessed.name, "Car");
        assert_eq!(guessed.version.as_deref(), Some("1.2"));
    }
}

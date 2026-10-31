use std::collections::HashSet;
use std::sync::OnceLock;

use fancy_regex::Regex;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedTitle {
    pub name: String,
    /// Рубрика сайта в начале названия («автомобиль LADA 2106», «Карта «…»»):
    /// из имени убрана, показывается меткой.
    pub kind: Option<ModKind>,
    pub mod_version: Option<String>,
    pub game_version: Option<String>,
    pub has_paid_marker: bool,
}

struct Patterns {
    paid_bracketed: Regex,
    paid_upper: Regex,
    game_version: Regex,
    leading_mod: Regex,
    leading_kind: Regex,
    leading_game: Regex,
    for_beam: Regex,
    word_version: Regex,
    version_with_v: Regex,
    trailing_bare_version: Regex,
    trailing_game_version: Regex,
    mods_only: Regex,
    whitespace: Regex,
    square: Regex,
    round: Regex,
    round_noise: Regex,
    bare_beam: Regex,
    update_word: Regex,
    dotted_version: Regex,
    short_version: Regex,
    non_word: Regex,
    bracket_version: Regex,
    version_before_note: Regex,
    number_after_year: Regex,
}

fn compile(pattern: &str, case_insensitive: bool) -> Regex {
    let pattern = if case_insensitive {
        format!("(?i){pattern}")
    } else {
        pattern.to_string()
    };
    Regex::new(&pattern).expect("статическое выражение должно компилироваться")
}

impl Patterns {
    fn new() -> Self {
        Self {
            paid_bracketed: compile(
                r"\[\s*(?:paid|premium|vip|early\s+access)\s*\]|\(\s*paid\s*\)",
                true,
            ),
            paid_upper: compile(r"(?<![\p{L}\p{Nd}])PAID(?![\p{L}\p{Nd}])", false),
            game_version: compile(r"\(\s*v?(\d+\.\d+(?:\.(?:\d+|x))*)\s*\)", true),
            leading_mod: compile(r"^\s*(?:мод|mod)\s+", true),
            // «Мод автомобиль LADA 2106», «Карта «Sumner Road»»: слово-рубрика сайта
            // в начале названия — не часть названия мода.
            leading_kind: compile(
                r"^\s*(?:автомобил[ьяи]|машин[аы]|карт[аы]|map)(?=[\s«\x22])\s*",
                true,
            ),
            leading_game: compile(
                r"^\s*beamng\s*\.?\s*drive\s*[-–—:]?\s*(?:скачать\s+моды?|скачать|моды?)\s+",
                true,
            ),
            for_beam: compile(r"\s*\b(?:для|for)\s+beamng(?:\s*\.?\s*drive)?\s*$", true),
            word_version: compile(
                r"\s(?:версия|version)\s+(\d+(?:\.\d+)*[a-z]?)(?![\p{L}\p{Nd}.])",
                true,
            ),
            version_with_v: compile(
                r"(?<![\p{L}\p{Nd}])v(\d+(?:\.\d+)+[a-z]?)(?![\p{L}\p{Nd}.])",
                true,
            ),
            trailing_bare_version: compile(
                r"\s(\d{1,2}(?:\.\d+)+(?:-?(?:beta|alpha|rc)\d*)?)\s*$",
                true,
            ),
            // «Universal Monster Frame v4.1.96 0.40.x»: версия игры без скобок.
            trailing_game_version: compile(r"\s(\d+\.\d+\.x)\s*$", true),
            mods_only: compile(r"^моды?$", true),
            whitespace: compile(r"\s+", false),
            square: compile(r"\[[^\]]*\]", true),
            round: compile(r"\(([^)]*)\)", true),
            round_noise: compile(
                r"\d\.\d|\b(?:update|fix|fixed|pack|beta|wip|paid|premium)\b",
                true,
            ),
            bare_beam: compile(r"\bbeamng(?:\s*\.?\s*drive)?\b", true),
            update_word: compile(
                r"\bupdate\s*\d*\b|\bfix(?:ed)?\b|\b(?:версия|version)\b",
                true,
            ),
            dotted_version: compile(r"\bv?\d+(?:\.\d+)+(?:\.x)?\b", true),
            short_version: compile(r"\bv\d+\b", true),
            non_word: compile(r"[^\p{L}\p{Nd}]+", true),
            bracket_version: compile(r"\s*[\[(]\s*v?\d+(?:\.\d+)+[a-z]?\s*[\])]\s*$", true),
            version_before_note: compile(r"\s+v?\d{1,2}(?:\.\d+)+\s+(\([^()]*\))\s*$", true),
            number_after_year: compile(r"(\b(?:19|20)\d{2})\s+\d{1,2}\s*$", true),
        }
    }
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(Patterns::new)
}

/// Что за мод по описанию на сайте.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModKind {
    Vehicle,
    Map,
    Addon,
}

impl ModKind {
    pub fn code(self) -> &'static str {
        match self {
            ModKind::Vehicle => "vehicle",
            ModKind::Map => "map",
            ModKind::Addon => "addon",
        }
    }

    /// По разделу сайта: адрес ModsGaming («mashiny»), Modland («Cars»),
    /// репозиторий BeamNG («Terrains, Levels, Maps», «Mods of Mods»).
    pub fn from_category(category: &str) -> Option<ModKind> {
        let text = category.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|word| text.contains(word));
        if has(&[
            "mashin",
            "машин",
            "автомоб",
            "car",
            "vehicle",
            "truck",
            "bus",
            "грузов",
        ]) {
            Some(ModKind::Vehicle)
        } else if has(&["karty", "карт", "map", "level", "terrain"]) {
            Some(ModKind::Map)
        } else if has(&[
            "mods-of-mods",
            "mods of mods",
            "addon",
            "skin",
            "sound",
            "звук",
            "interface",
            "ui app",
            "prop",
            "license",
            "scenario",
            "сценари",
            "тюнинг",
            "запчаст",
        ]) {
            Some(ModKind::Addon)
        } else {
            None
        }
    }

    fn from_title_word(word: &str) -> Option<ModKind> {
        let word = word.to_lowercase();
        if word.starts_with("автомобил") || word.starts_with("машин") {
            Some(ModKind::Vehicle)
        } else if word.starts_with("карт") || word == "map" {
            Some(ModKind::Map)
        } else {
            None
        }
    }
}

pub struct TitleParser;

impl TitleParser {
    pub fn has_paid_marker(title: &str) -> bool {
        let p = patterns();
        p.paid_bracketed.is_match(title).unwrap_or(false)
            || p.paid_upper.is_match(title).unwrap_or(false)
    }

    pub fn parse(raw: &str) -> ParsedTitle {
        let p = patterns();
        let mut text = raw.trim().to_string();
        let paid = Self::has_paid_marker(&text);

        text = replace_all(&p.paid_bracketed, &text, " ");
        text = replace_all(&p.paid_upper, &text, " ");

        let game_version = take_capture(&p.game_version, &mut text)
            .or_else(|| take_capture(&p.trailing_game_version, &mut text));

        let leading_end = match p.leading_game.find(&text) {
            Ok(Some(found)) if found.start() == 0 => Some(found.end()),
            _ => None,
        };
        if let Some(end) = leading_end {
            let after = text[end..].to_string();
            let rest = replace_all(&p.leading_mod, &after, "");
            let rest = rest.trim();
            let only_mods = p.mods_only.is_match(rest).unwrap_or(false);
            if !rest.is_empty() && !only_mods {
                text = after;
            }
        }

        text = replace_all(&p.leading_mod, &text, "");
        let kind = match p.leading_kind.find(&text) {
            Ok(Some(found)) if found.start() == 0 => {
                let kind = ModKind::from_title_word(found.as_str().trim());
                text = text[found.end()..].to_string();
                kind
            }
            _ => None,
        };
        text = replace_all(&p.for_beam, &text, "");

        let mod_version = take_capture(&p.word_version, &mut text)
            .or_else(|| take_capture(&p.version_with_v, &mut text))
            .or_else(|| take_capture(&p.trailing_bare_version, &mut text));

        text = replace_all(&p.whitespace, &text, " ");
        let name = text
            .trim_matches(|c| matches!(c, ' ' | '-' | '–' | '—' | ',' | ':' | ';'))
            .to_string();

        ParsedTitle {
            name,
            kind,
            mod_version,
            game_version,
            has_paid_marker: paid,
        }
    }
}

fn replace_all(regex: &Regex, text: &str, replacement: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last = 0;
    for found in regex.find_iter(text) {
        let Ok(found) = found else {
            continue;
        };
        result.push_str(&text[last..found.start()]);
        result.push_str(replacement);
        last = found.end();
    }
    result.push_str(&text[last..]);
    result
}

/// Заменяет совпадение на его первую группу (с приставкой `prefix`).
fn keep_group(regex: &Regex, text: &str, prefix: &str) -> String {
    let Ok(Some(captures)) = regex.captures(text) else {
        return text.to_string();
    };
    let (Some(whole), Some(group)) = (captures.get(0), captures.get(1)) else {
        return text.to_string();
    };
    format!(
        "{}{prefix}{}{}",
        &text[..whole.start()],
        group.as_str(),
        &text[whole.end()..]
    )
}

fn take_capture(regex: &Regex, text: &mut String) -> Option<String> {
    let captures = regex.captures(text).ok().flatten()?;
    let value = captures.get(1)?.as_str().to_string();
    let (start, end) = {
        let found = captures.get(0)?;
        (found.start(), found.end())
    };
    drop(captures);
    text.replace_range(start..end, "");
    Some(value)
}

fn replace_round(text: &str) -> String {
    let p = patterns();
    let mut result = String::with_capacity(text.len());
    let mut last = 0;

    for captures in p.round.captures_iter(text) {
        let Ok(captures) = captures else {
            continue;
        };
        let Some(found) = captures.get(0) else {
            continue;
        };
        result.push_str(&text[last..found.start()]);
        let inner = captures.get(1).map(|m| m.as_str()).unwrap_or("");
        let noise = p.round_noise.is_match(inner).unwrap_or(false);
        result.push(' ');
        if !noise {
            result.push_str(inner);
            result.push(' ');
        }
        last = found.end();
    }

    result.push_str(&text[last..]);
    result
}

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Латинские буквы без диакритики: «Haleakalā» → «Haleakala», «Café» → «Cafe».
/// Кириллицу не трогаем: «й» и «ё» — отдельные буквы, а не «и» и «е» со значком.
pub fn fold_latin(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut base_is_latin = false;
    for ch in text.nfd() {
        if is_combining_mark(ch) {
            if !base_is_latin {
                result.push(ch);
            }
            continue;
        }
        base_is_latin = ch.is_ascii_alphabetic();
        result.push(ch);
    }
    result.nfc().collect()
}

/// Слова, склеенные заглавными буквами, через пробел: «TrafficReborn» → «Traffic Reborn».
/// Режем только там, где заглавная начинает слово (за ней строчная): «BeamNG» и «GTI»
/// остаются целыми.
pub fn split_camel(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len() + 4);
    for (index, &ch) in chars.iter().enumerate() {
        if index > 0
            && ch.is_ascii_uppercase()
            && chars[index - 1].is_ascii_lowercase()
            && chars
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_lowercase())
        {
            result.push(' ');
        }
        result.push(ch);
    }
    result
}

pub struct TitleNormalizer;

impl TitleNormalizer {
    pub fn tokens(title: &str) -> Vec<String> {
        if title.trim().is_empty() {
            return Vec::new();
        }
        // Имена из zip, собранных на macOS, приходят в NFD; с сайта — в NFC.
        // Без приведения «é» из двух символов и один символ — разные токены.
        // Сайты пишут то «Haleakalā», то «Haleakala»: латиница сравнивается без значков.
        let normalized: String = fold_latin(title);

        let p = patterns();
        let mut text = replace_all(&p.square, &normalized, " ");
        text = replace_round(&text);
        text = replace_all(&p.leading_mod, &text, " ");
        text = replace_all(&p.leading_kind, &text, " ");
        text = replace_all(&p.for_beam, &text, " ");
        text = replace_all(&p.bare_beam, &text, " ");
        text = replace_all(&p.update_word, &text, " ");
        text = replace_all(&p.dotted_version, &text, " ");
        text = replace_all(&p.short_version, &text, " ");
        text = replace_all(&p.non_word, &text, " ");

        text.to_lowercase()
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    pub fn normalize(title: &str) -> String {
        Self::tokens(title).join(" ")
    }

    /// Доля общих слов. Склейку «TrafficReborn» сравниваем и как «Traffic Reborn»:
    /// сайты пишут название то слитно, то раздельно.
    pub fn similarity(a: &str, b: &str) -> f64 {
        let plain = Self::plain_similarity(a, b);
        let (split_a, split_b) = (split_camel(a), split_camel(b));
        if split_a == a && split_b == b {
            return plain;
        }
        plain.max(Self::plain_similarity(&split_a, &split_b))
    }

    /// Слова названия, если склейки разрезать: «TrafficReborn» → traffic, reborn.
    pub fn split_tokens(title: &str) -> Vec<String> {
        Self::tokens(&split_camel(title))
    }

    fn plain_similarity(a: &str, b: &str) -> f64 {
        let left: HashSet<String> = Self::tokens(a).into_iter().collect();
        let right: HashSet<String> = Self::tokens(b).into_iter().collect();
        if left.is_empty() || right.is_empty() {
            return 0.0;
        }
        let intersection = left.intersection(&right).count();
        let union = left.len() + right.len() - intersection;
        intersection as f64 / union as f64
    }
}

pub struct ModTitles;

impl ModTitles {
    const MAX_LENGTH: usize = 80;

    pub fn choose(
        repo_title: Option<&str>,
        archive_title: Option<&str>,
        site_title: Option<&str>,
        fallback: &str,
    ) -> String {
        usable(repo_title)
            .or_else(|| usable(archive_title))
            .or_else(|| usable(Self::site(site_title).as_deref()))
            .unwrap_or_else(|| fallback.to_string())
    }

    pub fn site(raw: Option<&str>) -> Option<String> {
        let raw = raw?;
        if raw.trim().is_empty() {
            return None;
        }
        let parsed = TitleParser::parse(raw);
        let p = patterns();
        let name = replace_all(&p.bracket_version, &parsed.name, "");
        // «Premium 1.0 (Replace/Add-On)»: версия перед пометкой в скобках.
        let name = keep_group(&p.version_before_note, &name, " ");
        // «Toyota Sienna 2017 1»: номер версии без точки после года.
        let name = keep_group(&p.number_after_year, &name, "");
        let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
        // «Карта «Приозерск»»: рубрику убрали, кавычки вокруг всего названия — лишние.
        let name = name
            .strip_prefix('«')
            .and_then(|rest| rest.strip_suffix('»'))
            .filter(|inner| !inner.contains(['«', '»']))
            .map(str::to_string)
            .unwrap_or(name);
        let name = name.trim().to_string();
        if name.is_empty() { None } else { Some(name) }
    }
}

fn usable(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() || value.chars().count() > ModTitles::MAX_LENGTH {
        None
    } else {
        Some(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn site_names_drop_leftover_versions_and_quotes() {
        let site = |raw: &str| ModTitles::site(Some(raw));
        assert_eq!(
            site("Карта «Приозерск» версия 0.1").as_deref(),
            Some("Приозерск")
        );
        assert_eq!(
            site("Toyota Sienna 2017 1").as_deref(),
            Some("Toyota Sienna 2017")
        );
        assert_eq!(
            site("2017 Toyota Sienna XLE Premium 1.0 (Replace/Add-On)").as_deref(),
            Some("2017 Toyota Sienna XLE Premium (Replace/Add-On)")
        );
        // Не трогаем то, что версией не является.
        assert_eq!(site("Porsche 911").as_deref(), Some("Porsche 911"));
        assert_eq!(
            site("Seat Leon MK2 (2005)").as_deref(),
            Some("Seat Leon MK2 (2005)")
        );
        assert_eq!(site("«A» и «B»").as_deref(), Some("«A» и «B»"));
    }

    #[test]
    fn glued_words_match_the_spaced_title() {
        assert_eq!(split_camel("TrafficReborn"), "Traffic Reborn");
        assert_eq!(split_camel("BeamNG GTI"), "BeamNG GTI");
        assert_eq!(
            TitleNormalizer::similarity("TrafficReborn", "Traffic Reborn [Free Edition] 1.4"),
            1.0
        );
        assert_eq!(
            TitleNormalizer::similarity("McLaren 720S", "McLaren 720S"),
            1.0
        );
        assert_eq!(
            TitleNormalizer::similarity("Mclaren 720S", "McLaren 720S"),
            1.0
        );
        assert_eq!(
            TitleNormalizer::split_tokens("TrafficReborn"),
            vec!["traffic", "reborn"]
        );
    }

    #[test]
    fn bare_game_version_and_beta_version_leave_the_name() {
        let frame = TitleParser::parse("Universal Monster Frame v4.1.96 0.40.x");
        assert_eq!(frame.name, "Universal Monster Frame");
        assert_eq!(frame.mod_version.as_deref(), Some("4.1.96"));
        assert_eq!(frame.game_version.as_deref(), Some("0.40.x"));
        let beta = TitleParser::parse("ACNG - Tire Heat 0.3.1-beta");
        assert_eq!(beta.name, "ACNG - Tire Heat");
        assert_eq!(beta.mod_version.as_deref(), Some("0.3.1-beta"));
    }

    #[test]
    fn latin_diacritics_do_not_split_titles_but_cyrillic_letters_stay() {
        assert_eq!(fold_latin("Haleakalā (Maui)"), "Haleakala (Maui)");
        assert_eq!(fold_latin("Škoda Café"), "Skoda Cafe");
        assert_eq!(fold_latin("Жигули ёлка йогурт"), "Жигули ёлка йогурт");
        assert_eq!(
            TitleNormalizer::similarity("Haleakalā, Maui", "Haleakala Maui"),
            1.0
        );
    }

    #[test]
    fn unicode_forms_are_the_same_title() {
        let composed = "Café Racer";
        let decomposed = "Cafe\u{301} Racer";
        assert_eq!(
            TitleNormalizer::tokens(composed),
            TitleNormalizer::tokens(decomposed)
        );
        assert_eq!(
            TitleNormalizer::similarity(composed, decomposed),
            1.0,
            "NFD и NFC не должны давать разные токены"
        );
    }

    #[test]
    fn normalize_removes_service_wrappers() {
        assert_eq!(
            TitleNormalizer::normalize(
                "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE"
            ),
            "toyota sienna 2011 2017"
        );
        assert_eq!(
            TitleNormalizer::normalize("Toyota Sienna 2011-2017 2.2"),
            "toyota sienna 2011 2017"
        );
        assert_eq!(
            TitleNormalizer::normalize("BMW M3 E92 [PACK] Update 2!"),
            "bmw m3 e92"
        );
        assert_eq!(
            TitleNormalizer::normalize(
                "МОД Autobello Aria (1972) v1.0.4 (0.39.x) ДЛЯ BEAMNG.DRIVE"
            ),
            "autobello aria 1972"
        );
        assert_eq!(
            TitleNormalizer::normalize("[PAID] Daxworx Plate Pack"),
            "daxworx plate pack"
        );
        assert_eq!(TitleNormalizer::normalize("   "), "");
    }

    #[test]
    fn site_rubric_words_are_not_part_of_the_name() {
        // Настоящие заголовки STMods и ModsGaming.
        assert_eq!(
            TitleNormalizer::normalize("Карта «Sumner Road» версия 1.0 для BeamNG.drive (v0.40.x)"),
            "sumner road"
        );
        assert_eq!(
            TitleNormalizer::normalize("Мод автомобиль LADA 2101 для BeamNG . drive"),
            "lada 2101"
        );
        assert_eq!(
            TitleNormalizer::normalize(
                "МОД Карта Poland - Winter v3.2 (0.38.x) ДЛЯ BEAMNG . DRIVE"
            ),
            "poland winter"
        );
        // Слово внутри названия остаётся.
        assert_eq!(
            TitleNormalizer::normalize("Treasure Map Pack"),
            "treasure map pack"
        );
        assert_eq!(TitleNormalizer::normalize("Mapleton"), "mapleton");
    }

    #[test]
    fn similarity_uses_token_sets() {
        assert_eq!(
            TitleNormalizer::similarity("Honda Accord 2017 Remaster v1.0", "Honda Accord 2017"),
            0.75
        );
        assert_eq!(TitleNormalizer::similarity("Kicks 2017", "Kicks 2017"), 1.0);
    }

    #[test]
    fn parse_mods_gaming_title() {
        let parsed = TitleParser::parse("МОД Toyota AE86 RLA v1.0.7 (0.39.x) ДЛЯ BEAMNG.DRIVE");
        assert_eq!(parsed.name, "Toyota AE86 RLA");
        assert_eq!(parsed.mod_version.as_deref(), Some("1.0.7"));
        assert_eq!(parsed.game_version.as_deref(), Some("0.39.x"));
        assert!(!parsed.has_paid_marker);
    }

    #[test]
    fn parse_takes_site_heading_as_kind() {
        let parsed = TitleParser::parse("Мод автомобиль VAZ (lada) 2106");
        assert_eq!(parsed.name, "VAZ (lada) 2106");
        assert_eq!(parsed.kind, Some(ModKind::Vehicle));
        let parsed = TitleParser::parse("автомобиль LADA 2106 v1.0");
        assert_eq!(parsed.name, "LADA 2106");
        assert_eq!(parsed.kind, Some(ModKind::Vehicle));
        let parsed = TitleParser::parse("Карта «Sumner Road»");
        assert_eq!(parsed.kind, Some(ModKind::Map));
        // Слово внутри названия — не рубрика.
        let parsed = TitleParser::parse("Weather Map Tool");
        assert_eq!(parsed.kind, None);
        assert_eq!(parsed.name, "Weather Map Tool");
        let parsed = TitleParser::parse("Картинг Arena");
        assert_eq!(parsed.kind, None);
    }

    #[test]
    fn kind_from_site_sections() {
        assert_eq!(ModKind::from_category("mashiny"), Some(ModKind::Vehicle));
        assert_eq!(ModKind::from_category("karty"), Some(ModKind::Map));
        assert_eq!(ModKind::from_category("Cars"), Some(ModKind::Vehicle));
        assert_eq!(ModKind::from_category("Maps"), Some(ModKind::Map));
        assert_eq!(
            ModKind::from_category("Terrains, Levels, Maps"),
            Some(ModKind::Map)
        );
        assert_eq!(ModKind::from_category("Mods of Mods"), Some(ModKind::Addon));
        assert_eq!(
            ModKind::from_category("User Interface Apps"),
            Some(ModKind::Addon)
        );
        assert_eq!(ModKind::from_category("Sounds"), Some(ModKind::Addon));
        assert_eq!(ModKind::from_category("Разное"), None);
    }

    #[test]
    fn parse_keeps_year_and_strips_version() {
        let parsed =
            TitleParser::parse("МОД Autobello Aria (1972) v1.0.4 (0.39.x) ДЛЯ BEAMNG.DRIVE");
        assert_eq!(parsed.name, "Autobello Aria (1972)");
        assert_eq!(parsed.mod_version.as_deref(), Some("1.0.4"));
        assert_eq!(parsed.game_version.as_deref(), Some("0.39.x"));
    }

    #[test]
    fn parse_trailing_bare_version() {
        let parsed = TitleParser::parse("Toyota Sienna 2011-2017 2.2");
        assert_eq!(parsed.name, "Toyota Sienna 2011-2017");
        assert_eq!(parsed.mod_version.as_deref(), Some("2.2"));
    }

    #[test]
    fn parse_cyrillic_and_word_version() {
        let parsed = TitleParser::parse("МОД ВАЗ 2106 версия 6.0 (0.38.x)");
        assert_eq!(parsed.name, "ВАЗ 2106");
        assert_eq!(parsed.mod_version.as_deref(), Some("6.0"));
        assert_eq!(parsed.game_version.as_deref(), Some("0.38.x"));
    }

    #[test]
    fn parse_paid_marker() {
        let parsed = TitleParser::parse("[PAID] Daxworx Plate Pack");
        assert_eq!(parsed.name, "Daxworx Plate Pack");
        assert!(parsed.has_paid_marker);
    }

    #[test]
    fn parse_does_not_treat_v8_as_version() {
        let parsed = TitleParser::parse("Ford V8 Coupe v1.2");
        assert_eq!(parsed.name, "Ford V8 Coupe");
        assert_eq!(parsed.mod_version.as_deref(), Some("1.2"));
    }

    #[test]
    fn mod_titles_precedence_and_limits() {
        assert_eq!(
            ModTitles::choose(Some("Repo"), Some("Archive"), Some("Site"), "file.zip"),
            "Repo"
        );
        assert_eq!(
            ModTitles::choose(Some("   "), Some("Archive"), Some("Site"), "file.zip"),
            "Archive"
        );
        assert_eq!(
            ModTitles::choose(None, None, Some("Site"), "file.zip"),
            "Site"
        );
        assert_eq!(ModTitles::choose(None, None, None, "file.zip"), "file.zip");
        let long = "x".repeat(81);
        assert_eq!(
            ModTitles::choose(Some(long.as_str()), Some("Archive"), None, "file.zip"),
            "Archive"
        );
    }

    #[test]
    fn site_titles_are_cleaned() {
        assert_eq!(
            ModTitles::site(Some("МОД BMW E34 v1.2 (0.32) ДЛЯ BEAMNG.DRIVE")).as_deref(),
            Some("BMW E34")
        );
        assert_eq!(
            ModTitles::site(Some("Ibishu Miramar Dirt Race [0.9.0]")).as_deref(),
            Some("Ibishu Miramar Dirt Race")
        );
        assert_eq!(
            ModTitles::site(Some("Lada Niva 4x4 версия 3.3 для BeamNG.drive (v0.39.x)")).as_deref(),
            Some("Lada Niva 4x4")
        );
        assert_eq!(
            ModTitles::site(Some("Ford Crown Victoria V8 [PAID]")).as_deref(),
            Some("Ford Crown Victoria V8")
        );
        assert_eq!(ModTitles::site(None), None);
        assert_eq!(ModTitles::site(Some("  ")), None);
    }
}

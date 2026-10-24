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
    copy_suffix: Regex,
    tag_after_version: Regex,
    pre_release: Regex,
    glued_version: Regex,
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
        // Метки сайтов и каналов, которые дописывают к имени файла при раздаче:
        // modland(.net), modsgaming, stmods, телеграм-канал «tg_m0dsbeamng».
        site_tag: Regex::new(
            r"(?i)(?<![\p{L}\p{Nd}])[_\-\s]?(?:modland(?:[_\-\s]?net)?|modsgaming|stmods|(?:tg[_\-\s]?)?m0dsbeamng)(?![\p{L}\p{Nd}])",
        )
        .unwrap(),
        // Копия, которую браузер сохранил повторно: «weather_fx (1)».
        copy_suffix: Regex::new(r"\s*\(\d{1,3}\)$").unwrap(),
        // Короткая метка раздачи сразу за версией: «v1.2_bm».
        tag_after_version: Regex::new(r"(?i)(?:(?<=\.\d)|(?<=\.\d\d)|(?<=\.\d\d\d))\s+bm$").unwrap(),
        pre_release: Regex::new(r"(?i)\s(?<p>(?:alpha|beta|rc)\s?\d{0,3})$").unwrap(),
        // Версия, приклеенная к слову: «hotw0.3.0», «remasterVer1.0».
        glued_version: Regex::new(
            r"(?i)(?<=\p{L})(?:ver(?:sion)?|v)?(?<v>\d{1,3}(?:\.\d{1,3}){1,3})$",
        )
        .unwrap(),
        trailing_version: Regex::new(
            r"(?i)\s(?:v\s*)?(?<v>\d{1,3}(?:\s\d{1,3})+)$",
        )
        .unwrap(),
        dotted_trailing: Regex::new(
            r"(?i)\s(?:v(?:er(?:sion)?)?\.?\s*)?(?<v>\d{1,3}(?:\.\d{1,3})+[a-z]?)$",
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

    let stem = replace_all(&p.copy_suffix, &stem);
    let mut text = replace_all(&p.site_tag, &stem);

    let mut author = None;
    // Только «by X for beamng»: короткое «_by_X» чаще пишет тот, кто перезалил мод,
    // и строгая сверка автора отбросила бы настоящую страницу.
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
    name = replace_all(&p.tag_after_version, &name);

    // «v2.8.0 alpha2»: суффикс сборки остаётся частью версии, иначе 2.8.0-alpha2
    // и 2.8.0 выглядели бы одинаковыми.
    let mut pre = None;
    if let Ok(Some(captures)) = p.pre_release.captures(&name)
        && let (Some(found), Some(value)) = (captures.get(0), captures.name("p"))
    {
        pre = Some((
            found.start(),
            value.as_str().replace(' ', "").to_lowercase(),
        ));
    }
    let base = match &pre {
        Some((start, _)) => name[..*start].trim().to_string(),
        None => name.clone(),
    };

    let mut version = None;
    let mut rest = base.clone();
    for regex in [
        &p.dotted_trailing,
        &p.trailing_version,
        &p.v_prefixed,
        &p.glued_version,
    ] {
        let Ok(Some(captures)) = regex.captures(&base) else {
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
        rest = base[..found.start()].trim().to_string();
        break;
    }
    if let Some(found) = &version {
        if let Some((_, pre)) = pre {
            version = Some(format!("{found}-{pre}"));
        }
        name = rest;
    }
    let name = name
        .split_whitespace()
        .map(split_camel)
        .collect::<Vec<_>>()
        .join(" ");

    GuessedName {
        name,
        author,
        version,
    }
}

/// «AstonMartinValhalla» → «Aston Martin Valhalla», «smokeDustFX» → «smoke Dust FX»:
/// склеенное название сайты не находят. Делим только слово с двумя и больше
/// переходами и не отрезаем короткие куски вроде «Mc» в «McLaren».
fn split_camel(word: &str) -> String {
    let chars: Vec<char> = word.chars().collect();
    let mut cuts = Vec::new();
    for index in 1..chars.len() {
        let (before, here) = (chars[index - 1], chars[index]);
        let next_lower = chars.get(index + 1).is_some_and(|c| c.is_lowercase());
        if (before.is_lowercase() && here.is_uppercase())
            || ((before.is_uppercase() || before.is_ascii_digit())
                && here.is_uppercase()
                && next_lower)
        {
            cuts.push(index);
        }
    }
    if cuts.len() < 2 {
        return word.to_string();
    }
    let mut pieces = Vec::new();
    let mut start = 0;
    for cut in cuts.iter().copied().chain(std::iter::once(chars.len())) {
        let piece: String = chars[start..cut].iter().collect();
        let short_word =
            piece.chars().count() < 3 && piece.chars().skip(1).any(|c| c.is_lowercase());
        if short_word && cut != chars.len() {
            continue; // «Mc» остаётся со следующим куском
        }
        pieces.push(piece);
        start = cut;
    }
    pieces.join(" ")
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

    /// Имена файлов из реального набора: скачаны с разных сайтов и телеграм-каналов.
    #[test]
    fn real_downloaded_file_names() {
        let cases: &[(&str, &str, Option<&str>, Option<&str>)] = &[
            (
                "AndrLada2106remasterVer1.0.zip",
                "AndrLada2106remaster",
                None,
                Some("1.0"),
            ),
            ("hotw0.3.0.zip", "hotw", None, Some("0.3.0")),
            (
                "Nissan Laurel C33 Ruzskye Stvoly v1.2_bm.zip",
                "Nissan Laurel C33 Ruzskye Stvoly",
                None,
                Some("1.2"),
            ),
            (
                "hawaii_haleakala_maui_v1.2.1_tg_m0dsbeamng.zip",
                "hawaii haleakala maui",
                None,
                Some("1.2.1"),
            ),
            (
                "porsche_992_gt3rs_v2.0_modland.zip",
                "porsche 992 gt3rs",
                None,
                Some("2.0"),
            ),
            ("weather_fx_tg_m0dsbeamng.zip", "weather fx", None, None),
            ("weather_fx_modland.zip", "weather fx", None, None),
            ("weather_fx (1).zip", "weather fx", None, None),
            (
                "acm_percheron_addon_GR_v1-0 (1).zip",
                "acm percheron addon GR",
                None,
                Some("1.0"),
            ),
            (
                "rls_career_overhaul_v2.8.0_alpha2.zip",
                "rls career overhaul",
                None,
                Some("2.8.0-alpha2"),
            ),
            ("LXR_S14_V1.3.zip", "LXR S14", None, Some("1.3")),
            (
                "carp_dynamicdirt_0.2.zip",
                "carp dynamicdirt",
                None,
                Some("0.2"),
            ),
            (
                "VAZ_2104_2105_2107VangohMods_v0.2.3.zip",
                "VAZ 2104 2105 2107 Vangoh Mods",
                None,
                Some("0.2.3"),
            ),
            (
                "Universal_EV_Conversion_modland.zip",
                "Universal EV Conversion",
                None,
                None,
            ),
            (
                "vaz2106kenemationrelated_by_ViharekMods.zip",
                "vaz2106kenemationrelated by ViharekMods",
                None,
                None,
            ),
            (
                "AstonMartinValhalla.zip",
                "Aston Martin Valhalla",
                None,
                None,
            ),
            ("HighPowerCarPack.zip", "High Power Car Pack", None, None),
            ("MapsFromRCD.zip", "Maps From RCD", None, None),
            (
                "smokeDustFX_mihaits.zip",
                "smoke Dust FX mihaits",
                None,
                None,
            ),
            ("McLaren_P1.zip", "McLaren P1", None, None),
            ("CJD_Specials.zip", "CJD Specials", None, None),
            ("mark2_tg_m0dsbeamng.zip", "mark2", None, None),
            ("Sumner_Road_tg_m0dsbeamng.zip", "Sumner Road", None, None),
            (
                "McLaren7SportSeries_Frix.zip",
                "McLaren7 Sport Series Frix",
                None,
                None,
            ),
            ("etka.zip", "etka", None, None),
            ("ikarus_55_lux.zip", "ikarus 55 lux", None, None),
            (
                "highly_customizable_engines_rk.zip",
                "highly customizable engines rk",
                None,
                None,
            ),
            (
                "vesta sport-cross-oper-avtozvuk.zip",
                "vesta sport cross oper avtozvuk",
                None,
                None,
            ),
        ];
        for (file, name, author, version) in cases {
            let guessed = from_file_name(file);
            assert_eq!(guessed.name, *name, "{file}");
            assert_eq!(guessed.author.as_deref(), *author, "{file}");
            assert_eq!(guessed.version.as_deref(), *version, "{file}");
        }
    }

    #[test]
    fn bm_inside_a_name_is_kept() {
        assert_eq!(
            from_file_name("Nissan Laurel C33 BM.zip").name,
            "Nissan Laurel C33 BM"
        );
        assert_eq!(from_file_name("modland_racer.zip").name, "racer");
        assert_eq!(from_file_name("xmodlandx.zip").name, "xmodlandx");
        let song = from_file_name("Stand by Me.zip");
        assert_eq!((song.name.as_str(), song.author), ("Stand by Me", None));
        let repack = from_file_name("car_by_Uploader.zip");
        assert_eq!(repack.author, None, "«_by_» без «for beamng» — не автор");
    }

    #[test]
    fn reads_space_separated_version() {
        let guessed = from_file_name("Car 1 2.zip");
        assert_eq!(guessed.name, "Car");
        assert_eq!(guessed.version.as_deref(), Some("1.2"));
    }
}

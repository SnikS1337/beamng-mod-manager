//! Варианты запроса: транслитерация и частые синонимы.
//!
//! Сайты ищут по своей вёрстке: русский запрос на англоязычном сайте часто
//! возвращает пусто, хотя мод там есть. Варианты используются только когда
//! основной запрос ничего не дал — лишних запросов к сайтам не добавляем.

const SYNONYMS: &[(&str, &str)] = &[
    ("vaz", "lada"),
    ("ваз", "lada"),
    ("chevy", "chevrolet"),
    ("vw", "volkswagen"),
    ("mercedes-benz", "mercedes"),
    ("мерседес-бенц", "mercedes"),
    ("карта", "map"),
    ("машина", "car"),
    ("машины", "cars"),
];

pub fn variants(query: &str) -> Vec<String> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }
    let mut out = vec![query.to_string()];
    let lower = query.to_lowercase();
    if has_cyrillic(&lower) {
        push_unique(&mut out, transliterate_ru(&lower));
    }
    let replaced = replace_synonyms(&lower);
    if replaced != lower {
        push_unique(&mut out, replaced);
    }
    // Иногда полезно и обратное направление: английский запрос на русском сайте.
    if !has_cyrillic(&lower) {
        push_unique(&mut out, transliterate_latin_brands(&lower));
    }
    out.truncate(3);
    out
}

fn push_unique(out: &mut Vec<String>, value: String) {
    let value = value.trim().to_string();
    if value.chars().count() >= 2
        && !out
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&value))
    {
        out.push(value);
    }
}

fn has_cyrillic(text: &str) -> bool {
    text.chars().any(|c| ('\u{0400}'..='\u{04ff}').contains(&c))
}

fn replace_synonyms(text: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            SYNONYMS
                .iter()
                .find_map(|(from, to)| word.eq_ignore_ascii_case(from).then_some(*to))
                .unwrap_or(word)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn transliterate_latin_brands(text: &str) -> String {
    // Только частые названия, которые русские сайты чаще пишут кириллицей.
    let mut result = text.to_string();
    for (latin, cyrillic) in [
        ("lada", "лада"),
        ("niva", "нива"),
        ("vaz", "ваз"),
        ("mercedes", "мерседес"),
        ("bmw", "бмв"),
        ("audi", "ауди"),
        ("toyota", "тойота"),
        ("nissan", "ниссан"),
        ("volkswagen", "фольксваген"),
    ] {
        result = replace_word(&result, latin, cyrillic);
    }
    result
}

fn replace_word(text: &str, from: &str, to: &str) -> String {
    text.split_whitespace()
        .map(|word| {
            if word.eq_ignore_ascii_case(from) {
                to
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn transliterate_ru(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        let lower = ch.to_ascii_lowercase();
        let mapped = match lower {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' | 'ё' => "e",
            'ж' => "zh",
            'з' => "z",
            'и' => "i",
            'й' => "y",
            'к' => "k",
            'л' => "l",
            'м' => "m",
            'н' => "n",
            'о' => "o",
            'п' => "p",
            'р' => "r",
            'с' => "s",
            'т' => "t",
            'у' => "u",
            'ф' => "f",
            'х' => "kh",
            'ц' => "ts",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "shch",
            'ъ' | 'ь' => "",
            'ы' => "y",
            'э' => "e",
            'ю' => "yu",
            'я' => "ya",
            _ => {
                out.push(ch);
                continue;
            }
        };
        out.push_str(mapped);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::variants;

    #[test]
    fn keeps_original_and_transliterates_russian() {
        let got = variants("Лада Нива");
        assert_eq!(got[0], "Лада Нива");
        assert!(got.iter().any(|value| value == "lada niva"), "{got:?}");
    }

    #[test]
    fn expands_common_brands_and_words() {
        assert!(
            variants("vaz 2106")
                .iter()
                .any(|value| value == "lada 2106")
        );
        assert!(
            variants("Lada Niva")
                .iter()
                .any(|value| value == "лада нива")
        );
        assert!(variants("карта").iter().any(|value| value == "map"));
    }

    #[test]
    fn no_variants_for_plain_query() {
        assert_eq!(variants("Porsche 911"), vec!["Porsche 911"]);
    }
}

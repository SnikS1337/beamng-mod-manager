//! Релевантность широкой выдачи ModsGaming: слово запроса в названии или адресе.

use bmm_domain::models::Listing;

pub fn filter(query: &str, found: Vec<Listing>, ignore: &[&str]) -> Vec<Listing> {
    let words: Vec<String> = words(query)
        .into_iter()
        .filter(|word| word.chars().count() >= 2)
        .filter(|word| !ignore.iter().any(|known| known.eq_ignore_ascii_case(word)))
        .collect();
    if words.is_empty() {
        return found;
    }
    let latin: Vec<String> = words.iter().map(|word| translit(word)).collect();
    found
        .into_iter()
        .filter(|listing| {
            let title = fold(&listing.title);
            let path = fold(listing.url.path());
            words
                .iter()
                .any(|word| title.contains(word) || path.contains(word))
                || latin.iter().any(|word| {
                    word.chars().count() >= 2 && (path.contains(word) || title.contains(word))
                })
        })
        .collect()
}

fn words(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            current.push(ch);
        } else if !current.is_empty() {
            result.push(fold(&current));
            current.clear();
        }
    }
    if !current.is_empty() {
        result.push(fold(&current));
    }
    result
}

fn fold(text: &str) -> String {
    text.to_lowercase().replace('ё', "е")
}

fn translit(word: &str) -> String {
    let mut result = String::with_capacity(word.len());
    for ch in word.chars() {
        let mapped = match ch {
            'а' => "a",
            'б' => "b",
            'в' => "v",
            'г' => "g",
            'д' => "d",
            'е' => "e",
            'ж' => "zh",
            'з' => "z",
            'и' => "i",
            'й' => "j",
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
            'х' => "h",
            'ц' => "c",
            'ч' => "ch",
            'ш' => "sh",
            'щ' => "sch",
            'ъ' => "",
            'ы' => "y",
            'ь' => "",
            'э' => "e",
            'ю' => "ju",
            'я' => "ja",
            _ => "",
        };
        if mapped.is_empty() && !matches!(ch, 'ъ' | 'ь') {
            result.push(ch);
        } else {
            result.push_str(mapped);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use bmm_domain::models::Listing;

    fn listing(title: &str, path: &str) -> Listing {
        Listing::new(
            "modsgaming",
            url::Url::parse(&format!("https://modsgaming.ru{path}")).unwrap(),
            "1",
            title,
        )
    }

    #[test]
    fn keeps_matching_words_and_translit() {
        let found = vec![
            listing("BMW M2 G87", "/load/beamng_drive/mashiny/38-1-0-1"),
            listing("Ford Engines", "/load/beamng_drive/mashiny/38-1-0-2"),
        ];
        let filtered = filter(
            "bmw beamng.drive",
            found,
            &["beamng", "drive", "beamng.drive"],
        );
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].title.contains("BMW"));

        let russian = vec![listing("Мод", "/load/beamng_drive/mashiny/bmv_m5")];
        assert_eq!(
            filter("бмв", russian, &["beamng"]).len(),
            1,
            "транслит в адресе"
        );
    }

    #[test]
    fn empty_words_do_not_filter() {
        let found = vec![listing("Anything", "/x")];
        assert_eq!(filter("!!!", found, &[]).len(), 1);
    }
}

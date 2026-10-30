//! Подбор страницы для мода, который положили в папку сами.
//!
//! Мод ищется на сайтах по названию; предлагается одна страница, только если
//! она явно та: название совпадает почти целиком, автор и размер не спорят,
//! версия на сайте не старше установленной, и на том же сайте нет второй такой
//! же похожей. Если совпадение почти наверняка ([`PageGuess::certain`]), программа
//! начинает следить сама — сотни модов вручную не подтверждают; остальные страницы
//! ждут «Следить» или «Это другой мод».

use std::collections::HashSet;

use chrono::{DateTime, FixedOffset, TimeDelta};

use bmm_domain::filename::from_file_name;
use bmm_domain::library::{InstalledMod, LibraryStore};
use bmm_domain::models::{Listing, PriceStatus};
use bmm_domain::text::{TitleNormalizer, TitleParser};
use bmm_domain::versioning::{VersionComparer, VersionOrder};

use crate::bindings::BindingSuggester;

/// Ничего не нашли — следующий поиск для этого мода не раньше.
pub const RETRY_AFTER: TimeDelta = TimeDelta::days(14);
/// Сколько модов искать за одну проверку: каждый — несколько запросов к каждому сайту.
pub const PER_RUN: usize = 60;
/// Доля совпавших слов названия (Жаккар), с которой страница считается «тем же модом».
pub const MIN_NAME: f64 = 0.8;
/// Вторая страница того же сайта ближе этого к лучшей — непонятно, какая из них.
pub const AMBIGUOUS: f64 = 0.05;
/// Прибавка, если автор страницы написан в имени файла мода или совпал с автором
/// из архива.
pub const AUTHOR_IN_FILE_BONUS: f64 = 0.15;
/// Порог похожести названия, когда автор совпал: «ВАЗ-2104/2105/2107» от VangohMods
/// и «LADA 2104/2105/2107» из архива VangohMods — один мод, хотя слова разные.
pub const MIN_NAME_SAME_AUTHOR: f64 = 0.5;
/// Похожесть, если склеенное имя файла целиком есть в адресе страницы
/// («dragpartswheelsnitrous» → …/meo_s_drag_parts_wheels_nitrous_by_…).
pub const NAME_IN_ADDRESS: f64 = 0.9;
/// Минимальное отношение размеров установленного мода и страницы.
pub const SIZE_MIN_RATIO: f64 = 0.85;
/// Название, с которым следить можно без вопроса (при одном подтверждающем признаке).
pub const CERTAIN_NAME: f64 = 0.9;
/// Размеры архива и страницы, которые считаются «тем же файлом».
pub const CERTAIN_SIZE_RATIO: f64 = 0.97;

/// Предложенная страница: запись на сайте и насколько она похожа.
#[derive(Debug, Clone, PartialEq)]
pub struct PageGuess {
    pub listing: Listing,
    pub score: f64,
    /// Почти наверняка тот же мод: название совпадает (≥ 0.9, от двух слов) и есть
    /// подтверждение — та же версия, тот же размер или полное совпадение длинного
    /// названия (от трёх слов). Тогда следим без вопроса.
    pub certain: bool,
}

/// Подбор страницы по названию.
pub struct PageGuesser;

impl PageGuesser {
    /// Моды, для которых пора искать страницу: без привязок, без отказа
    /// «Не проверять», не искали недавно; новые первыми, не больше [`PER_RUN`].
    pub fn due(store: &dyn LibraryStore, now: DateTime<FixedOffset>) -> Vec<InstalledMod> {
        Self::due_with(store, now, true)
    }

    /// Как [`Self::due`], но можно не искать страницы для выключенных модов.
    pub fn due_with(
        store: &dyn LibraryStore,
        now: DateTime<FixedOffset>,
        include_disabled: bool,
    ) -> Vec<InstalledMod> {
        Self::due_for(store, now, include_disabled, false)
    }

    /// Как [`Self::due_with`]; `retry_recent` — человек сам нажал «Найти страницы»:
    /// ищем и те моды, что искали недавно. Отказы («это не мой мод», «не проверять»)
    /// по-прежнему не трогаем.
    pub fn due_for(
        store: &dyn LibraryStore,
        now: DateTime<FixedOffset>,
        include_disabled: bool,
        retry_recent: bool,
    ) -> Vec<InstalledMod> {
        let bound: HashSet<String> = store
            .bindings()
            .into_iter()
            .map(|binding| binding.mod_id)
            .collect();
        let retry_after = RETRY_AFTER;
        let mut due: Vec<InstalledMod> = store
            .mods()
            .into_iter()
            .filter(|installed| !bound.contains(&installed.id))
            .filter(|installed| include_disabled || installed.enabled)
            .filter(|installed| !store.is_checking_declined(&installed.id))
            .filter(|installed| match store.get_page_search(&installed.id) {
                None => true,
                Some(search) => {
                    !search.rejected
                        && (retry_recent || now - search.at >= retry_after || search.at > now)
                }
            })
            .collect();
        // `sort_by_key` устойчива: при одинаковом времени установки порядок остаётся
        // прежним (по названию из хранилища), как и в legacy.
        due.sort_by_key(|installed| std::cmp::Reverse(installed.installed_at_utc));
        due.truncate(PER_RUN);
        due
    }

    /// Чем искать мод на сайтах: лучшее известное название, иначе угаданное из имени файла.
    pub fn query_for(installed: &InstalledMod, title: Option<&str>) -> String {
        match title.map(str::trim) {
            Some(title) if !title.is_empty() => title.to_string(),
            _ => BindingSuggester::query_for(installed),
        }
    }

    /// Выбирает одну страницу или ничего, если лучшая неоднозначна.
    ///
    /// `title` — название из окна (репозиторий, info.json, сайт); `None` — только
    /// имя файла и запись библиотеки.
    pub fn pick(
        installed: &InstalledMod,
        title: Option<&str>,
        found: &[Listing],
    ) -> Option<PageGuess> {
        Self::pick_with_author(installed, title, None, found)
    }

    /// Как [`Self::pick`], но знает автора из архива мода (`info.json`). Страница другого
    /// автора не отбрасывается — авторы у новых версий добавляются и пишутся по-разному, —
    /// но и следить за ней сами не начинаем: только предлагаем.
    pub fn pick_with_author(
        installed: &InstalledMod,
        title: Option<&str>,
        archive_author: Option<&str>,
        found: &[Listing],
    ) -> Option<PageGuess> {
        let guess = from_file_name(&installed.file_name);
        let candidates = [
            title,
            Some(installed.name.as_str()),
            Some(guess.name.as_str()),
        ];
        let mut names: Vec<&str> = Vec::new();
        for name in candidates.into_iter().flatten() {
            let name = name.trim();
            if name.is_empty() || TitleNormalizer::tokens(name).is_empty() {
                continue;
            }
            if names
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(name))
            {
                continue;
            }
            names.push(name);
        }
        if names.is_empty() {
            return None;
        }

        let installed_version = installed.version.clone().or_else(|| guess.version.clone());
        let author = author_key(guess.author.as_deref());
        let file_key = author_key(Some(&installed.file_name));
        // Имя файла одним словом без разделителей — его ищем в адресе страницы.
        let glued = {
            let stem = installed
                .file_name
                .rsplit_once('.')
                .map_or(installed.file_name.as_str(), |(stem, _)| stem);
            if stem.chars().all(char::is_alphanumeric) {
                stem.to_lowercase()
            } else {
                String::new()
            }
        };

        let mut seen: HashSet<(String, String)> = HashSet::new();
        let mut fits: Vec<PageGuess> = Vec::new();
        // Для каждой подходящей страницы: (тот же автор, точно другой автор).
        let mut authorship: Vec<(bool, bool)> = Vec::new();
        for listing in found {
            if !seen.insert((listing.source.clone(), listing.source_id.clone()))
                || listing.price == PriceStatus::Paid
            {
                continue;
            }

            // Одно слово («police») совпадает со слишком многим: только целиком.
            let mut similarity = 0.0f64;
            let mut tokens = 0usize;
            for name in &names {
                let candidate = TitleNormalizer::similarity(name, &listing.title);
                if similarity == 0.0 || candidate > similarity {
                    similarity = candidate;
                    tokens = TitleNormalizer::tokens(name).len();
                }
            }
            if glued.chars().count() >= 12
                && author_key(Some(listing.url.path())).contains(glued.as_str())
                && similarity < NAME_IN_ADDRESS
            {
                similarity = NAME_IN_ADDRESS;
                tokens = tokens.max(2);
            }
            // Автор страницы в имени файла («NissanSkylineER34_Ketinside.zip») или тот же,
            // что в архиве.
            let listing_author = author_key(listing.author.as_deref());
            let author_in_file =
                listing_author.chars().count() >= 4 && file_key.contains(listing_author.as_str());
            let same_author =
                author_in_file || authors_match(archive_author, listing.author.as_deref());
            let min_name = if same_author {
                MIN_NAME_SAME_AUTHOR
            } else {
                MIN_NAME
            };
            if similarity < min_name || (tokens < 2 && similarity < 1.0) {
                continue;
            }
            if !author.is_empty()
                && let Some(listing_author) = listing.author.as_deref()
            {
                let listing_author = author_key(Some(listing_author));
                if !listing_author.is_empty() && listing_author != author {
                    continue;
                }
            }
            if let Some(installed_size) = installed.size_bytes
                && installed_size > 0
                && let Some(listing_size) = listing.size_bytes
                && listing_size > 0
            {
                let smaller = installed_size.min(listing_size) as f64;
                let larger = installed_size.max(listing_size) as f64;
                if smaller / larger < SIZE_MIN_RATIO {
                    continue;
                }
            }

            let site_version = listing
                .mod_version
                .clone()
                .or_else(|| TitleParser::parse(&listing.title).mod_version);
            let mut score = similarity;
            let order =
                VersionComparer::compare(installed_version.as_deref(), site_version.as_deref());
            match order {
                VersionOrder::Older => continue, // на сайте старее установленного — это не его страница
                VersionOrder::Same => score += 0.1,
                VersionOrder::Newer => score += 0.02,
                VersionOrder::Unknown => {}
            }
            let same_size = match (installed.size_bytes, listing.size_bytes) {
                (Some(mine), Some(theirs)) if mine > 0 && theirs > 0 => {
                    mine.min(theirs) as f64 / mine.max(theirs) as f64 >= CERTAIN_SIZE_RATIO
                }
                _ => false,
            };
            // Тот же автор — сильный признак: две страницы «Nissan Skyline (ER34)» разных
            // авторов иначе спорили бы друг с другом, и мод остался бы без страницы.
            if same_author {
                score += AUTHOR_IN_FILE_BONUS;
            }
            let other_author =
                !same_author && !authors_overlap(archive_author, listing.author.as_deref());
            let certain = (similarity >= CERTAIN_NAME
                && tokens >= 2
                && !other_author
                && (order == VersionOrder::Same
                    || same_size
                    // Совпало длинное название целиком — но у страницы должен быть хоть
                    // какой-то признак (автор или версия): безымянная старая страница
                    // «Porsche 911 GT3 RS» на ModsGaming — не повод следить за ней молча.
                    || (similarity >= 1.0
                        && tokens >= 3
                        && (listing.author.is_some() || site_version.is_some()))))
                // Тот же автор и та же версия при похожем названии — тот же мод.
                || (same_author && order == VersionOrder::Same);
            fits.push(PageGuess {
                listing: listing.clone(),
                score,
                certain,
            });
            authorship.push((same_author, other_author));
        }
        // Нашлась страница того же автора — остальные не нужны: «Maserati GranTurismo»
        // от Flanje, а не одноимённый мод Cheekqo и не безымянные копии на ModsGaming.
        if authorship.iter().any(|(same, _)| *same) {
            let mut index = 0;
            fits.retain(|_| {
                let keep = authorship[index].0;
                index += 1;
                keep
            });
        }
        if fits.is_empty() {
            return None;
        }

        // Первый максимум, как `OrderByDescending(...).First()` в legacy.
        let mut best = 0usize;
        for index in 1..fits.len() {
            if fits[index].score > fits[best].score {
                best = index;
            }
        }
        let rival = fits.iter().enumerate().any(|(index, other)| {
            index != best
                && other.listing.source == fits[best].listing.source
                && other.listing.source_id != fits[best].listing.source_id
                && fits[best].score - other.score < AMBIGUOUS
        });
        if rival {
            None
        } else {
            Some(fits[best].clone())
        }
    }
}

/// Авторы, по которым мод не отличить: заготовка игры, «неизвестен».
const GENERIC_AUTHORS: &[&str] = &["beamng", "beamnggmbh", "unknown", "неизвестен", "author"];

/// Авторы известны с обеих сторон и совпадают хотя бы одним именем.
pub(crate) fn authors_match(archive: Option<&str>, listing: Option<&str>) -> bool {
    let known = |text: Option<&str>| {
        text.is_some_and(|text| {
            let key = author_key(Some(text));
            key.chars().count() >= 2 && !GENERIC_AUTHORS.contains(&key.as_str())
        })
    };
    known(archive) && known(listing) && authors_overlap(archive, listing)
}

/// Авторы мода и страницы пересекаются хотя бы одним именем. Неизвестный с любой
/// стороны (или общий вроде «BeamNG») — не спорит.
fn authors_overlap(archive: Option<&str>, listing: Option<&str>) -> bool {
    let names = |text: Option<&str>| -> Vec<String> {
        let Some(text) = text else {
            return Vec::new();
        };
        text.replace(" and ", ",")
            .replace(" и ", ",")
            .split([',', '&', '/', ';', '+'])
            .map(|name| author_key(Some(name)))
            .filter(|key| key.chars().count() >= 2 && !GENERIC_AUTHORS.contains(&key.as_str()))
            .collect()
    };
    let ours = names(archive);
    let theirs = names(listing);
    if ours.is_empty() || theirs.is_empty() {
        return true;
    }
    ours.iter().any(|mine| {
        theirs.iter().any(|other| {
            mine == other
                || (mine.chars().count() >= 4 && other.contains(mine.as_str()))
                || (other.chars().count() >= 4 && mine.contains(other.as_str()))
        })
    })
}

fn author_key(author: Option<&str>) -> String {
    let Some(author) = author else {
        return String::new();
    };
    if author.trim().is_empty() {
        return String::new();
    }
    author
        .chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use url::Url;

    use bmm_domain::library::{InMemoryLibraryStore, ModBinding, PageSearch};

    fn now() -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339("2026-10-18T12:00:00+00:00").unwrap()
    }

    fn installed(id: &str, file: &str, version: Option<&str>, size: Option<i64>) -> InstalledMod {
        InstalledMod {
            id: id.to_string(),
            name: from_file_name(file).name,
            file_name: file.to_string(),
            version: version.map(str::to_string),
            size_bytes: size,
            installed_at_utc: now() - TimeDelta::days(1),
            enabled: true,
        }
    }

    fn listing(
        source: &str,
        id: &str,
        title: &str,
        author: Option<&str>,
        size: Option<i64>,
        price: PriceStatus,
    ) -> Listing {
        let mut listing = Listing::new(
            source,
            Url::parse(&format!("https://{source}.test/{id}")).unwrap(),
            id,
            title,
        );
        listing.author = author.map(str::to_string);
        listing.size_bytes = size;
        listing.price = price;
        listing
    }

    fn l(source: &str, id: &str, title: &str) -> Listing {
        listing(source, id, title, None, None, PriceStatus::Unknown)
    }

    #[test]
    fn other_author_is_suggested_not_followed() {
        let site = listing(
            "stmods",
            "1",
            "Maserati GranTurismo v1.0",
            Some("cheekqo"),
            None,
            PriceStatus::Free,
        );
        let guess = PageGuesser::pick_with_author(
            &installed("a", "Flanje_Maserati_Granturismo.zip", Some("1.0"), None),
            Some("Maserati GranTurismo"),
            Some("Flanje"),
            &[site],
        )
        .expect("страницу предлагаем");
        assert!(!guess.certain, "другой автор — только предложение");

        // Новая версия с двумя авторами, в архиве один из них — тот же мод.
        let laurel = listing(
            "stmods",
            "2",
            "Nissan Laurel C33 v1.2",
            Some("Ruzskye Stvoly, black_moon"),
            None,
            PriceStatus::Free,
        );
        let guess = PageGuesser::pick_with_author(
            &installed("b", "laurel_c33.zip", Some("1.2"), None),
            Some("Nissan Laurel C33"),
            Some("Ruzskye Stvoly"),
            &[laurel],
        )
        .unwrap();
        assert!(guess.certain);
    }

    #[test]
    fn author_in_file_name_settles_two_pages_with_the_same_title() {
        // Реальный случай: два «Nissan Skyline (ER34)» на STMods, в архиве автор «ACM».
        let ketinside = listing(
            "stmods",
            "er34-k",
            "Nissan Skyline (ER34) версия 2.0",
            Some("Ketinside"),
            None,
            PriceStatus::Free,
        );
        let other = listing(
            "stmods",
            "er34-m",
            "Nissan Skyline (ER34) версия 1.1",
            Some("Для MotorSports"),
            None,
            PriceStatus::Free,
        );
        let mine = installed("er34", "NissanSkylineER34_Ketinside.zip", None, None);
        let guess = PageGuesser::pick_with_author(
            &mine,
            Some("Nissan Skyline ER34"),
            Some("ACM"),
            &[other.clone(), ketinside.clone()],
        )
        .expect("страница автора из имени файла");
        assert_eq!(guess.listing.source_id, "er34-k");

        // Без автора в имени файла — двусмысленно, не гадаем.
        let plain = installed("er34", "NissanSkylineER34.zip", None, None);
        assert!(
            PageGuesser::pick_with_author(
                &plain,
                Some("Nissan Skyline ER34"),
                Some("ACM"),
                &[other, ketinside]
            )
            .is_none()
        );
    }

    #[test]
    fn same_author_finds_pages_with_other_words() {
        // Реальные заголовки STMods для модов из скачанного набора.
        let vaz = listing(
            "stmods",
            "vaz",
            "ВАЗ-2104/2105/2107 версия 0.2.3 для BeamNG.drive (v0.38.x)",
            Some("VangohMods"),
            None,
            PriceStatus::Free,
        );
        let other_vaz = listing(
            "stmods",
            "vaz-nsd",
            "ВАЗ-2104/2105/2107 версия 1.1 Fix 3 для BeamNG.drive (v0.39.x)",
            Some("NSD mods"),
            None,
            PriceStatus::Free,
        );
        let guess = PageGuesser::pick_with_author(
            &installed(
                "v",
                "VAZ_2104_2105_2107VangohMods_v0.2.3.zip",
                Some("0.2.3"),
                None,
            ),
            Some("LADA 2104/2105/2107"),
            Some("VangohMods"),
            &[other_vaz, vaz],
        )
        .expect("та же версия того же автора");
        assert_eq!(guess.listing.source_id, "vaz");
        assert!(guess.certain);

        // Maserati: страница Flanje вернее одноимённой страницы другого автора.
        let cheekqo = listing(
            "stmods",
            "m-c",
            "Maserati Granturismo версия 1.1 для BeamNG.drive (v0.23.5.2)",
            Some("Cheekqo"),
            None,
            PriceStatus::Free,
        );
        let flanje = listing(
            "stmods",
            "m-f",
            "Maserati GranTurismo (2023-2026) версия 1.0 для BeamNG.drive (v0.39.x)",
            Some("Flanje"),
            None,
            PriceStatus::Free,
        );
        let guess = PageGuesser::pick_with_author(
            &installed("m", "Maserati_Granturismo.zip", None, None),
            Some("Maserati GranTurismo"),
            Some("Flanje"),
            &[cheekqo, flanje],
        )
        .unwrap();
        assert_eq!(guess.listing.source_id, "m-f");

        // Карта: «Карта «Sumner Road»» того же автора.
        let sumner = listing(
            "stmods",
            "s",
            "Карта «Sumner Road» версия 1.0 для BeamNG.drive (v0.40.x)",
            Some("DrowsySam"),
            None,
            PriceStatus::Free,
        );
        assert!(
            PageGuesser::pick_with_author(
                &installed("s", "Sumner_Road_tg_m0dsbeamng.zip", None, None),
                Some("Sumner Road"),
                Some("DrowsySam"),
                &[sumner],
            )
            .is_some()
        );
    }

    #[test]
    fn same_author_page_beats_unsigned_copies() {
        let flanje = listing(
            "stmods",
            "m-f",
            "Maserati GranTurismo (2023-2026) версия 1.0 для BeamNG.drive (v0.39.x)",
            Some("Flanje"),
            None,
            PriceStatus::Free,
        );
        let copy_a = l(
            "modsgaming",
            "a",
            "Мод автомобиль Maserati GranTurismo для BeamNG . drive",
        );
        let copy_b = l(
            "modsgaming",
            "b",
            "Мод автомобиль Maserati Granturismo для BeamNG . drive",
        );
        let guess = PageGuesser::pick_with_author(
            &installed("m", "Maserati_Granturismo.zip", None, None),
            Some("Maserati GranTurismo"),
            Some("Flanje"),
            &[copy_a, copy_b, flanje],
        )
        .expect("страница автора");
        assert_eq!(guess.listing.source_id, "m-f");
    }

    #[test]
    fn exact_name_alone_does_not_follow_unsigned_page() {
        let old = l(
            "modsgaming",
            "p",
            "Мод автомобиль Porsche 911 GT3 RS для BeamNG . drive",
        );
        let guess = PageGuesser::pick_with_author(
            &installed("p", "porsche_992_gt3rs_v2.0_modland.zip", Some("2.0"), None),
            Some("Porsche 911 GT3 RS (992.1)"),
            None,
            &[old],
        )
        .expect("предлагаем");
        assert!(!guess.certain, "без автора и версии — только предложение");
    }

    #[test]
    fn glued_file_name_is_found_in_page_address() {
        let mut meo = listing(
            "stmods",
            "meo",
            "Meo’s Drag Parts Pack версия 2.2 для BeamNG.drive",
            Some("Meo_3000_QC"),
            None,
            PriceStatus::Free,
        );
        meo.url = Url::parse(
            "https://stmods.org/beamng_drive/mods/meo_s_drag_parts_wheels_nitrous_by_meo_3000_qc_for_beamng/",
        )
        .unwrap();
        let guess = PageGuesser::pick_with_author(
            &installed("d", "dragpartswheelsnitrous.zip", None, None),
            None,
            None,
            &[meo],
        )
        .expect("имя файла целиком в адресе");
        assert!(!guess.certain, "по адресу — только предложение");
    }

    #[test]
    fn different_known_author_with_weak_name_is_skipped() {
        let page = listing(
            "stmods",
            "x",
            "Lada 2106 Sport версия 1.0 для BeamNG.drive",
            Some("Someone"),
            None,
            PriceStatus::Free,
        );
        assert!(
            PageGuesser::pick_with_author(
                &installed("l", "lada.zip", Some("1.0"), None),
                Some("LADA 2106 (Remaster Version)"),
                Some("USSR Project"),
                &[page],
            )
            .is_none()
        );
    }

    #[test]
    fn authors_overlap_ignores_generic_and_unknown() {
        assert!(authors_overlap(Some("BeamNG"), Some("turbodiesel")));
        assert!(authors_overlap(None, Some("x")));
        assert!(authors_overlap(
            Some("black_moon & Ruzskye Stvoly"),
            Some("Black Moon")
        ));
        assert!(!authors_overlap(Some("Flanje"), Some("cheekqo")));
    }

    #[test]
    fn certain_match_needs_a_confirming_sign() {
        let mb = 1024 * 1024;
        // Название из архива и та же версия на сайте — следим сами.
        let same_version = PageGuesser::pick(
            &installed("a", "daciasanderomk1_tbdiesel.zip", Some("1.2"), None),
            Some("Dacia Sandero (B90)"),
            &[l("stmods", "1", "Dacia Sandero (B90) v1.2")],
        )
        .unwrap();
        assert!(same_version.certain);

        // Тот же размер архива (в пределах 3 %).
        let same_size = PageGuesser::pick(
            &installed("b", "laurel_c33.zip", None, Some(541 * mb)),
            Some("Nissan Laurel C33"),
            &[listing(
                "modsgaming",
                "2",
                "Nissan Laurel C33",
                None,
                Some(545 * mb),
                PriceStatus::Free,
            )],
        )
        .unwrap();
        assert!(same_size.certain);

        // Длинное название совпало целиком, версий и размеров нет, у страницы есть автор.
        let exact = PageGuesser::pick(
            &installed("c", "x.zip", None, None),
            Some("Maserati GranTurismo Trofeo"),
            &[listing(
                "stmods",
                "3",
                "Maserati GranTurismo Trofeo",
                Some("Flanje"),
                None,
                PriceStatus::Free,
            )],
        )
        .unwrap();
        assert!(exact.certain);

        // Два слова, на сайте новее, размеров нет — только предложение.
        let unsure = PageGuesser::pick(
            &installed("d", "toyota_sienna_v1.0.zip", Some("1.0"), None),
            None,
            &[l("stmods", "4", "Toyota Sienna v2.0")],
        )
        .unwrap();
        assert!(!unsure.certain);
    }

    #[test]
    fn real_modsgaming_search_finds_sienna_and_collapses_reposts() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/modsgaming/search-real.html");
        let html = std::fs::read_to_string(path).unwrap();
        let found =
            bmm_sources::search::newest_only(bmm_sources::parsers::modsgaming::parse_search(&html));

        let guess = PageGuesser::pick(
            &installed("a", "toyota_sienna_2017_v1.0.zip", Some("1.0"), None),
            None,
            &found,
        )
        .expect("страница v2.0 находится");
        assert!(guess.listing.title.contains("Sienna 2017 v2.0"));

        // Две карточки «2023 Toyota Sienna» — перепост одного мода: в выдаче
        // остаётся свежая, и страница для установленного файла находится.
        let guess = PageGuesser::pick(
            &installed("b", "2023_toyota_sienna.zip", None, None),
            None,
            &found,
        )
        .expect("перепост склеен, страница находится");
        assert!(guess.listing.title.to_lowercase().contains("2023"));
    }

    #[test]
    fn rejects_other_name_author_size_older_paid_and_single_word() {
        let bmw = installed(
            "a",
            "bmw_e34_by_matissator_for_beamng_v3.3.zip",
            Some("3.3"),
            Some(100_000_000),
        );
        let good = PageGuesser::pick(
            &bmw,
            None,
            &[listing(
                "stmods",
                "1",
                "BMW E34 версия 3.3",
                Some("matissator"),
                Some(101_000_000),
                PriceStatus::Unknown,
            )],
        )
        .expect("та же страница");
        assert_eq!(TitleNormalizer::normalize(&good.listing.title), "bmw e34");

        assert!(
            PageGuesser::pick(&bmw, None, &[l("stmods", "2", "BMW E30 версия 3.3")]).is_none(),
            "другое название"
        );
        assert!(
            PageGuesser::pick(
                &bmw,
                None,
                &[listing(
                    "stmods",
                    "3",
                    "BMW E34 версия 3.3",
                    Some("someone"),
                    None,
                    PriceStatus::Unknown
                )]
            )
            .is_none(),
            "другой автор"
        );
        assert!(
            PageGuesser::pick(
                &bmw,
                None,
                &[listing(
                    "stmods",
                    "4",
                    "BMW E34 версия 3.3",
                    None,
                    Some(40_000_000),
                    PriceStatus::Unknown
                )]
            )
            .is_none(),
            "размер"
        );
        assert!(
            PageGuesser::pick(&bmw, None, &[l("stmods", "5", "BMW E34 версия 2.0")]).is_none(),
            "на сайте старее установленного"
        );
        assert!(
            PageGuesser::pick(
                &bmw,
                None,
                &[listing(
                    "modland",
                    "6",
                    "BMW E34 v3.3",
                    None,
                    None,
                    PriceStatus::Paid
                )]
            )
            .is_none(),
            "платный"
        );
        assert!(
            PageGuesser::pick(
                &installed("p", "police.zip", None, None),
                None,
                &[l("stmods", "7", "Police Pack")]
            )
            .is_none(),
            "одно слово — только целиком"
        );
        assert!(
            PageGuesser::pick(
                &installed("p", "police.zip", None, None),
                None,
                &[l("stmods", "8", "Police")]
            )
            .is_some(),
            "одно слово целиком — да"
        );
    }

    #[test]
    fn window_title_beats_file_name_and_first_site_wins() {
        let mod_without_title = installed("a", "taa_final.zip", None, None);
        let found = [
            l("modsgaming", "1", "Temporal Anti Aliasing v1.7"),
            l("stmods", "2", "Temporal Anti Aliasing 1.7"),
        ];
        assert!(
            PageGuesser::pick(&mod_without_title, None, &found).is_none(),
            "по имени файла не узнать"
        );
        assert_eq!(
            PageGuesser::pick(&mod_without_title, Some("Temporal Anti Aliasing"), &found)
                .unwrap()
                .listing
                .source,
            "modsgaming"
        );
        assert_eq!(
            PageGuesser::query_for(&mod_without_title, Some("Temporal Anti Aliasing")),
            "Temporal Anti Aliasing"
        );
        assert_eq!(
            PageGuesser::query_for(&mod_without_title, None),
            "taa final"
        );
    }

    #[test]
    fn version_bonuses_and_thresholds() {
        let mod_without_version = installed("a", "some_car.zip", None, None);
        let same = PageGuesser::pick(
            &mod_without_version,
            None,
            &[l("stmods", "1", "Some Car v2.0")],
        )
        .unwrap();
        assert!(
            same.score >= 1.0,
            "бонус за совпадение версии: {}",
            same.score
        );

        let one_word_too_short = installed("b", "german_cars_pack.zip", None, None);
        assert!(
            PageGuesser::pick(
                &one_word_too_short,
                None,
                &[l("stmods", "2", "German Cars Pack Deluxe")]
            )
            .is_none(),
            "0.75 < 0.8"
        );
    }

    #[test]
    fn due_skips_bound_declined_and_fresh_and_takes_newest() {
        let store = InMemoryLibraryStore::new();
        for index in 0..(PER_RUN as i64 + 5) {
            let mut installed = installed(
                &format!("m{index}"),
                &format!("mod_{index}.zip"),
                None,
                None,
            );
            installed.installed_at_utc = now() - TimeDelta::days(index);
            store.upsert_mod(installed);
        }
        store
            .upsert_binding(ModBinding {
                mod_id: "m0".to_string(),
                source: "stmods".to_string(),
                source_id: "1".to_string(),
                url: Url::parse("https://stmods.test/1").unwrap(),
                confirmed: true,
            })
            .unwrap();
        store
            .upsert_binding(ModBinding {
                mod_id: "m1".to_string(),
                source: "stmods".to_string(),
                source_id: "2".to_string(),
                url: Url::parse("https://stmods.test/2").unwrap(),
                confirmed: false,
            })
            .unwrap();
        store.set_checking_declined("m2", true);
        store.set_page_search(
            "m3",
            Some(PageSearch {
                at: now() - TimeDelta::days(3),
                rejected: false,
            }),
        );
        store.set_page_search(
            "m4",
            Some(PageSearch {
                at: now() - TimeDelta::days(15),
                rejected: false,
            }),
        );
        store.set_page_search(
            "m5",
            Some(PageSearch {
                at: now() - TimeDelta::days(30),
                rejected: true,
            }),
        );

        let due = PageGuesser::due(&store, now());
        let ids: Vec<&str> = due.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(ids.len(), PER_RUN);
        assert_eq!(&ids[..3], &["m4", "m6", "m7"]);
        assert!(
            ids.iter()
                .all(|id| !["m0", "m1", "m2", "m3", "m5"].contains(id))
        );
    }

    #[test]
    fn manual_search_retries_recent_attempts_but_not_refusals() {
        // Было: «Найти страницы для остальных модов» молчало две недели после первого
        // поиска — все моды без страницы уже «искали», и кнопка ничего не делала.
        let store = InMemoryLibraryStore::new();
        for id in ["recent", "refused", "declined"] {
            store.upsert_mod(installed(id, &format!("{id}.zip"), None, None));
        }
        store.set_page_search(
            "recent",
            Some(PageSearch {
                at: now() - TimeDelta::days(3),
                rejected: false,
            }),
        );
        store.set_page_search(
            "refused",
            Some(PageSearch {
                at: now() - TimeDelta::days(3),
                rejected: true,
            }),
        );
        store.set_checking_declined("declined", true);

        assert!(PageGuesser::due_for(&store, now(), true, false).is_empty());
        let manual: Vec<String> = PageGuesser::due_for(&store, now(), true, true)
            .into_iter()
            .map(|item| item.id)
            .collect();
        assert_eq!(manual, ["recent"]);
    }

    #[test]
    fn disabled_mods_are_skipped_when_page_search_is_limited() {
        let store = InMemoryLibraryStore::new();
        let mut off = installed("off", "off.zip", None, None);
        off.enabled = false;
        store.upsert_mod(off);
        store.upsert_mod(installed("on", "on.zip", None, None));

        let all = PageGuesser::due(&store, now());
        assert_eq!(all.len(), 2);
        let enabled = PageGuesser::due_with(&store, now(), false);
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "on");
    }

    #[test]
    fn future_page_search_is_retried() {
        let store = InMemoryLibraryStore::new();
        store.upsert_mod(installed("a", "car.zip", None, None));
        store.set_page_search(
            "a",
            Some(PageSearch {
                at: now() + TimeDelta::days(2),
                rejected: false,
            }),
        );
        assert_eq!(PageGuesser::due(&store, now()).len(), 1);
    }
}

//! Оценка качества поиска на фиксированных наборах: слияние, порядок,
//! вопросы о дублях и подстановки запроса. Метрика внутренняя: запускать вручную,
//! в обычный прогон она не входит.

use std::sync::Arc;

use bmm_domain::models::Listing;
use url::Url;

use super::query;
use super::{SearchOptions, SearchService, SourceStatus};

struct Case {
    name: &'static str,
    query: &'static str,
    listings: Vec<Listing>,
    /// Ожидаемая карточка для каждой записи: одинаковый номер — один мод.
    groups: Vec<usize>,
    /// Запись, которая должна оказаться в первой карточке (если порядок важен).
    top: Option<usize>,
}

fn listing(
    source: &str,
    id: &str,
    title: &str,
    author: Option<&str>,
    size: Option<i64>,
) -> Listing {
    let mut item = Listing::new(
        source,
        Url::parse(&format!("https://{source}.example.org/{id}")).unwrap(),
        id,
        title,
    );
    item.author = author.map(str::to_string);
    item.size_bytes = size;
    item
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            name: "один мод на двух сайтах",
            query: "bmw m2",
            listings: vec![
                listing("modland", "a", "BMW M2 G87", Some("Author"), Some(100)),
                listing("stmods", "b", "BMW M2 G87", Some("Author"), Some(100)),
            ],
            groups: vec![0, 0],
            top: Some(0),
        },
        Case {
            name: "один мод на трёх сайтах",
            query: "rolls royce",
            listings: vec![
                listing(
                    "modland",
                    "a",
                    "Rolls Royce Phantom",
                    Some("Author"),
                    Some(100),
                ),
                listing(
                    "stmods",
                    "b",
                    "Rolls-Royce Phantom",
                    Some("Author"),
                    Some(100),
                ),
                listing(
                    "modsgaming",
                    "c",
                    "Rolls Royce Phantom",
                    Some("Author"),
                    Some(100),
                ),
            ],
            groups: vec![0, 0, 0],
            top: Some(0),
        },
        Case {
            name: "точное название выше длинного",
            query: "civic",
            listings: vec![
                listing(
                    "modland",
                    "a",
                    "Honda Civic Type R Pack Deluxe",
                    Some("Author"),
                    Some(100),
                ),
                listing("modland", "b", "Civic", Some("Other"), Some(50)),
            ],
            groups: vec![0, 1],
            top: Some(1),
        },
        Case {
            name: "похожие названия не склеиваются",
            query: "roadster",
            listings: vec![
                listing("modland", "a", "Alpha Roadster", Some("Author"), Some(100)),
                listing("stmods", "b", "Alpha Hauler", Some("Author"), Some(100)),
            ],
            groups: vec![0, 1],
            top: Some(0),
        },
        Case {
            name: "похожий мод без общих примет",
            query: "sienna",
            listings: vec![
                listing("modland", "a", "Toyota Sienna 2011-2017 2.2", None, None),
                listing(
                    "stmods",
                    "b",
                    "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE",
                    None,
                    None,
                ),
            ],
            groups: vec![0, 1],
            top: None,
        },
    ]
}

/// Запрос → подстановка, которая обязана появиться.
const VARIANTS: &[(&str, &str)] = &[
    ("Лада Нива", "lada niva"),
    ("ваз 2106", "lada 2106"),
    ("Lada Niva", "лада нива"),
    ("Audi RS3", "ауди"),
    ("карта", "map"),
    ("vw golf", "volkswagen golf"),
];

fn card_positions(cards: &[super::ModCard], listings: &[Listing]) -> Vec<Option<usize>> {
    listings
        .iter()
        .map(|listing| {
            cards.iter().position(|card| {
                card.listings.iter().any(|item| {
                    item.source == listing.source && item.source_id == listing.source_id
                })
            })
        })
        .collect()
}

#[test]
#[ignore = "метрика: cargo test -p bmm-sources search_quality -- --ignored --nocapture"]
fn search_quality() {
    let service = SearchService::new(
        Vec::new(),
        Arc::new(|_| SourceStatus::Ok),
        SearchOptions::default(),
        None,
    );
    let cases = cases();
    let mut expected_pairs = 0usize;
    let mut merged_pairs = 0usize;
    let mut correct_pairs = 0usize;
    let mut ranked = 0usize;
    let mut first = 0usize;
    let mut reciprocal = 0.0;
    let mut questions = 0usize;

    println!("Поиск: {} сценариев", cases.len());
    for case in &cases {
        let cards = service.group(&case.listings, case.query);
        let positions = card_positions(&cards, &case.listings);
        let mut merged = 0usize;
        let mut correct = 0usize;
        for left in 0..case.listings.len() {
            for right in left + 1..case.listings.len() {
                let same_expected = case.groups[left] == case.groups[right];
                let same_card = positions[left].is_some() && positions[left] == positions[right];
                if same_expected {
                    expected_pairs += 1;
                }
                if same_card {
                    merged_pairs += 1;
                }
                if same_expected && same_card {
                    correct_pairs += 1;
                }
                if same_expected {
                    merged += usize::from(same_card);
                    correct += usize::from(same_card);
                }
            }
        }
        let asked: usize = cards.iter().map(|card| card.possible_same_as.len()).sum();
        questions += asked;
        let place = case
            .top
            .and_then(|index| positions[index])
            .map(|position| position + 1);
        if let Some(place) = place {
            ranked += 1;
            first += usize::from(place == 1);
            reciprocal += 1.0 / place as f64;
        }
        println!(
            "  {:<32} карточек: {}, склеено: {}, спрошено: {}{}",
            case.name,
            cards.len(),
            merged,
            asked,
            place.map_or(String::new(), |place| format!(", место: {place}"))
        );
        assert_eq!(
            correct, merged,
            "{}: склеены записи разных модов",
            case.name
        );
    }

    let precision = correct_pairs as f64 / merged_pairs.max(1) as f64;
    let recall = correct_pairs as f64 / expected_pairs.max(1) as f64;
    let top1 = first as f64 / ranked.max(1) as f64;
    let mrr = reciprocal / ranked.max(1) as f64;
    let variant_hits = VARIANTS
        .iter()
        .filter(|(raw, expected)| {
            query::variants(raw)
                .iter()
                .any(|item| item.to_lowercase().contains(&expected.to_lowercase()))
        })
        .count();
    let variants = variant_hits as f64 / VARIANTS.len() as f64;

    println!(
        "Итог: слияние {:.2}/{:.2}, первое место {:.2}, MRR {:.2}, вопросов {}, подстановки {:.2}",
        precision, recall, top1, mrr, questions, variants
    );
    assert!(precision >= 0.95, "точность слияния {precision:.2}");
    assert!(recall >= 0.95, "полнота слияния {recall:.2}");
    assert!(top1 >= 0.95, "первое место {top1:.2}");
    assert!(mrr >= 0.95, "MRR {mrr:.2}");
    assert!(
        questions <= 2,
        "слишком много вопросов о дублях: {questions}"
    );
    assert!(variants >= 0.95, "подстановки запроса {variants:.2}");
}

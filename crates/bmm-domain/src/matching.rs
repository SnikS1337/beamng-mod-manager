use crate::hosts;
use crate::matches::{ManualMatch, MatchDecisions};
use crate::models::{Listing, Offer};
use crate::text::TitleNormalizer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchVerdict {
    Different,
    Maybe,
    Same,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchResult {
    pub score: f64,
    pub verdict: MatchVerdict,
    pub reason: String,
}

const SAME_THRESHOLD: f64 = 0.9;
const MAYBE_THRESHOLD: f64 = 0.7;
const MIN_NAME_SIMILARITY: f64 = 0.6;

pub struct DuplicateMatcher;

impl DuplicateMatcher {
    pub fn compare(
        a: &Listing,
        b: &Listing,
        decisions: Option<&dyn MatchDecisions>,
    ) -> MatchResult {
        if let Some(decision) = decisions.and_then(|decisions| decisions.get(a, b)) {
            return match decision {
                ManualMatch::Same => MatchResult {
                    score: 1.0,
                    verdict: MatchVerdict::Same,
                    reason: "решение пользователя: один мод".to_string(),
                },
                ManualMatch::Different => MatchResult {
                    score: 0.0,
                    verdict: MatchVerdict::Different,
                    reason: "решение пользователя: разные моды".to_string(),
                },
            };
        }

        if let Some((host, id)) = shared_host_file(a, b) {
            return MatchResult {
                score: 1.0,
                verdict: MatchVerdict::Same,
                reason: format!("один файл на {host} ({id})"),
            };
        }

        let name = TitleNormalizer::similarity(&a.title, &b.title);
        if name < MIN_NAME_SIMILARITY {
            return verdict(name * 0.5, format!("название {name:.2}, признаков 0.50"));
        }

        let mut matched = name * 0.5;
        let mut available = 0.5;
        let mut author_mismatch = false;
        let mut size_conflict = false;

        if let (Some(author_a), Some(author_b)) = (&a.author, &b.author) {
            let key_a = author_key(author_a);
            let key_b = author_key(author_b);
            if !key_a.is_empty() && !key_b.is_empty() {
                available += 0.20;
                if key_a == key_b {
                    matched += 0.20;
                } else {
                    author_mismatch = true;
                }
            }
        }

        if let (Some(size_a), Some(size_b)) = (a.size_bytes, b.size_bytes)
            && size_a > 0
            && size_b > 0
        {
            available += 0.15;
            let ratio = size_a.min(size_b) as f64 / size_a.max(size_b) as f64;
            if ratio >= 0.97 {
                matched += 0.15;
            } else if ratio >= 0.90 {
                matched += 0.075;
            } else {
                size_conflict = true;
            }
        }

        if let (Some(hash_a), Some(hash_b)) = (a.preview_hash, b.preview_hash) {
            available += 0.10;
            let difference = (hash_a ^ hash_b).count_ones();
            if difference <= 6 {
                matched += 0.10;
            } else if difference <= 12 {
                matched += 0.05;
            }
        }

        if let (Some(game_a), Some(game_b)) = (&a.game_version, &b.game_version) {
            if !game_a.trim().is_empty() && !game_b.trim().is_empty() {
                available += 0.05;
                if game_a.trim().eq_ignore_ascii_case(game_b.trim()) {
                    matched += 0.05;
                }
            }
        }

        let mut confidence = matched / available * (0.7 + 0.3 * available);
        if author_mismatch {
            confidence = confidence.min(0.69);
        }
        if size_conflict {
            confidence = confidence.min(0.75);
        }

        let mut reason = format!("название {name:.2}, признаков {available:.2}");
        if author_mismatch {
            reason.push_str(", авторы разные");
        }
        if size_conflict {
            reason.push_str(", размер сильно отличается");
        }

        verdict(confidence, reason)
    }
}

fn verdict(score: f64, reason: String) -> MatchResult {
    let verdict = if score >= SAME_THRESHOLD {
        MatchVerdict::Same
    } else if score >= MAYBE_THRESHOLD {
        MatchVerdict::Maybe
    } else {
        MatchVerdict::Different
    };
    MatchResult {
        score,
        verdict,
        reason,
    }
}

fn author_key(author: &str) -> String {
    author
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn shared_host_file(a: &Listing, b: &Listing) -> Option<(String, String)> {
    let left = offer_keys(&a.offers);
    let right = offer_keys(&b.offers);
    for (host_a, id_a) in &left {
        for (host_b, id_b) in &right {
            if host_a == host_b && id_a == id_b {
                return Some((host_a.clone(), id_a.clone()));
            }
        }
    }
    None
}

fn offer_keys(offers: &[Offer]) -> Vec<(String, String)> {
    let mut result = Vec::new();
    for offer in offers {
        if let Some(id) = &offer.host_file_id
            && !id.trim().is_empty()
        {
            result.push((offer.host.to_lowercase(), id.clone()));
            continue;
        }
        if let Some(link) = hosts::try_parse_url(&offer.url) {
            result.push((link.host, link.file_id));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::system_clock;
    use crate::matches::MatchDecisionStore;
    use url::Url;

    fn listing(source: &str, title: &str) -> Listing {
        Listing::new(
            source,
            Url::parse("https://example.org/x").unwrap(),
            "id",
            title,
        )
    }

    fn offer(host: &str, id: &str) -> Offer {
        Offer {
            kind: crate::models::OfferKind::ExternalHost,
            host: host.to_string(),
            url: Url::parse("https://modsfire.com/x").unwrap(),
            host_file_id: Some(id.to_string()),
            size_bytes: None,
        }
    }

    #[test]
    fn one_name_only_is_maybe() {
        let a = listing("modland", "Toyota Sienna 2011-2017 2.2");
        let b = listing(
            "stmods",
            "МОД Toyota Sienna 2011-2017 v2.2 (0.39.x) ДЛЯ BEAMNG.DRIVE",
        );
        let result = DuplicateMatcher::compare(&a, &b, None);
        assert_eq!(result.verdict, MatchVerdict::Maybe);
        assert!((result.score - 0.85).abs() < 1e-9, "{}", result.score);
    }

    #[test]
    fn name_author_size_match() {
        let mut a = listing("modland", "Some Car");
        let mut b = listing("stmods", "Some Car");
        a.author = Some("Some Author".to_string());
        b.author = Some("some author".to_string());
        a.size_bytes = Some(61 * 1024 * 1024);
        b.size_bytes = Some(61 * 1024 * 1024 + 100_000);
        let result = DuplicateMatcher::compare(&a, &b, None);
        assert_eq!(result.verdict, MatchVerdict::Same);
    }

    #[test]
    fn shared_host_file_wins_even_with_different_titles() {
        let mut a = listing("modland", "Gavril D-Series");
        let mut b = listing("stmods", "Ibishu Covet");
        a.offers.push(offer("modsfire", "9nDwN00BJ04Ef55"));
        b.offers.push(offer("modsfire", "9nDwN00BJ04Ef55"));
        let result = DuplicateMatcher::compare(&a, &b, None);
        assert_eq!(result.verdict, MatchVerdict::Same);
        assert_eq!(result.score, 1.0);
    }

    #[test]
    fn different_file_ids_do_not_merge() {
        let mut a = listing("modland", "Gavril D-Series");
        let mut b = listing("stmods", "Ibishu Covet");
        a.offers.push(offer("modsfire", "AAAAAAAAAAAA1111"));
        b.offers.push(offer("modsfire", "BBBBBBBBBBBB2222"));
        let result = DuplicateMatcher::compare(&a, &b, None);
        assert_eq!(result.verdict, MatchVerdict::Different);
    }

    #[test]
    fn remaster_is_not_the_same_mod() {
        let a = listing("modland", "Honda Accord 2017 Remaster v1.0");
        let b = listing("stmods", "Honda Accord 2017");
        let result = DuplicateMatcher::compare(&a, &b, None);
        assert_eq!(result.verdict, MatchVerdict::Different);
    }

    #[test]
    fn different_authors_are_different() {
        let mut a = listing("modland", "Some Car");
        let mut b = listing("stmods", "Some Car");
        a.author = Some("Alice".to_string());
        b.author = Some("Bob".to_string());
        a.size_bytes = Some(50 * 1024 * 1024);
        b.size_bytes = Some(50 * 1024 * 1024);
        let result = DuplicateMatcher::compare(&a, &b, None);
        assert_eq!(result.verdict, MatchVerdict::Different);
    }

    #[test]
    fn very_different_size_caps_confidence() {
        let mut a = listing("modland", "Some Car");
        let mut b = listing("stmods", "Some Car");
        a.author = Some("Alice".to_string());
        b.author = Some("alice".to_string());
        a.size_bytes = Some(60 * 1024 * 1024);
        b.size_bytes = Some(80 * 1024 * 1024);
        let result = DuplicateMatcher::compare(&a, &b, None);
        assert_eq!(result.verdict, MatchVerdict::Maybe);
        assert!(result.score <= 0.75, "{}", result.score);
    }

    #[test]
    fn matching_preview_raises_confidence() {
        let mut a = listing("modland", "Some Car");
        let mut b = listing("stmods", "Some Car");
        a.preview_hash = Some(0xF0F0_F0F0_F0F0_F0F0);
        b.preview_hash = Some(0xF0F0_F0F0_F0F0_F0F0);
        let result = DuplicateMatcher::compare(&a, &b, None);
        assert!(result.score > 0.85, "{}", result.score);
    }

    #[test]
    fn manual_decision_beats_signals() {
        let store = MatchDecisionStore::new(None, system_clock());
        let mut a = listing("modland", "Gavril D-Series");
        let mut b = listing("stmods", "Ibishu Covet");
        a.offers.push(offer("modsfire", "9nDwN00BJ04Ef55"));
        b.offers.push(offer("modsfire", "9nDwN00BJ04Ef55"));
        store.set(&a, &b, ManualMatch::Different).unwrap();
        let result = DuplicateMatcher::compare(&a, &b, Some(&store));
        assert_eq!(result.verdict, MatchVerdict::Different);
        assert_eq!(result.score, 0.0);
    }
}

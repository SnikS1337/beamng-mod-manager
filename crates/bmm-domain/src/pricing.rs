use url::Url;

use crate::hosts::{self, KNOWN_FILE_HOSTS, PAID_HOSTS};
use crate::models::PriceStatus;
use crate::text::TitleParser;

const PAID_TAGS: &[&str] = &[
    "paid",
    "premium",
    "vip",
    "early access",
    "early-access",
    "платный",
    "платно",
];

pub struct PaidDetector;

impl PaidDetector {
    pub fn detect(title: &str, tags: Option<&[String]>, urls: &[Url]) -> PriceStatus {
        if TitleParser::has_paid_marker(title) {
            return PriceStatus::Paid;
        }
        if let Some(tags) = tags
            && tags.iter().any(|tag| {
                PAID_TAGS
                    .iter()
                    .any(|known| tag.trim().eq_ignore_ascii_case(known))
            })
        {
            return PriceStatus::Paid;
        }
        if urls.is_empty() {
            return PriceStatus::Unknown;
        }
        if urls
            .iter()
            .any(|url| hosts::host_matches(url, KNOWN_FILE_HOSTS))
        {
            return PriceStatus::Free;
        }
        if urls.iter().all(|url| hosts::host_matches(url, PAID_HOSTS)) {
            return PriceStatus::Paid;
        }
        PriceStatus::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn paid_markers_in_title() {
        assert_eq!(
            PaidDetector::detect("Daxworx Plate Pack PAID", None, &[]),
            PriceStatus::Paid
        );
        assert_eq!(
            PaidDetector::detect("Some Car [Premium]", None, &[]),
            PriceStatus::Paid
        );
        assert_eq!(
            PaidDetector::detect("Toyota Sienna Limited Premium", None, &[]),
            PriceStatus::Unknown
        );
    }

    #[test]
    fn paid_tags() {
        let tags = vec!["Premium".to_string()];
        assert_eq!(
            PaidDetector::detect("Some Car", Some(&tags), &[]),
            PriceStatus::Paid
        );
    }

    #[test]
    fn only_patreon_is_paid() {
        let urls = vec![url("https://www.patreon.com/someauthor")];
        assert_eq!(
            PaidDetector::detect("Some Car", None, &urls),
            PriceStatus::Paid
        );
    }

    #[test]
    fn patreon_next_to_file_host_is_free() {
        let urls = vec![
            url("https://www.patreon.com/someauthor"),
            url("https://modsfire.com/9nDwN00BJ04Ef55"),
        ];
        assert_eq!(
            PaidDetector::detect("Some Car", None, &urls),
            PriceStatus::Free
        );
    }

    #[test]
    fn known_file_host_is_free() {
        let urls = vec![url("https://modsfire.com/9nDwN00BJ04Ef55")];
        assert_eq!(
            PaidDetector::detect("Some Car", None, &urls),
            PriceStatus::Free
        );
    }

    #[test]
    fn unknown_sources_stay_unknown() {
        assert_eq!(
            PaidDetector::detect("Some Car", None, &[]),
            PriceStatus::Unknown
        );
        let urls = vec![url("https://example.org/file.zip")];
        assert_eq!(
            PaidDetector::detect("Some Car", None, &urls),
            PriceStatus::Unknown
        );
    }

    #[test]
    fn paid_subdomain() {
        let urls = vec![url("https://author.patreon.com/posts/1")];
        assert_eq!(
            PaidDetector::detect("Some Car", None, &urls),
            PriceStatus::Paid
        );
    }
}

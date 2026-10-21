use std::sync::OnceLock;

use fancy_regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionOrder {
    Unknown,
    Older,
    Same,
    Newer,
}

struct Patterns {
    dotted: Regex,
    prefixed: Regex,
}

fn patterns() -> &'static Patterns {
    static PATTERNS: OnceLock<Patterns> = OnceLock::new();
    PATTERNS.get_or_init(|| Patterns {
        dotted: Regex::new(
            r"(?i)(?<![\p{L}\p{Nd}.])v?(\d+(?:\.\d+)+)(?![\p{Nd}])(?:(?:[-_. ]?(?<pre>alpha|beta|rc|preview|pre|dev|wip|test|[ab](?=\d))(?![\p{L}])\.?(?<pn>\d+)?)|(?<suf>[a-z])(?![\p{L}\p{Nd}]))?",
        )
        .expect("статическое выражение версии"),
        prefixed: Regex::new(r"(?i)(?<![\p{L}\p{Nd}])v(\d+)(?![\p{Nd}.])")
            .expect("статическое выражение версии"),
    })
}

pub struct VersionComparer;

impl VersionComparer {
    pub fn try_parse(text: Option<&str>) -> Option<Vec<i32>> {
        let text = text?;
        if text.trim().is_empty() {
            return None;
        }
        let p = patterns();
        let captures = p.dotted.captures(text).ok().flatten();
        let digits = if let Some(captures) = &captures {
            captures.get(1)?.as_str()
        } else {
            p.prefixed.captures(text).ok().flatten()?.get(1)?.as_str()
        };

        digits
            .split('.')
            .map(|part| part.parse::<i32>().ok())
            .collect()
    }

    pub fn compare(installed: Option<&str>, current: Option<&str>) -> VersionOrder {
        let (Some(a), Some(b)) = (Self::try_parse(installed), Self::try_parse(current)) else {
            return VersionOrder::Unknown;
        };
        let (rank_a, num_a, suffix_a) = pre(installed);
        let (rank_b, num_b, suffix_b) = pre(current);

        let len = a.len().max(b.len());
        for index in 0..len {
            let x = a.get(index).copied().unwrap_or(0);
            let y = b.get(index).copied().unwrap_or(0);
            if y > x {
                return VersionOrder::Newer;
            }
            if y < x {
                return VersionOrder::Older;
            }
        }

        if rank_b != rank_a {
            return if rank_b > rank_a {
                VersionOrder::Newer
            } else {
                VersionOrder::Older
            };
        }
        if num_b != num_a {
            return if num_b > num_a {
                VersionOrder::Newer
            } else {
                VersionOrder::Older
            };
        }
        if suffix_b != suffix_a {
            return if suffix_b > suffix_a {
                VersionOrder::Newer
            } else {
                VersionOrder::Older
            };
        }
        VersionOrder::Same
    }
}

fn pre_rank(tag: Option<&str>) -> i32 {
    match tag.map(str::to_lowercase).as_deref() {
        None | Some("") => 100,
        Some("rc") => 3,
        Some("beta" | "b") => 2,
        Some("alpha" | "a") => 1,
        _ => 0,
    }
}

fn pre(text: Option<&str>) -> (i32, i32, char) {
    let Some(text) = text else {
        return (100, 0, '\0');
    };
    if text.trim().is_empty() {
        return (100, 0, '\0');
    }
    let p = patterns();
    let Ok(Some(captures)) = p.dotted.captures(text) else {
        return (100, 0, '\0');
    };
    let tag = captures.name("pre").map(|m| m.as_str());
    let number = captures
        .name("pn")
        .and_then(|m| m.as_str().parse::<i32>().ok())
        .unwrap_or(0);
    let suffix = captures
        .name("suf")
        .and_then(|m| m.as_str().chars().next())
        .map(|c| c.to_lowercase().next().unwrap_or(c))
        .unwrap_or('\0');
    let rank = pre_rank(tag);
    let number = if tag.is_some() { number.max(1) } else { 0 };
    (rank, number, suffix)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateStatus {
    None,
    Likely,
    Possible,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSnapshot {
    pub version: Option<String>,
    pub size_bytes: Option<i64>,
    pub content_hash: Option<String>,
}

impl VersionSnapshot {
    pub fn new(version: Option<&str>, size_bytes: Option<i64>, content_hash: Option<&str>) -> Self {
        Self {
            version: version.map(str::to_string),
            size_bytes,
            content_hash: content_hash.map(str::to_string),
        }
    }
}

pub struct SizeComparer;

impl SizeComparer {
    pub fn differs(a: Option<i64>, b: Option<i64>) -> bool {
        let (Some(x), Some(y)) = (a, b) else {
            return false;
        };
        if x <= 0 || y <= 0 {
            return false;
        }
        const GIB: i64 = 1024 * 1024 * 1024;
        let larger = x.max(y);
        let fraction = if larger >= GIB { 0.05 } else { 0.02 };
        let minimum = if larger >= 100 * 1024 { 6 * 1024 } else { 1024 };
        let tolerance = minimum.max((larger as f64 * fraction) as i64);
        x.abs_diff(y) > tolerance as u64
    }
}

pub struct UpdateDecider;

impl UpdateDecider {
    pub fn decide(installed: &VersionSnapshot, current: &VersionSnapshot) -> UpdateStatus {
        let order =
            VersionComparer::compare(installed.version.as_deref(), current.version.as_deref());
        if order == VersionOrder::Newer {
            return UpdateStatus::Likely;
        }
        if order == VersionOrder::Older {
            return UpdateStatus::None;
        }

        let changed = SizeComparer::differs(installed.size_bytes, current.size_bytes)
            || (installed.content_hash.is_some()
                && current.content_hash.is_some()
                && installed.content_hash != current.content_hash);
        if changed {
            UpdateStatus::Possible
        } else {
            UpdateStatus::None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: &str, b: &str) -> VersionOrder {
        VersionComparer::compare(Some(a), Some(b))
    }

    #[test]
    fn compares_versions() {
        assert_eq!(cmp("1.3", "v1.3.2"), VersionOrder::Newer);
        assert_eq!(cmp("1.0", "1.0.0"), VersionOrder::Same);
        assert_eq!(cmp("1.2", "1.1.9"), VersionOrder::Older);
        assert_eq!(cmp("1.9", "1.10"), VersionOrder::Newer);
        assert_eq!(cmp("v2", "v3"), VersionOrder::Newer);
        assert_eq!(cmp("Update 2!", "3"), VersionOrder::Unknown);
        assert_eq!(
            VersionComparer::compare(None, Some("1.0")),
            VersionOrder::Unknown
        );
    }

    #[test]
    fn compares_prereleases() {
        assert_eq!(cmp("0.3.1-beta", "0.3.1"), VersionOrder::Newer);
        assert_eq!(cmp("0.3.1-beta", "0.3.1-rc1"), VersionOrder::Newer);
        assert_eq!(cmp("0.3.1-alpha", "0.3.1-beta"), VersionOrder::Newer);
        assert_eq!(cmp("0.3.1-rc1", "0.3.1-rc2"), VersionOrder::Newer);
        assert_eq!(cmp("0.3.1-beta1", "0.3.1-beta"), VersionOrder::Same);
        assert_eq!(cmp("1.0.9-beta", "1.1.0-alpha"), VersionOrder::Newer);
        assert_eq!(cmp("1.2", "1.2a"), VersionOrder::Newer);
        assert_eq!(cmp("1.2a", "1.2b"), VersionOrder::Newer);
        assert_eq!(cmp("v8", "v9"), VersionOrder::Newer);
    }

    #[test]
    fn size_differences_use_tolerance() {
        assert!(!SizeComparer::differs(None, Some(100)));
        assert!(!SizeComparer::differs(Some(0), Some(100)));
        assert!(!SizeComparer::differs(
            Some(100 * 1024 * 1024),
            Some(101 * 1024 * 1024)
        ));
        assert!(SizeComparer::differs(
            Some(100 * 1024 * 1024),
            Some(120 * 1024 * 1024)
        ));
        let gib = 1024 * 1024 * 1024;
        assert!(!SizeComparer::differs(Some(gib), Some(gib + gib / 20)));
        assert!(SizeComparer::differs(Some(gib), Some(gib + gib / 10)));
    }

    #[test]
    fn decides_updates() {
        let snapshot = |version: Option<&str>, size: Option<i64>, hash: Option<&str>| {
            VersionSnapshot::new(version, size, hash)
        };

        assert_eq!(
            UpdateDecider::decide(
                &snapshot(Some("1.0"), Some(100), None),
                &snapshot(Some("1.1"), Some(100), None)
            ),
            UpdateStatus::Likely
        );
        assert_eq!(
            UpdateDecider::decide(
                &snapshot(Some("1.0"), Some(100 * 1024 * 1024), None),
                &snapshot(Some("1.0"), Some(120 * 1024 * 1024), None)
            ),
            UpdateStatus::Possible
        );
        assert_eq!(
            UpdateDecider::decide(
                &snapshot(Some("1.0"), Some(100), Some("abc")),
                &snapshot(Some("1.0"), Some(100), Some("abc"))
            ),
            UpdateStatus::None
        );
        assert_eq!(
            UpdateDecider::decide(
                &snapshot(Some("Update 2!"), Some(100 * 1024 * 1024), None),
                &snapshot(Some("Update 3!"), Some(150 * 1024 * 1024), None)
            ),
            UpdateStatus::Possible
        );
        assert_eq!(
            UpdateDecider::decide(
                &snapshot(None, Some(100), None),
                &snapshot(None, Some(100), None)
            ),
            UpdateStatus::None
        );
        assert_eq!(
            UpdateDecider::decide(
                &snapshot(Some("2.0"), Some(100), None),
                &snapshot(Some("1.5"), Some(90), None)
            ),
            UpdateStatus::None
        );
        assert_eq!(
            UpdateDecider::decide(
                &snapshot(Some("1.0"), None, Some("aaa")),
                &snapshot(Some("1.0"), None, Some("bbb"))
            ),
            UpdateStatus::Possible
        );
    }
}

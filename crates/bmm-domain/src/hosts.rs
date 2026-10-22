use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostLink {
    pub host: String,
    pub file_id: String,
    pub url: Url,
}

pub const KNOWN_FILE_HOSTS: &[&str] = &[
    "modsfire.com",
    "mediafire.com",
    "mega.nz",
    "drive.google.com",
    "dropbox.com",
    "github.com",
    "beamng.com",
];

pub const PAID_HOSTS: &[&str] = &[
    "patreon.com",
    "boosty.to",
    "gumroad.com",
    "ko-fi.com",
    "buymeacoffee.com",
];

const MODSFIRE_RESERVED: &[&str] = &[
    "get-premium",
    "login",
    "register",
    "points",
    "guide",
    "blog",
    "contact-us",
    "privacy-policy",
    "tos",
    "download",
    "auth",
    "alt",
    "img",
    "assets",
];

pub fn try_parse(raw: Option<&str>) -> Option<HostLink> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    try_parse_url(&url)
}

pub fn try_parse_url(url: &Url) -> Option<HostLink> {
    if url.scheme() != "https" && url.scheme() != "http" {
        return None;
    }

    let mut host = url.host_str()?.to_lowercase();
    if let Some(stripped) = host.strip_prefix("www.") {
        host = stripped.to_string();
    }

    let segments: Vec<&str> = url
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect();

    match host.as_str() {
        "modsfire.com" => parse_modsfire(url, &segments),
        "mediafire.com" => parse_mediafire(url, &segments),
        _ => None,
    }
}

fn parse_modsfire(url: &Url, segments: &[&str]) -> Option<HostLink> {
    let mut id = None;

    if segments.len() == 1
        && !MODSFIRE_RESERVED
            .iter()
            .any(|reserved| reserved.eq_ignore_ascii_case(segments[0]))
    {
        id = Some(segments[0]);
    } else if segments.len() >= 2
        && (segments[0].eq_ignore_ascii_case("download")
            || segments[0].eq_ignore_ascii_case("d")
            || segments[0].eq_ignore_ascii_case("install_mod"))
    {
        id = Some(segments[1]);
    }

    let id = id?;
    if !is_file_id(id) {
        return None;
    }

    Some(HostLink {
        host: "modsfire".to_string(),
        file_id: id.to_string(),
        url: url.clone(),
    })
}

fn parse_mediafire(url: &Url, segments: &[&str]) -> Option<HostLink> {
    if segments.len() >= 2
        && (segments[0].eq_ignore_ascii_case("file") || segments[0].eq_ignore_ascii_case("view"))
        && is_file_id(segments[1])
    {
        return Some(HostLink {
            host: "mediafire".to_string(),
            file_id: segments[1].to_string(),
            url: url.clone(),
        });
    }

    None
}

fn is_file_id(id: &str) -> bool {
    (6..=32).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

pub fn host_matches(url: &Url, domains: &[&str]) -> bool {
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_lowercase();
    domains
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(url: &str) -> Option<HostLink> {
        try_parse(Some(url))
    }

    #[test]
    fn modsfire_short_link() {
        let link = parse("https://modsfire.com/9nDwN00BJ04Ef55").unwrap();
        assert_eq!(link.host, "modsfire");
        assert_eq!(link.file_id, "9nDwN00BJ04Ef55");
    }

    #[test]
    fn modsfire_download_link_has_same_id() {
        let link = parse("https://www.modsfire.com/download/9nDwN00BJ04Ef55/0fa51").unwrap();
        assert_eq!(link.file_id, "9nDwN00BJ04Ef55");
    }

    #[test]
    fn modsfire_d_and_install_mod_links_have_download_id() {
        let plain = parse("https://modsfire.com/9nDwN00BJ04Ef55").unwrap();
        let short = parse("https://modsfire.com/d/9nDwN00BJ04Ef55").unwrap();
        let install =
            parse("https://modsfire.com/install_mod/9nDwN00BJ04Ef55/?manager=ava").unwrap();
        assert_eq!(short.file_id, "9nDwN00BJ04Ef55");
        assert_eq!(install.file_id, "9nDwN00BJ04Ef55");
        assert_eq!(plain.file_id, short.file_id);
    }

    #[test]
    fn modsfire_service_pages_are_not_files() {
        for url in [
            "https://modsfire.com/login",
            "https://modsfire.com/get-premium",
            "https://modsfire.com/blog",
        ] {
            assert!(parse(url).is_none(), "{url}");
        }
    }

    #[test]
    fn mediafire_file_link() {
        let link = parse("https://www.mediafire.com/file/abcdef12/archive.zip").unwrap();
        assert_eq!(link.host, "mediafire");
        assert_eq!(link.file_id, "abcdef12");
    }

    #[test]
    fn garbage_and_other_schemes_are_rejected() {
        assert!(parse("https://example.org/file/abcdef12").is_none());
        assert!(parse("ftp://modsfire.com/abcdef12").is_none());
        assert!(try_parse(None).is_none());
        assert!(try_parse(Some("  ")).is_none());
    }

    #[test]
    fn file_id_is_case_sensitive() {
        let lower = parse("https://modsfire.com/abcdef12").unwrap();
        let upper = parse("https://modsfire.com/ABCDEF12").unwrap();
        assert_ne!(lower.file_id, upper.file_id);
    }

    #[test]
    fn subdomains_match_hosts() {
        let url = Url::parse("https://author.patreon.com/posts/1").unwrap();
        assert!(host_matches(&url, PAID_HOSTS));
        assert!(!host_matches(&url, KNOWN_FILE_HOSTS));
    }
}

//! Почему сайт не открывается — понятными словами.
//!
//! «Сайт недоступен» ничего не говорит человеку: сайт лежит, нет интернета или
//! провайдер закрыл к нему доступ? По виду сбоя соединения это не узнать наверняка,
//! но другие сайты подсказывают: если они отвечают, а этот обрывает соединение,
//! похоже на блокировку; если молчат все — похоже, нет интернета.

use std::error::Error;
use std::io::ErrorKind;
use std::time::Duration;

use url::Url;

/// Как не открылся сайт.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Reach {
    /// Адрес сайта не находится (DNS).
    NoAddress,
    /// Сайт отклоняет соединение.
    Refused,
    /// Соединение обрывается.
    Reset,
    /// Защищённое соединение не устанавливается.
    Secure,
    /// На подключение нет ответа.
    NoAnswer,
    /// Сайт подключился, страница начала грузиться и встала: так чаще всего и
    /// выглядит блокировка — сайт «бесконечно грузится» и отваливается.
    Stalled,
    /// Компьютер не подключён к сети (так пишет встроенный браузер).
    Offline,
    /// Вместо сайта — страница о блокировке или код 451.
    BlockPage,
}

impl Reach {
    pub const ALL: [Reach; 8] = [
        Reach::NoAddress,
        Reach::Refused,
        Reach::Reset,
        Reach::Secure,
        Reach::NoAnswer,
        Reach::Stalled,
        Reach::Offline,
        Reach::BlockPage,
    ];

    /// Имя для интерфейса.
    pub fn code(self) -> &'static str {
        match self {
            Reach::NoAddress => "noAddress",
            Reach::Refused => "refused",
            Reach::Reset => "reset",
            Reach::Secure => "secure",
            Reach::NoAnswer => "noAnswer",
            Reach::Stalled => "stalled",
            Reach::Offline => "offline",
            Reach::BlockPage => "blockPage",
        }
    }

    /// Что случилось — для подписи сайта.
    pub fn detail(self) -> &'static str {
        match self {
            Reach::NoAddress => "адрес сайта не находится",
            Reach::Refused => "сайт отклоняет соединение",
            Reach::Reset => "соединение обрывается",
            Reach::Secure => "не устанавливается защищённое соединение",
            Reach::NoAnswer => "сайт не отвечает на подключение",
            Reach::Stalled => "страница грузится и обрывается по времени",
            Reach::Offline => "компьютер не подключён к сети",
            Reach::BlockPage => "вместо сайта открывается страница о блокировке",
        }
    }
}

/// Что известно о других сайтах в то же время.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Others {
    /// Хотя бы один другой сайт ответил.
    Answer,
    /// Другие сайты тоже не открылись.
    Silent,
    /// Других сайтов не спрашивали.
    Unknown,
}

/// Сколько времени ответ (или молчание) другого сайта что-то говорит об этом.
pub const EVIDENCE_WINDOW: Duration = Duration::from_secs(10 * 60);

/// Подпись сайта, который не открылся.
pub fn explain(reach: Reach, others: Others) -> String {
    match (reach, others) {
        (Reach::BlockPage, _) => {
            "Вместо сайта открывается страница о блокировке — доступ закрыт в вашей сети. \
             Попробуйте VPN."
                .to_string()
        }
        (Reach::Offline, _) | (_, Others::Silent) => format!(
            "Не открывается: {}. Другие сайты тоже не отвечают — похоже, нет интернета.",
            reach.detail()
        ),
        (_, Others::Answer) => format!(
            "Не открывается из вашей сети: {}, а другие сайты отвечают — похоже на \
             блокировку. Попробуйте VPN.",
            reach.detail()
        ),
        (_, Others::Unknown) => format!(
            "Не открывается: {}. Если другие сайты открываются — возможно, он \
             заблокирован в вашей сети; попробуйте VPN.",
            reach.detail()
        ),
    }
}

/// Сайт в одном проходе (проверке, поиске): ответил ли он и как не открылся.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Visit {
    pub answered: bool,
    pub reach: Option<Reach>,
}

/// Что говорят другие сайты того же прохода о каждом неоткрывшемся. Сайты ходят
/// одновременно, и сетевой слой видит только то, что успело случиться до сбоя:
/// недоступный по адресу сайт отказывает раньше, чем ответят другие. Здесь — весь
/// проход целиком. `None` — сайт открылся (или упал не на соединении).
pub fn verdicts(visits: &[Visit]) -> Vec<Option<Others>> {
    let unreachable = visits.iter().filter(|visit| visit.reach.is_some()).count();
    visits
        .iter()
        .enumerate()
        .map(|(index, visit)| {
            visit.reach?;
            let answered = visits
                .iter()
                .enumerate()
                .any(|(other, visit)| other != index && visit.answered);
            Some(if answered {
                Others::Answer
            } else if unreachable > 1 {
                Others::Silent
            } else {
                Others::Unknown
            })
        })
        .collect()
}

/// Сбой соединения по цепочке ошибок HTTP-клиента. `None` — не сбой соединения
/// (сайт ответил, но не так, или ошибка не сетевая).
pub fn from_error(timeout: bool, connect: bool, error: &(dyn Error + 'static)) -> Option<Reach> {
    let mut text = String::new();
    let mut kind: Option<ErrorKind> = None;
    let mut current: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(error) = current {
        text.push_str(&error.to_string().to_lowercase());
        text.push(' ');
        if kind.is_none()
            && let Some(io) = error.downcast_ref::<std::io::Error>()
        {
            kind = Some(io.kind());
        }
        current = error.source();
    }
    if text.contains("dns error")
        || text.contains("failed to lookup address")
        || text.contains("no such host")
        || text.contains("name or service not known")
        || text.contains("nodename nor servname")
        || text.contains("name resolution")
        || text.contains("этот хост неизвестен")
    {
        return Some(Reach::NoAddress);
    }
    match kind {
        Some(ErrorKind::ConnectionRefused) => return Some(Reach::Refused),
        Some(
            ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::UnexpectedEof,
        ) if connect => return Some(Reach::Reset),
        Some(ErrorKind::TimedOut) if connect => return Some(Reach::NoAnswer),
        Some(ErrorKind::NetworkUnreachable | ErrorKind::HostUnreachable) => {
            return Some(Reach::NoAnswer);
        }
        _ => {}
    }
    if !connect {
        return None;
    }
    if timeout {
        return Some(Reach::NoAnswer);
    }
    if text.contains("refused") {
        return Some(Reach::Refused);
    }
    if text.contains("reset") || text.contains("closed") || text.contains("eof") {
        return Some(Reach::Reset);
    }
    if text.contains("tls")
        || text.contains("ssl")
        || text.contains("certificate")
        || text.contains("handshake")
        || text.contains("schannel")
    {
        return Some(Reach::Secure);
    }
    Some(Reach::Reset)
}

/// Страница ошибки встроенного браузера (Chromium, WebView2): код ошибки в тексте.
pub fn from_browser_page(html: &str) -> Option<Reach> {
    let upper = html.to_uppercase();
    let has = |code: &str| upper.contains(code);
    if has("ERR_INTERNET_DISCONNECTED") {
        Some(Reach::Offline)
    } else if has("ERR_NAME_NOT_RESOLVED") || has("DNS_PROBE_") || has("ERR_NAME_RESOLUTION") {
        Some(Reach::NoAddress)
    } else if has("ERR_CONNECTION_REFUSED") {
        Some(Reach::Refused)
    } else if has("ERR_CONNECTION_RESET")
        || has("ERR_CONNECTION_CLOSED")
        || has("ERR_CONNECTION_ABORTED")
        || has("ERR_EMPTY_RESPONSE")
    {
        Some(Reach::Reset)
    } else if has("ERR_SSL_") || has("ERR_CERT_") || has("ERR_BAD_SSL") {
        Some(Reach::Secure)
    } else if has("ERR_CONNECTION_TIMED_OUT")
        || has("ERR_TIMED_OUT")
        || has("ERR_ADDRESS_UNREACHABLE")
        || has("ERR_NETWORK_ACCESS_DENIED")
    {
        Some(Reach::NoAnswer)
    } else {
        None
    }
}

/// Приметы страниц о блокировке: провайдеры в России подменяют ими сайт из реестра.
const BLOCK_MARKERS: &[&str] = &[
    "eais.rkn.gov.ru",
    "blocklist.rkn.gov.ru",
    "rkn.gov.ru",
    "роскомнадзор",
    "единый реестр",
    "149-фз",
    "доступ к запрашиваемому ресурсу ограничен",
    "доступ к ресурсу ограничен",
    "доступ ограничен по решению",
];
/// Страницы о блокировке короткие; длинная страница — не она.
const BLOCK_PAGE_MAX: usize = 64 * 1024;

/// Ответ — не сайт, а страница о блокировке: код 451 или короткая страница
/// провайдера на чужом адресе с приметами реестра. Страница самого сайта не в счёт,
/// даже если в ней ссылка на реестр (новость о блокировках). Чаще провайдер ничего
/// не подсовывает, а сайт просто бесконечно грузится — это [`Reach::Stalled`].
pub fn is_block_page(requested: &Url, status: u16, final_url: &Url, text: &str) -> bool {
    if status == 451 {
        return true;
    }
    if text.len() > BLOCK_PAGE_MAX {
        return false;
    }
    let moved = match (requested.host_str(), final_url.host_str()) {
        (Some(asked), Some(got)) => base_domain(asked) != base_domain(got),
        _ => false,
    };
    if !moved {
        return false;
    }
    let lower = text.to_lowercase();
    BLOCK_MARKERS.iter().any(|marker| lower.contains(marker))
}

fn base_domain(host: &str) -> String {
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() > 2 {
        labels[labels.len() - 2..].join(".")
    } else {
        host
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct Wrapped {
        text: &'static str,
        inner: Option<Box<dyn Error + 'static>>,
    }

    impl std::fmt::Display for Wrapped {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str(self.text)
        }
    }

    impl Error for Wrapped {
        fn source(&self) -> Option<&(dyn Error + 'static)> {
            self.inner.as_deref()
        }
    }

    fn chain(text: &'static str, inner: Option<Box<dyn Error + 'static>>) -> Wrapped {
        Wrapped { text, inner }
    }

    fn io(kind: ErrorKind) -> Option<Box<dyn Error + 'static>> {
        Some(Box::new(std::io::Error::new(kind, "os error")))
    }

    #[test]
    fn connection_failures_get_their_reason() {
        let dns = chain(
            "error sending request",
            Some(Box::new(chain(
                "client error (Connect)",
                Some(Box::new(chain("dns error: failed to lookup address", None))),
            ))),
        );
        assert_eq!(from_error(false, true, &dns), Some(Reach::NoAddress));
        let refused = chain("client error (Connect)", io(ErrorKind::ConnectionRefused));
        assert_eq!(from_error(false, true, &refused), Some(Reach::Refused));
        let reset = chain("client error (Connect)", io(ErrorKind::ConnectionReset));
        assert_eq!(from_error(false, true, &reset), Some(Reach::Reset));
        let tls = chain(
            "client error (Connect)",
            Some(Box::new(chain("tls handshake failed", None))),
        );
        assert_eq!(from_error(false, true, &tls), Some(Reach::Secure));
        let slow = chain(
            "client error (Connect)",
            Some(Box::new(chain("operation timed out", None))),
        );
        assert_eq!(from_error(true, true, &slow), Some(Reach::NoAnswer));
        let unreachable = chain("client error (Connect)", io(ErrorKind::NetworkUnreachable));
        assert_eq!(from_error(false, true, &unreachable), Some(Reach::NoAnswer));
        let odd = chain("client error (Connect)", None);
        assert_eq!(
            from_error(false, true, &odd),
            Some(Reach::Reset),
            "неизвестный сбой подключения"
        );
    }

    #[test]
    fn failures_after_the_site_answered_are_not_connection_failures() {
        // Сайт ответил и оборвал чтение страницы или не успел её отдать: это сам сайт.
        let body = chain(
            "error decoding response body",
            io(ErrorKind::ConnectionReset),
        );
        assert_eq!(from_error(false, false, &body), None);
        let slow = chain("operation timed out", None);
        assert_eq!(from_error(true, false, &slow), None);
        // Но DNS и отказ — всегда соединение, как бы их ни пометил клиент.
        let dns = chain("dns error", None);
        assert_eq!(from_error(false, false, &dns), Some(Reach::NoAddress));
        let refused = chain("request", io(ErrorKind::ConnectionRefused));
        assert_eq!(from_error(false, false, &refused), Some(Reach::Refused));
    }

    #[test]
    fn real_refused_connection_is_recognised() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let error = client
            .get(format!("http://127.0.0.1:{port}/"))
            .send()
            .unwrap_err();
        assert_eq!(
            from_error(error.is_timeout(), error.is_connect(), &error),
            Some(Reach::Refused)
        );
    }

    #[test]
    fn real_secure_connection_cut_by_the_server_is_a_connection_failure() {
        // Так выглядит блокировка по имени сайта: соединение рвут на рукопожатии.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                drop(stream);
            }
        });
        let client = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let error = client
            .get(format!("https://127.0.0.1:{port}/"))
            .send()
            .unwrap_err();
        server.join().unwrap();
        let reach = from_error(error.is_timeout(), error.is_connect(), &error);
        assert!(
            matches!(reach, Some(Reach::Reset | Reach::Secure)),
            "{reach:?}: {error:?}"
        );
    }

    #[test]
    fn browser_error_pages_name_the_reason() {
        let page = |code: &str| {
            format!("<div id=\"main-frame-error\"><div class=\"error-code\">{code}</div></div>")
        };
        assert_eq!(
            from_browser_page(&page("ERR_NAME_NOT_RESOLVED")),
            Some(Reach::NoAddress)
        );
        assert_eq!(
            from_browser_page(&page("DNS_PROBE_FINISHED_NXDOMAIN")),
            Some(Reach::NoAddress)
        );
        assert_eq!(
            from_browser_page(&page("ERR_CONNECTION_RESET")),
            Some(Reach::Reset)
        );
        assert_eq!(
            from_browser_page(&page("ERR_CONNECTION_CLOSED")),
            Some(Reach::Reset)
        );
        assert_eq!(
            from_browser_page(&page("ERR_CONNECTION_REFUSED")),
            Some(Reach::Refused)
        );
        assert_eq!(
            from_browser_page(&page("ERR_CONNECTION_TIMED_OUT")),
            Some(Reach::NoAnswer)
        );
        assert_eq!(
            from_browser_page(&page("ERR_SSL_PROTOCOL_ERROR")),
            Some(Reach::Secure)
        );
        assert_eq!(
            from_browser_page(&page("ERR_INTERNET_DISCONNECTED")),
            Some(Reach::Offline)
        );
        assert_eq!(
            from_browser_page(&page("err_connection_reset")),
            Some(Reach::Reset)
        );
        assert_eq!(from_browser_page(&page("ERR_ABORTED")), None);
        assert_eq!(from_browser_page(""), None);
    }

    #[test]
    fn block_pages_are_recognised() {
        let site = Url::parse("https://modland.net/mods/a.html").unwrap();
        let isp = Url::parse("http://warning.example-isp.ru/").unwrap();
        assert!(is_block_page(&site, 451, &site, ""), "код 451");
        assert!(is_block_page(
            &site,
            200,
            &isp,
            "<h1>Доступ к запрашиваемому ресурсу ограничен</h1> по решению Роскомнадзора"
        ));
        assert!(
            !is_block_page(
                &site,
                200,
                &site,
                "<a href=\"https://eais.rkn.gov.ru/\">реестр</a>"
            ),
            "новость о блокировках на самом сайте — не заглушка"
        );
        assert!(is_block_page(
            &site,
            200,
            &isp,
            "<a href=\"https://eais.rkn.gov.ru/\">реестр</a>"
        ));
        assert!(
            !is_block_page(
                &site,
                200,
                &site,
                "<p>Роскомнадзор опять что-то запретил</p> комментарий"
            ),
            "сайт сам упомянул Роскомнадзор — это не заглушка"
        );
        assert!(
            !is_block_page(&site, 200, &isp, "<h1>Обычная страница</h1>"),
            "переадресация без примет блокировки"
        );
        let www = Url::parse("https://www.modland.net/").unwrap();
        assert!(
            !is_block_page(&site, 200, &www, "Роскомнадзор"),
            "тот же сайт"
        );
        let long = format!("{}eais.rkn.gov.ru", "x".repeat(BLOCK_PAGE_MAX));
        assert!(
            !is_block_page(&site, 200, &isp, &long),
            "длинная страница сайта"
        );
        assert!(!is_block_page(&site, 403, &site, "Forbidden"));
    }

    #[test]
    fn verdicts_look_at_the_whole_pass() {
        let open = Visit {
            answered: true,
            reach: None,
        };
        let cut = Visit {
            answered: false,
            reach: Some(Reach::Reset),
        };
        let failed = Visit {
            answered: false,
            reach: None,
        };
        assert_eq!(
            verdicts(&[cut, open, failed]),
            [Some(Others::Answer), None, None]
        );
        assert_eq!(
            verdicts(&[cut, cut, failed]),
            [Some(Others::Silent), Some(Others::Silent), None],
            "все, кто дошёл до соединения, не открылись"
        );
        assert_eq!(verdicts(&[cut, failed]), [Some(Others::Unknown), None]);
        assert_eq!(verdicts(&[cut]), [Some(Others::Unknown)]);
        assert!(verdicts(&[]).is_empty());
        // Сайт не считается «другим» сам для себя.
        let both = Visit {
            answered: true,
            reach: Some(Reach::Reset),
        };
        assert_eq!(verdicts(&[both]), [Some(Others::Unknown)]);
    }

    #[test]
    fn hanging_page_is_explained_like_other_connection_failures() {
        let blocked = explain(Reach::Stalled, Others::Answer);
        assert!(
            blocked.contains("страница грузится и обрывается"),
            "{blocked}"
        );
        assert!(blocked.contains("похоже на блокировку"), "{blocked}");
        assert!(explain(Reach::Stalled, Others::Silent).contains("нет интернета"));
        assert!(Reach::ALL.contains(&Reach::Stalled));
        assert_eq!(Reach::Stalled.code(), "stalled");
    }

    #[test]
    fn explanations_depend_on_other_sites() {
        let blocked = explain(Reach::Reset, Others::Answer);
        assert!(blocked.contains("похоже на блокировку"), "{blocked}");
        assert!(blocked.contains("соединение обрывается"), "{blocked}");
        assert!(blocked.contains("VPN"));
        let offline = explain(Reach::NoAddress, Others::Silent);
        assert!(offline.contains("нет интернета"), "{offline}");
        assert!(!offline.contains("VPN"), "без интернета VPN не поможет");
        assert!(explain(Reach::Offline, Others::Answer).contains("нет интернета"));
        let unknown = explain(Reach::Secure, Others::Unknown);
        assert!(
            unknown.contains("Если другие сайты открываются"),
            "{unknown}"
        );
        let page = explain(Reach::BlockPage, Others::Silent);
        assert!(page.contains("страница о блокировке"), "{page}");
        assert!(page.contains("VPN"));
    }
}

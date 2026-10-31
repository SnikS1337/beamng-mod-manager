//! Вежливый HTTP-слой: один запрос на сайт, паузы с джиттером, кэш страниц,
//! повторы на 429/503 и временные ошибки шлюза, защёлка Cloudflare.

use std::collections::HashMap;
use std::io::Read;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use encoding_rs::{Encoding, UTF_8, WINDOWS_1251};
use regex::Regex;
use reqwest::blocking::Client;
use reqwest::header::{
    CONTENT_LENGTH, CONTENT_TYPE, ETAG, IF_MODIFIED_SINCE, IF_NONE_MATCH, LAST_MODIFIED,
    RETRY_AFTER,
};
use url::Url;

pub const USER_AGENT: &str = concat!(
    "BeamModManager/",
    env!("CARGO_PKG_VERSION"),
    " (personal use; update checks)"
);
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
/// Предел памяти под страницы в кеше: 64 страницы могут быть и по 16 МБ каждая.
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
/// «Страницы нет» помним не дольше этого: автор мог вернуть мод или сменить адрес.
const NEGATIVE_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    Ok,
    NeedsHumanCheck,
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct RawResponse {
    pub status: u16,
    pub text: String,
    pub final_url: Url,
    pub retry_after: Option<Duration>,
    /// Валидаторы ответа для условных GET: не менялись — сайт вернёт 304.
    pub etag: Option<String>,
    pub last_modified: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("сайт не ответил вовремя")]
    Timeout,
    #[error("нет соединения с сайтом")]
    Network,
    #[error("ответ сайта слишком большой")]
    TooLarge,
    #[error("запрос отменён")]
    Cancelled,
    #[error("{0}")]
    Message(String),
    #[error("этот сайт не принимает POST")]
    Unsupported,
}

impl FetchError {
    pub fn user_message(&self) -> String {
        self.to_string()
    }
}

pub trait PageFetcher: Send + Sync {
    fn get(&self, url: &Url) -> Result<RawResponse, FetchError>;
    /// GET с отменой: скрытый браузер проверяет её во время ожидания страницы.
    fn get_cancellable(&self, url: &Url, _cancel: &AtomicBool) -> Result<RawResponse, FetchError> {
        self.get(url)
    }
    /// GET с валидаторами кэша; по умолчанию обычный GET (браузерные страницы
    /// и тестовые загрузчики не умеют 304).
    fn get_conditional(
        &self,
        url: &Url,
        _etag: Option<&str>,
        _last_modified: Option<&str>,
    ) -> Result<RawResponse, FetchError> {
        self.get(url)
    }
    fn get_conditional_cancellable(
        &self,
        url: &Url,
        etag: Option<&str>,
        last_modified: Option<&str>,
        _cancel: &AtomicBool,
    ) -> Result<RawResponse, FetchError> {
        self.get_conditional(url, etag, last_modified)
    }
    fn post_form(&self, url: &Url, form: &[(&str, &str)]) -> Result<RawResponse, FetchError> {
        let _ = (url, form);
        Err(FetchError::Unsupported)
    }
    fn post_form_cancellable(
        &self,
        url: &Url,
        form: &[(&str, &str)],
        _cancel: &AtomicBool,
    ) -> Result<RawResponse, FetchError> {
        self.post_form(url, form)
    }
    /// Картинка вместе с её типом; нужен сайтам, которые не отдают файлы обычным запросом.
    fn get_bytes(&self, url: &Url) -> Result<Option<(String, Vec<u8>)>, FetchError> {
        let _ = url;
        Ok(None)
    }
    fn get_bytes_cancellable(
        &self,
        url: &Url,
        _cancel: &AtomicBool,
    ) -> Result<Option<(String, Vec<u8>)>, FetchError> {
        self.get_bytes(url)
    }
}

pub struct HttpPageFetcher {
    client: Client,
    max_body: usize,
}

pub struct OfflineFetcher;

impl PageFetcher for OfflineFetcher {
    fn get(&self, _url: &Url) -> Result<RawResponse, FetchError> {
        Err(FetchError::Message(
            "Не удалось создать сетевой клиент.".to_string(),
        ))
    }
}

/// Редирект с сайта не должен уводить в локальную сеть или на служебный адрес.
/// Это не полная защита от DNS rebinding, но закрывает прямой переход на IP/localhost.
pub fn redirect_allowed(url: &Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host() else {
        return false;
    };
    match host {
        url::Host::Domain(name) => {
            // Хвостовая точка — тот же localhost для резолвера: `localhost.`.
            let name = name.trim_end_matches('.').to_ascii_lowercase();
            name != "localhost" && !name.ends_with(".localhost")
        }
        url::Host::Ipv4(ip) => public_v4(ip),
        url::Host::Ipv6(ip) => public_v6(ip),
    }
}

/// Редирект HTTP-клиента: дополнительно запрещено понижение схемы до http.
/// По http страницу карточки можно подменить вместе со ссылкой на архив.
pub fn secure_redirect_allowed(url: &Url) -> bool {
    url.scheme() == "https" && redirect_allowed(url)
}

/// Старые http-адреса из базы поднимаем до https до запроса: ходить по ним
/// незашифрованными нельзя, а ломать старые ссылки незачем.
pub fn secure_url(url: &Url) -> Url {
    if url.scheme() != "http" {
        return url.clone();
    }
    let mut secure = url.clone();
    if secure.set_scheme("https").is_err() {
        return url.clone();
    }
    secure
}

/// В тестах подняты локальные HTTP-серверы: их адреса не поднимаем.
#[cfg(not(test))]
fn secure_request_url(url: &Url) -> Url {
    secure_url(url)
}

#[cfg(test)]
fn secure_request_url(url: &Url) -> Url {
    url.clone()
}

fn public_v4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    // Диапазоны, которые не должны получать запросы от имени сайта:
    // this-network, private, CGNAT, loopback, link-local, IETF,
    // documentation, benchmarking, multicast и reserved.
    !(a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 0 && c == 2)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224)
}

fn public_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped().or_else(|| ip.to_ipv4()) {
        return public_v4(v4);
    }
    let segments = ip.segments();
    let first = segments[0];
    // unspecified/loopback, unique-local (fc00::/7), link-local (fe80::/10),
    // documentation (2001:db8::/32), 6to4/NAT64 и multicast.
    !(ip.is_unspecified()
        || ip.is_loopback()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
        || (first == 0x2001 && segments[1] == 0x0db8)
        || (first == 0x2002)
        || (first == 0x0064 && segments[1] == 0xff9b)
        || (first & 0xff00) == 0xff00)
}

/// Политика редиректов для обычных страниц и картинок: не больше 10 переходов
/// и только на обычные внешние https-адреса.
pub fn safe_redirect_policy() -> reqwest::redirect::Policy {
    reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 10 || !secure_redirect_allowed(attempt.url()) {
            attempt.stop()
        } else {
            attempt.follow()
        }
    })
}

pub fn default_fetcher() -> Arc<dyn PageFetcher> {
    match HttpPageFetcher::new() {
        Ok(fetcher) => Arc::new(fetcher),
        Err(_) => Arc::new(OfflineFetcher),
    }
}

/// Сайты, которые обычный запрос не пускает (Cloudflare), грузятся встроенным браузером.
/// Остальные адреса идут обычным запросом.
pub struct RoutingPageFetcher {
    plain: Arc<dyn PageFetcher>,
    browser: Arc<dyn PageFetcher>,
    browser_hosts: Vec<String>,
}

impl RoutingPageFetcher {
    pub fn new(
        plain: Arc<dyn PageFetcher>,
        browser: Arc<dyn PageFetcher>,
        browser_hosts: &[&str],
    ) -> Self {
        Self {
            plain,
            browser,
            browser_hosts: browser_hosts
                .iter()
                .map(|host| host.to_ascii_lowercase())
                .collect(),
        }
    }

    fn uses_browser(&self, url: &Url) -> bool {
        let Some(host) = url.host_str().map(str::to_ascii_lowercase) else {
            return false;
        };
        self.browser_hosts.iter().any(|known| {
            host == *known
                || host
                    .strip_suffix(known.as_str())
                    .is_some_and(|prefix| prefix.ends_with('.'))
        })
    }
}

impl PageFetcher for RoutingPageFetcher {
    fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
        if self.uses_browser(url) {
            self.browser.get(url)
        } else {
            self.plain.get(url)
        }
    }

    fn get_cancellable(&self, url: &Url, cancel: &AtomicBool) -> Result<RawResponse, FetchError> {
        if self.uses_browser(url) {
            self.browser.get_cancellable(url, cancel)
        } else {
            self.plain.get_cancellable(url, cancel)
        }
    }

    fn get_conditional(
        &self,
        url: &Url,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<RawResponse, FetchError> {
        if self.uses_browser(url) {
            self.browser.get(url)
        } else {
            self.plain.get_conditional(url, etag, last_modified)
        }
    }

    fn get_conditional_cancellable(
        &self,
        url: &Url,
        etag: Option<&str>,
        last_modified: Option<&str>,
        cancel: &AtomicBool,
    ) -> Result<RawResponse, FetchError> {
        if self.uses_browser(url) {
            self.browser.get_cancellable(url, cancel)
        } else {
            self.plain
                .get_conditional_cancellable(url, etag, last_modified, cancel)
        }
    }

    fn post_form(&self, url: &Url, form: &[(&str, &str)]) -> Result<RawResponse, FetchError> {
        if self.uses_browser(url) {
            self.browser.post_form(url, form)
        } else {
            self.plain.post_form(url, form)
        }
    }

    fn post_form_cancellable(
        &self,
        url: &Url,
        form: &[(&str, &str)],
        cancel: &AtomicBool,
    ) -> Result<RawResponse, FetchError> {
        if self.uses_browser(url) {
            self.browser.post_form_cancellable(url, form, cancel)
        } else {
            self.plain.post_form_cancellable(url, form, cancel)
        }
    }

    fn get_bytes(&self, url: &Url) -> Result<Option<(String, Vec<u8>)>, FetchError> {
        if self.uses_browser(url) {
            self.browser.get_bytes(url)
        } else {
            self.plain.get_bytes(url)
        }
    }

    fn get_bytes_cancellable(
        &self,
        url: &Url,
        cancel: &AtomicBool,
    ) -> Result<Option<(String, Vec<u8>)>, FetchError> {
        if self.uses_browser(url) {
            self.browser.get_bytes_cancellable(url, cancel)
        } else {
            self.plain.get_bytes_cancellable(url, cancel)
        }
    }
}

impl HttpPageFetcher {
    pub fn new() -> Result<Self, FetchError> {
        Self::with_user_agent(USER_AGENT)
    }

    pub fn with_user_agent(user_agent: &str) -> Result<Self, FetchError> {
        let client = Client::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(30))
            .redirect(safe_redirect_policy())
            .cookie_store(true)
            .build()
            .map_err(|error| FetchError::Message(error.to_string()))?;
        Ok(Self {
            client,
            max_body: MAX_BODY_BYTES,
        })
    }

    fn send(&self, request: reqwest::blocking::RequestBuilder) -> Result<RawResponse, FetchError> {
        let response = request.send().map_err(map_reqwest_error)?;
        let status = response.status().as_u16();
        let final_url = response.url().clone();
        let retry_after = parse_retry_after(
            response
                .headers()
                .get(RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            SystemTime::now(),
        );
        let content_type = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let last_modified = response
            .headers()
            .get(LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|length| length > self.max_body as u64)
        {
            return Err(FetchError::TooLarge);
        }
        let mut body = Vec::new();
        response
            .take(self.max_body as u64 + 1)
            .read_to_end(&mut body)
            .map_err(|_| FetchError::Network)?;
        if body.len() > self.max_body {
            return Err(FetchError::TooLarge);
        }
        Ok(RawResponse {
            status,
            text: decode_body(&body, content_type.as_deref()),
            final_url,
            retry_after,
            etag,
            last_modified,
        })
    }
}

impl PageFetcher for HttpPageFetcher {
    fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
        let url = secure_request_url(url);
        ensure_public(&url)?;
        self.send(self.client.get(url))
    }

    fn get_conditional(
        &self,
        url: &Url,
        etag: Option<&str>,
        last_modified: Option<&str>,
    ) -> Result<RawResponse, FetchError> {
        let url = secure_request_url(url);
        ensure_public(&url)?;
        let mut request = self.client.get(url);
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        if let Some(last_modified) = last_modified {
            request = request.header(IF_MODIFIED_SINCE, last_modified);
        }
        self.send(request)
    }

    fn post_form(&self, url: &Url, form: &[(&str, &str)]) -> Result<RawResponse, FetchError> {
        let url = secure_request_url(url);
        ensure_public(&url)?;
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form)
            .finish();
        self.send(
            self.client
                .post(url)
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(body),
        )
    }
}

#[cfg(not(test))]
fn ensure_public(url: &Url) -> Result<(), FetchError> {
    if redirect_allowed(url) {
        Ok(())
    } else {
        Err(FetchError::Message(
            "Адрес ведёт в локальную сеть или служебный диапазон.".to_string(),
        ))
    }
}

// Юнит-тесты поднимают локальный HTTP-сервер; в выпуске проверка всегда включена.
#[cfg(test)]
fn ensure_public(_: &Url) -> Result<(), FetchError> {
    Ok(())
}

fn map_reqwest_error(error: reqwest::Error) -> FetchError {
    if error.is_timeout() {
        FetchError::Timeout
    } else if error.is_connect() || error.is_request() {
        FetchError::Network
    } else {
        FetchError::Message(error.to_string())
    }
}

pub fn parse_retry_after(value: Option<&str>, now: SystemTime) -> Option<Duration> {
    const MAX_WAIT: Duration = Duration::from_secs(3600);
    let value = value?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        let wait = Duration::from_secs(seconds);
        return (!wait.is_zero()).then_some(wait.min(MAX_WAIT));
    }
    let when = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let when = Duration::from_secs(when.timestamp().max(0) as u64);
    let now = now.duration_since(UNIX_EPOCH).ok()?;
    let wait = when.saturating_sub(now);
    // Дату в далёком будущем не ждём десятилетиями: источник не должен
    // оставаться закрытым до перезапуска из-за одного заголовка.
    (!wait.is_zero()).then_some(wait.min(MAX_WAIT))
}

pub fn decode_body(bytes: &[u8], content_type: Option<&str>) -> String {
    let declared = content_type.and_then(charset_of);
    let encoding = if let Some((encoding, _)) = Encoding::for_bom(bytes) {
        encoding
    } else if let Some(label) = declared {
        Encoding::for_label(label.as_bytes()).unwrap_or(UTF_8)
    } else if std::str::from_utf8(bytes).is_ok() {
        // Валидный UTF-8 важнее упоминания «charset=» где-то в скрипте.
        UTF_8
    } else if let Some(label) = sniff_meta_charset(bytes) {
        Encoding::for_label(label.as_bytes()).unwrap_or(UTF_8)
    } else {
        WINDOWS_1251
    };
    let (text, _, _) = encoding.decode(bytes);
    text.into_owned()
}

fn charset_of(content_type: &str) -> Option<&str> {
    content_type.split(';').find_map(|part| {
        let part = part.trim();
        part.strip_prefix("charset=")
            .or_else(|| part.strip_prefix("Charset="))
            .map(|value| value.trim().trim_matches(['"', '\'']))
    })
}

fn sniff_meta_charset(bytes: &[u8]) -> Option<String> {
    static META: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r#"(?is)<meta[^>]*charset\s*=\s*["']?\s*([a-z0-9_-]+)"#).unwrap()
    });
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let value = META.captures(&head)?.get(1)?.as_str().to_string();
    (!value.is_empty()).then_some(value)
}

pub fn is_challenge_page(text: &str) -> bool {
    let lower = text.to_lowercase();
    let title = lower
        .split_once("<title")
        .and_then(|(_, rest)| rest.split_once('>'))
        .and_then(|(_, rest)| rest.split_once("</title>"))
        .map(|(title, _)| title)
        .unwrap_or("");
    const TITLES: &[&str] = &[
        "just a moment",
        "один момент",
        "attention required",
        "checking your browser",
        "подтвердите, что вы человек",
    ];
    if TITLES.iter().any(|marker| title.contains(marker)) {
        return true;
    }
    // Структура челленджа однозначна даже без заголовка.
    const STRUCTURE: &[&str] = &[
        "id=\"challenge-form\"",
        "id=\"challenge-running\"",
        "id=\"challenge-stage\"",
    ];
    if STRUCTURE.iter().any(|marker| lower.contains(marker)) {
        return true;
    }
    // Фразы проверки ищем только на короткой странице: в длинном описании мода
    // они ничего не доказывают.
    const PHRASES: &[&str] = &[
        "проверяем, человек ли вы",
        "verify you are human",
        "checking if the site connection is secure",
        "needs to review the security of your connection",
    ];
    lower.len() < 4096 && PHRASES.iter().any(|marker| lower.contains(marker))
}

#[derive(Debug, Clone)]
pub struct PoliteOptions {
    pub pause: Duration,
    pub jitter: Duration,
    pub retries: usize,
    pub backoff: Duration,
    pub max_backoff: Duration,
    pub cache_ttl: Duration,
    pub cache_capacity: usize,
    pub failure_limit: u32,
    pub cooldown: Duration,
}

impl Default for PoliteOptions {
    fn default() -> Self {
        Self {
            pause: Duration::from_millis(1500),
            jitter: Duration::from_millis(1000),
            retries: 2,
            backoff: Duration::from_secs(5),
            max_backoff: Duration::from_secs(60),
            cache_ttl: Duration::from_secs(30 * 60),
            cache_capacity: 64,
            failure_limit: 3,
            cooldown: Duration::from_secs(10 * 60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FetchResult {
    pub status: SourceStatus,
    pub response: Option<RawResponse>,
    pub message: Option<String>,
}

impl FetchResult {
    fn ok(response: RawResponse) -> Self {
        Self {
            status: SourceStatus::Ok,
            response: Some(response),
            message: None,
        }
    }

    fn failed(status: SourceStatus, message: impl Into<String>) -> Self {
        Self {
            status,
            response: None,
            message: Some(message.into()),
        }
    }
}

#[derive(Debug)]
struct HostInner {
    last_request: Option<Instant>,
    failures: u32,
    blocked_until: Option<Instant>,
    /// Паузу назначил сам сайт (Retry-After, 429/503): её соблюдаем всегда, даже
    /// при «Проверить принудительно».
    blocked_by_site: bool,
    /// До какого момента сайт требует проверку. После срока пробуем снова.
    human_until: Option<Instant>,
    status: SourceStatus,
    message: Option<String>,
}

impl Default for HostInner {
    fn default() -> Self {
        Self {
            last_request: None,
            failures: 0,
            blocked_until: None,
            blocked_by_site: false,
            human_until: None,
            status: SourceStatus::Ok,
            message: None,
        }
    }
}

struct HostEntry {
    gate: Mutex<()>,
    inner: Mutex<HostInner>,
    /// Растёт, когда человек прошёл проверку сайта ([`PoliteFetcher::reset_source`]).
    /// Запрос, начатый раньше, мог принести заглушку «вы человек?» уже после
    /// прохождения: такой ответ не должен снова закрывать сайт.
    passes: std::sync::atomic::AtomicU64,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    stored: Instant,
    /// У «страницы нет» срок короче, чем у обычной: страница может вернуться.
    ttl: Duration,
    response: RawResponse,
}

#[derive(Debug, Clone)]
struct SourceState {
    status: SourceStatus,
    message: Option<String>,
    /// До какого момента источник отключён после серии сбоев. `None` — пауза уже не действует:
    /// одиночный сбой не должен закрывать сайт до перезапуска программы.
    cooldown_until: Option<Instant>,
}

impl Default for SourceState {
    fn default() -> Self {
        Self {
            status: SourceStatus::Ok,
            message: None,
            cooldown_until: None,
        }
    }
}

/// Пауза после серии сбоев истекла — источник снова доступен. Одиночный сбой
/// (`cooldown_until = None`) тоже не закрывает сайт навсегда.
fn refresh_source(state: &mut SourceState) {
    let expired = state
        .cooldown_until
        .is_some_and(|until| until <= Instant::now());
    match state.status {
        // Одиночный сбой не закрывает сайт до перезапуска программы.
        SourceStatus::Unavailable if state.cooldown_until.is_none() || expired => {
            *state = SourceState::default();
        }
        // Проверка «вы человек» тоже не вечная: после срока пробуем снова.
        SourceStatus::NeedsHumanCheck if expired => {
            *state = SourceState::default();
        }
        _ => {}
    }
}

pub struct PoliteFetcher {
    fetcher: Arc<dyn PageFetcher>,
    options: PoliteOptions,
    hosts: Mutex<HashMap<String, Arc<HostEntry>>>,
    sources: Mutex<HashMap<String, SourceState>>,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl PoliteFetcher {
    pub fn new(fetcher: Arc<dyn PageFetcher>, options: PoliteOptions) -> Self {
        Self {
            fetcher,
            options,
            hosts: Mutex::new(HashMap::new()),
            sources: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn status_of(&self, source: &str) -> SourceStatus {
        let mut sources = lock(&self.sources);
        let Some(state) = sources.get_mut(source) else {
            return SourceStatus::Ok;
        };
        refresh_source(state);
        state.status
    }

    pub fn message_of(&self, source: &str) -> Option<String> {
        let mut sources = lock(&self.sources);
        let state = sources.get_mut(source)?;
        refresh_source(state);
        state.message.clone()
    }

    /// Общая очередь сайта для картинок: у них свой HTTP-клиент, но они не должны
    /// обгонять страницы и бомбить хост десятком параллельных запросов.
    /// `None` — сайт сейчас на паузе (сбой или проверка «вы человек»).
    pub fn image_slot<T>(&self, url: &Url, body: impl FnOnce() -> T) -> Option<T> {
        let url = secure_request_url(url);
        let entry = self.host_entry(&url);
        {
            let inner = lock(&entry.inner);
            if inner
                .blocked_until
                .is_some_and(|until| until > Instant::now())
            {
                return None;
            }
            if inner.status == SourceStatus::NeedsHumanCheck
                && inner.human_until.is_none_or(|until| until > Instant::now())
            {
                return None;
            }
        }
        let _guard = loop {
            match entry.gate.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(CANCEL_POLL),
            }
        };
        if !self.wait_for_slot(&entry, &url, &NEVER) {
            return None;
        }
        let result = body();
        lock(&entry.inner).last_request = Some(Instant::now());
        Some(result)
    }

    fn host_entry(&self, url: &Url) -> Arc<HostEntry> {
        let host = url.host_str().unwrap_or_default().to_lowercase();
        let mut hosts = lock(&self.hosts);
        hosts
            .entry(host)
            .or_insert_with(|| {
                Arc::new(HostEntry {
                    gate: Mutex::new(()),
                    inner: Mutex::new(HostInner::default()),
                    passes: std::sync::atomic::AtomicU64::new(0),
                })
            })
            .clone()
    }

    pub fn get(&self, source: &str, url: &Url) -> FetchResult {
        self.fetch(source, url, None, &NEVER)
    }

    pub fn post_form(&self, source: &str, url: &Url, form: &[(&str, &str)]) -> FetchResult {
        self.post_form_cancellable(source, url, form, &NEVER)
    }

    /// Как [`Self::get`], но отмена (кнопка «Остановить», таймаут поиска) прерывает
    /// ожидание очереди сайта, паузу между запросами и повторы. Уже отправленный запрос
    /// дожидается ответа (до 30 с), но после него ничего больше не делается.
    pub fn get_cancellable(&self, source: &str, url: &Url, cancel: &AtomicBool) -> FetchResult {
        self.fetch(source, url, None, cancel)
    }

    pub fn post_form_cancellable(
        &self,
        source: &str,
        url: &Url,
        form: &[(&str, &str)],
        cancel: &AtomicBool,
    ) -> FetchResult {
        let form = form
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();
        self.fetch(source, url, Some(form), cancel)
    }

    fn fetch(
        &self,
        source: &str,
        url: &Url,
        form: Option<Vec<(String, String)>>,
        cancel: &AtomicBool,
    ) -> FetchResult {
        let secure = secure_request_url(url);
        let url = &secure;
        let key = cache_key(url, form.as_deref());
        if cancel.load(Ordering::Relaxed) {
            return cancelled();
        }
        if let Some(response) = self.cached(&key) {
            return FetchResult::ok(response);
        }
        let entry = self.host_entry(url);
        {
            let mut inner = lock(&entry.inner);
            if let Some(until) = inner.blocked_until
                && until > Instant::now()
            {
                let wait = until.duration_since(Instant::now());
                let message = inner
                    .message
                    .clone()
                    .unwrap_or_else(|| format!("Сайт отдыхает ещё {}.", wait_text(wait)));
                self.remember(
                    source,
                    SourceStatus::Unavailable,
                    Some(message.clone()),
                    Some(until),
                );
                return FetchResult::failed(SourceStatus::Unavailable, message);
            }
            if inner.status == SourceStatus::NeedsHumanCheck {
                if inner
                    .human_until
                    .is_none_or(|until| until <= Instant::now())
                {
                    *inner = HostInner::default();
                } else {
                    let message = inner
                        .message
                        .clone()
                        .unwrap_or_else(|| "Сайт просит подтвердить, что вы человек.".to_string());
                    self.remember(
                        source,
                        SourceStatus::NeedsHumanCheck,
                        Some(message.clone()),
                        inner.human_until,
                    );
                    return FetchResult::failed(SourceStatus::NeedsHumanCheck, message);
                }
            }
        }
        // Очередь сайта ждём так, чтобы отмена её прерывала: иначе остановленный поиск
        // стоял бы за чужим долгим запросом и потом всё равно ходил на сайт.
        let _guard = loop {
            if cancel.load(Ordering::Relaxed) {
                return cancelled();
            }
            match entry.gate.try_lock() {
                Ok(guard) => break guard,
                Err(std::sync::TryLockError::Poisoned(poisoned)) => break poisoned.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => std::thread::sleep(CANCEL_POLL),
            }
        };
        if let Some(response) = self.cached(&key) {
            return FetchResult::ok(response);
        }
        let passes_at_start = entry.passes.load(Ordering::SeqCst);
        let mut retried_after_pass = false;
        let conditional = form
            .is_none()
            .then(|| self.cache_validators(&key))
            .flatten();
        if !self.wait_for_slot(&entry, url, cancel) {
            return cancelled();
        }

        let mut last_message = "Сайт не ответил.".to_string();
        let mut next_attempt = 0;
        while next_attempt <= self.options.retries {
            let attempt = next_attempt;
            next_attempt += 1;
            if cancel.load(Ordering::Relaxed) {
                return cancelled();
            }
            let result = match &form {
                Some(form) => {
                    let pairs: Vec<(&str, &str)> = form
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.as_str()))
                        .collect();
                    self.fetcher.post_form_cancellable(url, &pairs, cancel)
                }
                None => match &conditional {
                    Some((etag, last_modified)) => self.fetcher.get_conditional_cancellable(
                        url,
                        etag.as_deref(),
                        last_modified.as_deref(),
                        cancel,
                    ),
                    None => self.fetcher.get_cancellable(url, cancel),
                },
            };
            lock(&entry.inner).last_request = Some(Instant::now());
            // Отменённый прогон не должен менять общий статус сайта и кэш:
            // новый поиск может идти уже с другим состоянием.
            if cancel.load(Ordering::Relaxed) {
                return cancelled();
            }
            match result {
                Err(FetchError::Cancelled) => return cancelled(),
                Ok(response) => {
                    if cancel.load(Ordering::Relaxed) {
                        return cancelled();
                    }
                    if response.status == 304 {
                        if let Some(cached) = self.use_cached_not_modified(&key, &response) {
                            let mut inner = lock(&entry.inner);
                            inner.failures = 0;
                            inner.status = SourceStatus::Ok;
                            inner.message = None;
                            self.remember(source, SourceStatus::Ok, None, None);
                            return FetchResult::ok(cached);
                        }
                        last_message =
                            "Сайт ответил «не изменилось» без сохранённой страницы.".to_string();
                        break;
                    }
                    if is_challenge_page(&response.text)
                        && !retried_after_pass
                        && entry.passes.load(Ordering::SeqCst) != passes_at_start
                    {
                        // Заглушку принёс запрос, начатый до того, как человек прошёл
                        // проверку: пробуем ещё раз уже с пропуском.
                        retried_after_pass = true;
                        next_attempt = attempt; // повтор не считается попыткой
                        continue;
                    }
                    if is_challenge_page(&response.text) {
                        let message = "Сайт просит подтвердить, что вы человек.".to_string();
                        let until = Instant::now() + self.options.cooldown;
                        let mut inner = lock(&entry.inner);
                        inner.status = SourceStatus::NeedsHumanCheck;
                        inner.message = Some(message.clone());
                        inner.human_until = Some(until);
                        self.remember(
                            source,
                            SourceStatus::NeedsHumanCheck,
                            Some(message.clone()),
                            Some(until),
                        );
                        return FetchResult {
                            status: SourceStatus::NeedsHumanCheck,
                            response: Some(response),
                            message: Some(message),
                        };
                    }
                    if matches!(response.status, 429 | 503) {
                        last_message = format!(
                            "Сайт перегружен (код {}), попробуем позже.",
                            response.status
                        );
                        let can_retry = attempt < self.options.retries;
                        if let Some(blocked) =
                            self.block_for_retry_after(&entry, source, &response, can_retry)
                        {
                            return blocked;
                        }
                        if can_retry {
                            let wait = response
                                .retry_after
                                .unwrap_or_else(|| self.retry_backoff(attempt));
                            if !pause(wait.min(self.options.max_backoff), cancel) {
                                return cancelled();
                            }
                            continue;
                        }
                    } else if matches!(response.status, 502 | 504 | 520..=524) {
                        // Шлюз/Cloudflare моргнул: ровно один повтор.
                        // Retry-After важнее нашей паузы, а при retries = 0 спать не за что.
                        last_message =
                            format!("Сайт временно недоступен (код {}).", response.status);
                        let can_retry = attempt < self.options.retries.min(1);
                        if let Some(blocked) =
                            self.block_for_retry_after(&entry, source, &response, can_retry)
                        {
                            return blocked;
                        }
                        if can_retry {
                            let wait = response
                                .retry_after
                                .unwrap_or_else(|| self.retry_backoff(attempt));
                            if !pause(wait.min(self.options.max_backoff), cancel) {
                                return cancelled();
                            }
                            continue;
                        }
                        break;
                    } else {
                        let success = (200..300).contains(&response.status);
                        if success {
                            // Пустая страница — заглушка или «ничего не найдено»: кэшировать
                            // её на полчаса значит столько же времени отдавать пустоту.
                            if !response.text.trim().is_empty() {
                                self.store_cache(key, response.clone(), self.options.cache_ttl);
                            }
                            let mut inner = lock(&entry.inner);
                            inner.failures = 0;
                            inner.status = SourceStatus::Ok;
                            inner.message = None;
                            self.remember(source, SourceStatus::Ok, None, None);
                            return FetchResult::ok(response);
                        }
                        if matches!(response.status, 404 | 410) {
                            // Мёртвую страницу запоминаем ненадолго: иначе проверка
                            // обновлений дёргает каждый такой мод заново каждые 1,5–2,5 с.
                            self.store_cache(key, response.clone(), NEGATIVE_CACHE_TTL);
                            let mut inner = lock(&entry.inner);
                            inner.failures = 0;
                            inner.status = SourceStatus::Ok;
                            self.remember(source, SourceStatus::Ok, None, None);
                            return FetchResult::ok(response);
                        }
                        // 403, 500 и прочие коды повтором не лечатся: сразу отступаем,
                        // а не стучимся ещё дважды без паузы.
                        last_message = format!("Сайт вернул ошибку {}.", response.status);
                        break;
                    }
                }
                Err(error) => {
                    if cancel.load(Ordering::Relaxed) {
                        return cancelled();
                    }
                    last_message = error.user_message();
                    // Повторяем и таймаут, и обрыв соединения. Прочие ошибки (слишком большой ответ,
                    // неверный адрес) повтором не лечатся.
                    if attempt < self.options.retries
                        && matches!(error, FetchError::Timeout | FetchError::Network)
                    {
                        if !pause(self.retry_backoff(attempt), cancel) {
                            return cancelled();
                        }
                        continue;
                    }
                    break;
                }
            }
        }
        self.mark_failure(&entry, source, last_message)
    }

    /// Вежливая пауза между запросами к сайту. `false` — пока ждали, пришла отмена.
    fn wait_for_slot(&self, entry: &HostEntry, url: &Url, cancel: &AtomicBool) -> bool {
        let last = {
            let inner = lock(&entry.inner);
            inner.last_request
        };
        if let Some(last) = last {
            let jitter = jitter_for(url, self.options.jitter);
            let target = last + self.options.pause + jitter;
            let now = Instant::now();
            if target > now {
                return pause(target - now, cancel);
            }
        }
        true
    }

    fn retry_backoff(&self, attempt: usize) -> Duration {
        let factor = 1u32 << attempt.min(6);
        (self.options.backoff * factor).min(self.options.max_backoff)
    }

    /// Сайт назвал `Retry-After`. Если ждать дольше обычного лимита или повтор уже
    /// не разрешён, не стучимся раньше срока: закрываем источник до него.
    fn block_for_retry_after(
        &self,
        entry: &HostEntry,
        source: &str,
        response: &RawResponse,
        can_retry: bool,
    ) -> Option<FetchResult> {
        let wait = response.retry_after?;
        if wait.is_zero() || (can_retry && wait <= self.options.max_backoff) {
            return None;
        }
        let message = format!("Сайт просит подождать {}.", wait_text(wait));
        Some(self.block_until(entry, source, message, Instant::now() + wait))
    }

    fn block_until(
        &self,
        entry: &HostEntry,
        source: &str,
        message: String,
        until: Instant,
    ) -> FetchResult {
        let mut inner = lock(&entry.inner);
        inner.failures = 0;
        inner.status = SourceStatus::Unavailable;
        inner.message = Some(message.clone());
        inner.blocked_by_site = true;
        inner.blocked_until = Some(
            inner
                .blocked_until
                .map_or(until, |current| current.max(until)),
        );
        let blocked_until = inner.blocked_until;
        drop(inner);
        self.remember(
            source,
            SourceStatus::Unavailable,
            Some(message.clone()),
            blocked_until,
        );
        FetchResult::failed(SourceStatus::Unavailable, message)
    }

    fn mark_failure(&self, entry: &HostEntry, source: &str, message: String) -> FetchResult {
        let mut inner = lock(&entry.inner);
        inner.failures += 1;
        inner.status = SourceStatus::Unavailable;
        inner.message = Some(message.clone());
        if inner.failures >= self.options.failure_limit {
            let until = Instant::now() + self.options.cooldown;
            // Пауза сайта длиннее нашей — оставляем её признак.
            if inner.blocked_until.is_none_or(|current| current <= until) {
                inner.blocked_by_site = false;
            }
            inner.blocked_until = Some(
                inner
                    .blocked_until
                    .map_or(until, |current| current.max(until)),
            );
            inner.failures = 0;
        }
        let cooldown_until = inner.blocked_until;
        drop(inner);
        self.remember(
            source,
            SourceStatus::Unavailable,
            Some(message.clone()),
            cooldown_until,
        );
        FetchResult::failed(SourceStatus::Unavailable, message)
    }

    fn remember(
        &self,
        source: &str,
        status: SourceStatus,
        message: Option<String>,
        cooldown_until: Option<Instant>,
    ) {
        lock(&self.sources).insert(
            source.to_string(),
            SourceState {
                status,
                message,
                cooldown_until,
            },
        );
    }

    fn cached(&self, key: &str) -> Option<RawResponse> {
        let cache = lock(&self.cache);
        let entry = cache.get(key)?;
        if entry.stored.elapsed() > entry.ttl {
            return None;
        }
        Some(entry.response.clone())
    }

    fn cache_validators(&self, key: &str) -> Option<(Option<String>, Option<String>)> {
        let cache = lock(&self.cache);
        let entry = cache.get(key)?;
        let etag = entry.response.etag.clone();
        let last_modified = entry.response.last_modified.clone();
        (etag.is_some() || last_modified.is_some()).then_some((etag, last_modified))
    }

    /// Сайт ответил 304: отдаём сохранённую страницу и продлеваем её свежесть.
    fn use_cached_not_modified(&self, key: &str, response: &RawResponse) -> Option<RawResponse> {
        let mut cache = lock(&self.cache);
        let entry = cache.get_mut(key)?;
        entry.stored = Instant::now();
        if response.etag.is_some() {
            entry.response.etag = response.etag.clone();
        }
        if response.last_modified.is_some() {
            entry.response.last_modified = response.last_modified.clone();
        }
        Some(entry.response.clone())
    }

    fn store_cache(&self, key: String, response: RawResponse, ttl: Duration) {
        let mut cache = lock(&self.cache);
        if !make_room(
            &mut cache,
            self.options.cache_capacity,
            MAX_CACHE_BYTES,
            response.text.len(),
        ) {
            return;
        }
        cache.insert(
            key,
            CacheEntry {
                stored: Instant::now(),
                ttl,
                response,
            },
        );
    }

    pub fn clear_cache(&self) {
        lock(&self.cache).clear();
    }

    /// «Проверить принудительно»: снять нашу паузу после серии сбоев, чтобы проверка
    /// действительно сходила на сайт. Паузу, которую назначил сам сайт, и проверку
    /// «вы человек?» не трогаем.
    pub fn forgive_failures(&self) {
        let mut forgiven_hosts = 0usize;
        for entry in lock(&self.hosts).values() {
            let mut inner = lock(&entry.inner);
            inner.failures = 0;
            if inner.blocked_until.is_some() && !inner.blocked_by_site {
                inner.blocked_until = None;
                if inner.status == SourceStatus::Unavailable {
                    inner.status = SourceStatus::Ok;
                    inner.message = None;
                }
                forgiven_hosts += 1;
            }
        }
        if forgiven_hosts > 0 {
            for state in lock(&self.sources).values_mut() {
                if state.status == SourceStatus::Unavailable {
                    *state = SourceState::default();
                }
            }
        }
    }

    pub fn reset_source(&self, source: &str, url: &Url) {
        if let Some(host) = url.host_str() {
            let hosts = lock(&self.hosts);
            if let Some(entry) = hosts.get(&host.to_lowercase()) {
                let mut inner = lock(&entry.inner);
                *inner = HostInner::default();
                entry.passes.fetch_add(1, Ordering::SeqCst);
            }
        }
        lock(&self.sources).insert(source.to_string(), SourceState::default());
    }
}

/// Освобождает место под новую страницу. `false` — страницу не кешируем:
/// она одна больше допустимого объёма кеша.
fn make_room(
    cache: &mut HashMap<String, CacheEntry>,
    capacity: usize,
    max_bytes: usize,
    incoming: usize,
) -> bool {
    if capacity == 0 || incoming > max_bytes {
        return false;
    }
    let mut total: usize = cache.values().map(|entry| entry.response.text.len()).sum();
    while !cache.is_empty()
        && (cache.len() >= capacity || total.saturating_add(incoming) > max_bytes)
    {
        let Some(oldest) = cache
            .iter()
            .min_by_key(|(_, entry)| entry.stored)
            .map(|(key, _)| key.clone())
        else {
            break;
        };
        if let Some(entry) = cache.remove(&oldest) {
            total = total.saturating_sub(entry.response.text.len());
        }
    }
    true
}

/// Срок словами для сообщения: секунды до минуты, дальше — минуты.
fn wait_text(wait: Duration) -> String {
    if wait >= Duration::from_secs(60) {
        format!("{} мин.", wait.as_secs().div_ceil(60))
    } else {
        format!("{} с.", wait.as_millis().div_ceil(1000).max(1))
    }
}

fn cache_key(url: &Url, form: Option<&[(String, String)]>) -> String {
    match form {
        Some(form) => {
            let body = url::form_urlencoded::Serializer::new(String::new())
                .extend_pairs(
                    form.iter()
                        .map(|(key, value)| (key.as_str(), value.as_str())),
                )
                .finish();
            format!("POST {url} {body}")
        }
        None => format!("GET {url}"),
    }
}

fn jitter_for(url: &Url, jitter: Duration) -> Duration {
    if jitter.is_zero() {
        return Duration::ZERO;
    }
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.subsec_nanos())
        .unwrap_or(0);
    let mixed = nanos.wrapping_add(url.as_str().bytes().fold(0u32, |acc, byte| {
        acc.wrapping_mul(31).wrapping_add(byte as u32)
    }));
    Duration::from_millis((mixed as u64) % (jitter.as_millis().max(1) as u64))
}

static NEVER: AtomicBool = AtomicBool::new(false);
const CANCEL_POLL: Duration = Duration::from_millis(25);

/// Ответ на отменённый запрос. Сбоем сайта не считается: ни счётчик ошибок, ни статус
/// сайта он не трогает.
fn cancelled() -> FetchResult {
    FetchResult::failed(SourceStatus::Unavailable, "отменено".to_string())
}

/// Спит `duration`, просыпаясь, чтобы проверить отмену. `false` — отменили.
fn pause(duration: Duration, cancel: &AtomicBool) -> bool {
    let until = Instant::now() + duration;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        let now = Instant::now();
        if now >= until {
            return true;
        }
        std::thread::sleep((until - now).min(CANCEL_POLL));
    }
}

/// Замок, переживший панику в другом потоке. Внутри — счётчики и кэш: после сбоя одного
/// запроса они остаются пригодными, а «отравленный» замок ломал бы сайт до перезапуска.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    fn read_request(stream: &mut TcpStream) -> String {
        let mut data = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let read = stream.read(&mut buffer).unwrap();
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
            if let Some(position) = data.windows(4).position(|part| part == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&data[..position]).to_lowercase();
                let length = head
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length:"))
                    .and_then(|value| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if data.len() >= position + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).to_string()
    }

    fn url(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    fn fixture(path: &str) -> String {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
        std::fs::read_to_string(root.join(path)).unwrap()
    }

    #[test]
    fn redirects_into_local_network_are_rejected() {
        for raw in [
            "http://127.0.0.1/x",
            "http://10.1.2.3/x",
            "http://192.168.1.1/x",
            "http://169.254.1.1/x",
            "http://[::1]/x",
            "http://[fe80::1]/x",
            "http://localhost/x",
            "http://localhost./x",
            "http://foo.localhost/x",
            "http://foo.localhost./x",
            "http://100.64.0.1/x",
            "http://100.100.100.200/x",
            "http://192.0.0.1/x",
            "http://192.0.2.1/x",
            "http://198.18.0.1/x",
            "http://198.51.100.1/x",
            "http://203.0.113.1/x",
            "http://224.0.0.1/x",
            "http://240.0.0.1/x",
            "http://[2002::1]/x",
            "http://[64:ff9b::1]/x",
            "http://[2001:db8::1]/x",
            "http://[fc00::1]/x",
            "http://[fe80::1]/x",
            "http://[ff02::1]/x",
            "file:///C:/x",
        ] {
            assert!(!redirect_allowed(&url(raw)), "{raw} — локальная сеть");
        }
        for raw in [
            "https://modland.net/x",
            "https://stmods.org/x",
            "http://8.8.8.8/x",
            "https://[2001:4860:4860::8888]/x",
        ] {
            assert!(redirect_allowed(&url(raw)), "{raw} — обычный адрес");
        }
    }

    #[test]
    fn http_downgrade_is_not_followed_and_old_links_are_upgraded() {
        let plain = url("http://8.8.8.8/x");
        assert!(redirect_allowed(&plain), "сам публичный адрес допустим");
        assert!(
            !secure_redirect_allowed(&plain),
            "редирект HTTP-клиента на http не идём"
        );
        assert!(secure_redirect_allowed(&url("https://8.8.8.8/x")));
        assert!(!secure_redirect_allowed(&url("http://127.0.0.1/x")));

        assert_eq!(
            secure_url(&url("http://modland.net/beamng.drive-mods/x.html")).as_str(),
            "https://modland.net/beamng.drive-mods/x.html"
        );
        assert_eq!(
            secure_url(&url("https://stmods.org/beamng_drive/x/")).as_str(),
            "https://stmods.org/beamng_drive/x/"
        );
    }

    #[test]
    fn user_agent_uses_the_crate_version() {
        assert!(
            USER_AGENT.contains(env!("CARGO_PKG_VERSION")),
            "{USER_AGENT}"
        );
        assert!(!USER_AGENT.contains("/0.4 "), "{USER_AGENT}");
    }

    #[test]
    fn decodes_windows_1251_from_meta() {
        let bytes = b"<html><head><meta charset=\"windows-1251\"></head><body>\xcf\xf0\xe8\xe2\xe5\xf2</body></html>";
        let text = decode_body(bytes, Some("text/html"));
        assert!(text.contains("Привет"), "{text}");
    }

    #[test]
    fn decodes_charset_from_header() {
        let bytes = b"\xcf\xf0\xe8\xe2\xe5\xf2";
        assert_eq!(
            decode_body(bytes, Some("text/html; charset=windows-1251")),
            "Привет"
        );
    }

    #[test]
    fn valid_utf8_wins_over_a_script_mention_of_charset() {
        let page = "<html><head><script>const hint = 'charset=windows-1251';</script></head><body>Привет</body></html>";
        let text = decode_body(page.as_bytes(), Some("text/html"));
        assert!(text.contains("Привет"), "{text}");
    }

    #[test]
    fn meta_charset_without_quotes_is_read() {
        let bytes = b"<html><head><meta charset=windows-1251></head><body>\xcf\xf0\xe8\xe2\xe5\xf2</body></html>";
        let text = decode_body(bytes, Some("text/html"));
        assert!(text.contains("Привет"), "{text}");
    }

    #[test]
    fn detects_cloudflare_challenge() {
        assert!(is_challenge_page(
            "<html><head><title>Just a moment...</title></head><body>checking</body></html>"
        ));
        assert!(is_challenge_page(
            "<html><body><p>Проверяем, человек ли вы</p></body></html>"
        ));
        assert!(!is_challenge_page(
            "<html><head><title>ACNG Tire Heat</title></head><body>mod</body></html>"
        ));
    }

    #[test]
    fn routes_modland_pages_to_browser_and_the_rest_to_plain() {
        let plain = FakeFetcher::new(vec![
            Ok(response(200, "plain")),
            Ok(response(200, "plain-evil")),
        ]);
        let browser = FakeFetcher::new(vec![
            Ok(response(200, "browser")),
            Ok(response(200, "browser-uppercase")),
        ]);
        let routed = RoutingPageFetcher::new(plain.clone(), browser.clone(), &["modland.net"]);

        assert_eq!(
            routed
                .get(&url("https://www.modland.net/beamng.drive-mods"))
                .unwrap()
                .text,
            "browser"
        );
        assert_eq!(
            routed
                .get(&url("https://MODLAND.net/search/bmw"))
                .unwrap()
                .text,
            "browser-uppercase"
        );
        assert_eq!(
            routed
                .get(&url("https://stmods.org/beamng_drive/"))
                .unwrap()
                .text,
            "plain"
        );
        assert_eq!(
            routed.get(&url("https://evil-modland.net/")).unwrap().text,
            "plain-evil"
        );
        assert_eq!(browser.calls.lock().unwrap().len(), 2);
        assert_eq!(plain.calls.lock().unwrap().len(), 2);

        let plain = FakeFetcher::new(vec![Ok(response(200, "plain-post"))]);
        let browser = FakeFetcher::new(vec![Ok(response(200, "browser-post"))]);
        let routed = RoutingPageFetcher::new(plain.clone(), browser.clone(), &["modland.net"]);
        assert_eq!(
            routed
                .post_form(&url("https://stmods.org/search.php"), &[("q", "bmw")])
                .unwrap()
                .text,
            "plain-post"
        );
        assert_eq!(
            routed
                .post_form(&url("https://www.modland.net/search.php"), &[("q", "bmw")])
                .unwrap()
                .text,
            "browser-post"
        );
        assert_eq!(browser.calls.lock().unwrap().len(), 1);
        assert_eq!(plain.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn routes_modland_images_to_browser() {
        struct BrowserBytes;

        impl PageFetcher for BrowserBytes {
            fn get(&self, _url: &Url) -> Result<RawResponse, FetchError> {
                Err(FetchError::Unsupported)
            }

            fn get_bytes(&self, _url: &Url) -> Result<Option<(String, Vec<u8>)>, FetchError> {
                Ok(Some(("image/webp".to_string(), vec![1, 2, 3])))
            }
        }

        let plain = FakeFetcher::new(Vec::new());
        let routed = RoutingPageFetcher::new(plain, Arc::new(BrowserBytes), &["modland.net"]);
        let (content_type, bytes) = routed
            .get_bytes(&url("https://temp2.modland.net/i/x.webp"))
            .unwrap()
            .unwrap();
        assert_eq!(content_type, "image/webp");
        assert_eq!(bytes, vec![1, 2, 3]);
        assert!(
            routed
                .get_bytes(&url("https://stmods.org/preview.jpg"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn challenge_markers_do_not_fire_on_normal_pages() {
        assert!(is_challenge_page(
            "<html><body>Needs to review the security of your connection before proceeding.</body></html>"
        ));
        assert!(is_challenge_page(
            "<html><body><div id=\"challenge-running\"></div></body></html>"
        ));
        // Обычная карточка мода с подключённым скриптом Cloudflare — не проверка.
        assert!(!is_challenge_page(
            "<html><head><title>Мод</title><script src=\"/cdn-cgi/challenge-platform/scripts/precursor/main.js\"></script></head><body>BMW E34</body></html>"
        ));
        assert!(!is_challenge_page(
            "<html><body><a href=\"https://example.org/cf-challenge\">challenge</a></body></html>"
        ));
    }

    #[test]
    fn real_pages_are_not_challenges() {
        for path in [
            "modland/list-real.html",
            "modland/mod-paid-real.html",
            "modsgaming/list-real.html",
            "stmods/list-real.html",
            "beamng/list-real.html",
        ] {
            assert!(!is_challenge_page(&fixture(path)), "{path}");
        }
    }

    #[test]
    fn retry_after_seconds_and_date() {
        let now = UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        assert_eq!(
            parse_retry_after(Some("7"), now),
            Some(Duration::from_secs(7))
        );
        let date = chrono::DateTime::from_timestamp(1_700_000_030, 0)
            .unwrap()
            .to_rfc2822();
        assert_eq!(
            parse_retry_after(Some(&date), now),
            Some(Duration::from_secs(30))
        );
        assert_eq!(parse_retry_after(Some("0"), now), None);
        let past = chrono::DateTime::from_timestamp(1_699_999_000, 0)
            .unwrap()
            .to_rfc2822();
        assert_eq!(parse_retry_after(Some(&past), now), None);
        let far = chrono::DateTime::from_timestamp(4_102_444_800, 0)
            .unwrap()
            .to_rfc2822();
        assert_eq!(
            parse_retry_after(Some(&far), now),
            Some(Duration::from_secs(3600)),
            "далёкую дату режем часом, а не ждём до 2100 года"
        );
    }

    struct FakeFetcher {
        responses: Mutex<Vec<Result<RawResponse, FetchError>>>,
        calls: Mutex<Vec<String>>,
    }

    impl FakeFetcher {
        fn new(responses: Vec<Result<RawResponse, FetchError>>) -> Arc<Self> {
            Arc::new(Self {
                responses: Mutex::new(responses),
                calls: Mutex::new(Vec::new()),
            })
        }
    }

    impl PageFetcher for FakeFetcher {
        fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
            self.calls.lock().unwrap().push(url.to_string());
            let mut responses = self.responses.lock().unwrap();
            responses.remove(0)
        }

        fn post_form(&self, url: &Url, form: &[(&str, &str)]) -> Result<RawResponse, FetchError> {
            self.calls
                .lock()
                .unwrap()
                .push(format!("POST {url} {form:?}"));
            let mut responses = self.responses.lock().unwrap();
            responses.remove(0)
        }
    }

    fn response(status: u16, text: &str) -> RawResponse {
        RawResponse {
            status,
            text: text.to_string(),
            final_url: url("https://example.org/"),
            retry_after: None,
            etag: None,
            last_modified: None,
        }
    }

    fn options() -> PoliteOptions {
        PoliteOptions {
            pause: Duration::ZERO,
            jitter: Duration::ZERO,
            retries: 2,
            backoff: Duration::ZERO,
            max_backoff: Duration::ZERO,
            cache_ttl: Duration::from_secs(60),
            cache_capacity: 8,
            failure_limit: 3,
            cooldown: Duration::from_secs(60),
        }
    }

    #[test]
    fn image_slot_waits_for_pages_and_holds_the_next_page() {
        let fake = FakeFetcher::new(vec![
            Ok(response(200, "<html>mod</html>")),
            Ok(response(200, "<html>mod2</html>")),
        ]);
        let mut polite_options = options();
        polite_options.pause = Duration::from_millis(120);
        let polite = PoliteFetcher::new(fake.clone(), polite_options);
        let page = url("https://example.org/mod");
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);

        let started = Instant::now();
        let value = polite
            .image_slot(&url("https://example.org/preview.jpg"), || 7)
            .unwrap();
        assert_eq!(value, 7);
        assert!(
            started.elapsed() >= Duration::from_millis(100),
            "картинка ждёт страницу"
        );

        let started = Instant::now();
        let _ = polite.get("site", &url("https://example.org/mod2"));
        assert!(
            started.elapsed() >= Duration::from_millis(100),
            "следующая страница ждёт картинку"
        );
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn caches_successful_get_only() {
        let fake = FakeFetcher::new(vec![Ok(response(200, "<html>mod</html>"))]);
        let polite = PoliteFetcher::new(fake.clone(), options());
        let page = url("https://example.org/mod");
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(fake.calls.lock().unwrap().len(), 1);

        let missing = FakeFetcher::new(vec![Ok(response(404, "")), Ok(response(404, ""))]);
        let polite = PoliteFetcher::new(missing.clone(), options());
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(
            missing.calls.lock().unwrap().len(),
            1,
            "404 запоминается ненадолго, а не запрашивается каждый раз"
        );
    }

    struct ConditionalFetcher {
        calls: Mutex<Vec<(Option<String>, Option<String>)>>,
        etag: String,
        text: String,
    }

    impl PageFetcher for ConditionalFetcher {
        fn get(&self, _url: &Url) -> Result<RawResponse, FetchError> {
            self.calls.lock().unwrap().push((None, None));
            let mut response = response(200, &self.text);
            response.etag = Some(self.etag.clone());
            Ok(response)
        }

        fn get_conditional(
            &self,
            _url: &Url,
            etag: Option<&str>,
            last_modified: Option<&str>,
        ) -> Result<RawResponse, FetchError> {
            self.calls
                .lock()
                .unwrap()
                .push((etag.map(str::to_string), last_modified.map(str::to_string)));
            if etag == Some(self.etag.as_str()) {
                Ok(response(304, ""))
            } else {
                let mut response = response(200, &self.text);
                response.etag = Some(self.etag.clone());
                Ok(response)
            }
        }
    }

    #[test]
    fn expired_cache_revalidates_with_etag_and_304_returns_body() {
        let fake = Arc::new(ConditionalFetcher {
            calls: Mutex::new(Vec::new()),
            etag: "\"v1\"".to_string(),
            text: "<html>mod</html>".to_string(),
        });
        let mut opts = options();
        opts.cache_ttl = Duration::from_millis(1);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let page = url("https://example.org/mod");
        let first = polite.get("site", &page);
        std::thread::sleep(Duration::from_millis(2));
        assert_eq!(first.status, SourceStatus::Ok);
        assert_eq!(first.response.unwrap().text, "<html>mod</html>");
        let second = polite.get("site", &page);
        assert_eq!(second.status, SourceStatus::Ok);
        assert_eq!(second.response.unwrap().text, "<html>mod</html>");
        let calls = fake.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].0.as_deref(), Some("\"v1\""));
    }

    #[test]
    fn unexpected_304_without_cache_is_a_failure() {
        struct AlwaysNotModified;
        impl PageFetcher for AlwaysNotModified {
            fn get(&self, _url: &Url) -> Result<RawResponse, FetchError> {
                Ok(response(304, ""))
            }

            fn get_conditional(
                &self,
                _url: &Url,
                _etag: Option<&str>,
                _last_modified: Option<&str>,
            ) -> Result<RawResponse, FetchError> {
                Ok(response(304, ""))
            }
        }
        let polite = PoliteFetcher::new(Arc::new(AlwaysNotModified), options());
        assert_eq!(
            polite.get("site", &url("https://example.org/mod")).status,
            SourceStatus::Unavailable
        );
    }

    #[test]
    fn last_modified_revalidation_returns_cached_body() {
        struct ModifiedFetcher {
            calls: Mutex<Vec<Option<String>>>,
        }
        impl PageFetcher for ModifiedFetcher {
            fn get(&self, _url: &Url) -> Result<RawResponse, FetchError> {
                self.calls.lock().unwrap().push(None);
                let mut response = response(200, "cached");
                response.last_modified = Some("Wed, 21 Oct 2026 07:28:00 GMT".to_string());
                Ok(response)
            }

            fn get_conditional(
                &self,
                _url: &Url,
                _etag: Option<&str>,
                last_modified: Option<&str>,
            ) -> Result<RawResponse, FetchError> {
                self.calls
                    .lock()
                    .unwrap()
                    .push(last_modified.map(str::to_string));
                if last_modified == Some("Wed, 21 Oct 2026 07:28:00 GMT") {
                    Ok(response(304, ""))
                } else {
                    Ok(response(200, "cached"))
                }
            }
        }
        let fake = Arc::new(ModifiedFetcher {
            calls: Mutex::new(Vec::new()),
        });
        let mut opts = options();
        opts.cache_ttl = Duration::from_millis(1);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let page = url("https://example.org/mod");
        assert_eq!(polite.get("site", &page).response.unwrap().text, "cached");
        std::thread::sleep(Duration::from_millis(2));
        let second = polite.get("site", &page);
        assert_eq!(second.status, SourceStatus::Ok);
        assert_eq!(second.response.unwrap().text, "cached");
        let calls = fake.calls.lock().unwrap().clone();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].as_deref(), Some("Wed, 21 Oct 2026 07:28:00 GMT"));
    }

    #[test]
    fn client_and_server_errors_are_not_retried() {
        for status in [403u16, 500] {
            let fake = FakeFetcher::new(vec![
                Ok(response(status, "no")),
                Ok(response(200, "<html>ok</html>")),
                Ok(response(200, "<html>ok</html>")),
            ]);
            let polite = PoliteFetcher::new(fake.clone(), options());
            let result = polite.get("site", &url("https://example.org/mod"));
            assert_eq!(result.status, SourceStatus::Unavailable, "{status}");
            assert_eq!(fake.calls.lock().unwrap().len(), 1, "{status}");
        }
    }

    #[test]
    fn retries_overloaded_then_succeeds() {
        let fake = FakeFetcher::new(vec![
            Ok(response(503, "busy")),
            Ok(response(200, "<html>ok</html>")),
        ]);
        let polite = PoliteFetcher::new(fake.clone(), options());
        let result = polite.get("site", &url("https://example.org/mod"));
        assert_eq!(result.status, SourceStatus::Ok);
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn retries_network_errors() {
        for error in [FetchError::Network, FetchError::Timeout] {
            let fake = FakeFetcher::new(vec![Err(error), Ok(response(200, "<html>ok</html>"))]);
            let polite = PoliteFetcher::new(fake.clone(), options());
            let result = polite.get("site", &url("https://example.org/mod"));
            assert_eq!(result.status, SourceStatus::Ok);
            assert_eq!(fake.calls.lock().unwrap().len(), 2);
        }
        // Прочие ошибки (например, слишком большой ответ) не повторяются.
        let fake = FakeFetcher::new(vec![Err(FetchError::TooLarge), Ok(response(200, "ok"))]);
        let polite = PoliteFetcher::new(fake.clone(), options());
        assert_eq!(
            polite.get("site", &url("https://example.org/mod")).status,
            SourceStatus::Unavailable
        );
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn gateway_502_waits_retry_after_and_respects_zero_retries() {
        let mut retry_after = response(502, "bad gateway");
        retry_after.retry_after = Some(Duration::from_millis(40));
        let fake = FakeFetcher::new(vec![Ok(retry_after), Ok(response(200, "ok"))]);
        let mut opts = options();
        opts.retries = 2;
        opts.backoff = Duration::from_secs(5);
        opts.max_backoff = Duration::from_secs(60);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let started = Instant::now();
        let result = polite.get("site", &url("https://example.org/mod"));
        assert_eq!(result.status, SourceStatus::Ok);
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
        let elapsed = started.elapsed();
        assert!(elapsed >= Duration::from_millis(30), "{elapsed:?}");
        assert!(
            elapsed < Duration::from_secs(2),
            "Retry-After важнее backoff: {elapsed:?}"
        );

        // retries = 0: один запрос и никакого сна, даже если backoff большой.
        let mut opts = options();
        opts.retries = 0;
        opts.backoff = Duration::from_secs(5);
        let fake = FakeFetcher::new(vec![Ok(response(520, "cloudflare"))]);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let started = Instant::now();
        let result = polite.get("site", &url("https://example.org/mod"));
        assert_eq!(result.status, SourceStatus::Unavailable);
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );

        // Шлюз не ожил: ровно один повтор, а не 1 + retries запросов.
        let fake = FakeFetcher::new(vec![
            Ok(response(502, "bad gateway")),
            Ok(response(502, "bad gateway")),
        ]);
        let mut opts = options();
        opts.retries = 2;
        let polite = PoliteFetcher::new(fake.clone(), opts);
        assert_eq!(
            polite.get("site", &url("https://example.org/mod")).status,
            SourceStatus::Unavailable
        );
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn long_retry_after_blocks_the_source_instead_of_retrying_early() {
        let mut busy = response(429, "slow down");
        busy.retry_after = Some(Duration::from_millis(250));
        let fake = FakeFetcher::new(vec![Ok(busy), Ok(response(200, "should not be fetched"))]);
        let mut opts = options();
        opts.retries = 2;
        opts.backoff = Duration::from_millis(10);
        opts.max_backoff = Duration::from_millis(50);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let page = url("https://example.org/mod");

        let started = Instant::now();
        let result = polite.get("site", &page);
        assert_eq!(result.status, SourceStatus::Unavailable);
        assert!(
            result
                .message
                .as_deref()
                .is_some_and(|message| message.contains("подождать")),
            "{:?}",
            result.message
        );
        assert_eq!(
            fake.calls.lock().unwrap().len(),
            1,
            "не повторяем раньше Retry-After"
        );
        assert!(
            started.elapsed() < Duration::from_millis(150),
            "не ждём урезанный срок: {:?}",
            started.elapsed()
        );

        assert_eq!(polite.status_of("site"), SourceStatus::Unavailable);
        assert_eq!(
            polite.get("site", &url("https://example.org/other")).status,
            SourceStatus::Unavailable
        );
        assert_eq!(
            fake.calls.lock().unwrap().len(),
            1,
            "источник отдыхает до указанного срока"
        );
        // Срок назначил сайт — его соблюдает и принудительная проверка.
        polite.forgive_failures();
        assert_eq!(
            polite.get("site", &url("https://example.org/third")).status,
            SourceStatus::Unavailable
        );
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn retry_after_is_honored_even_without_retries() {
        let mut busy = response(503, "busy");
        busy.retry_after = Some(Duration::from_millis(200));
        let fake = FakeFetcher::new(vec![Ok(busy), Ok(response(200, "should not be fetched"))]);
        let mut opts = options();
        opts.retries = 0;
        opts.max_backoff = Duration::from_secs(60);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let page = url("https://example.org/mod");

        assert_eq!(polite.get("site", &page).status, SourceStatus::Unavailable);
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
        assert_eq!(
            polite.get("site", &url("https://example.org/next")).status,
            SourceStatus::Unavailable
        );
        assert_eq!(
            fake.calls.lock().unwrap().len(),
            1,
            "Retry-After соблюдается и без повторов"
        );
    }

    #[test]
    fn cache_evicts_oldest_to_stay_within_limits() {
        let now = Instant::now();
        let mut cache = HashMap::new();
        for (key, offset) in [("a", 0), ("b", 1), ("c", 2)] {
            cache.insert(
                key.to_string(),
                CacheEntry {
                    stored: now + Duration::from_millis(offset),
                    ttl: Duration::from_secs(60),
                    response: response(200, "1234"),
                },
            );
        }
        assert!(make_room(&mut cache, 10, 10, 4));
        assert_eq!(cache.len(), 1, "старые ушли, новое поместилось");
        assert!(cache.contains_key("c"));

        // Влезает только одна страница — значит, и храним одну.
        let mut cache = HashMap::new();
        cache.insert(
            "old".to_string(),
            CacheEntry {
                stored: now,
                ttl: Duration::from_secs(60),
                response: response(200, "1234"),
            },
        );
        assert!(make_room(&mut cache, 1, 1024, 4));
        assert!(cache.is_empty());
    }

    #[test]
    fn cache_does_not_keep_a_page_bigger_than_the_limit() {
        let mut cache = HashMap::new();
        cache.insert(
            "old".to_string(),
            CacheEntry {
                stored: Instant::now(),
                ttl: Duration::from_secs(60),
                response: response(200, "1234"),
            },
        );
        assert!(!make_room(&mut cache, 10, 10, 20));
        assert_eq!(cache.len(), 1, "старое не вытесняем без нужды");
        assert!(!make_room(&mut cache, 0, 10, 1), "нулевая ёмкость");
    }

    #[test]
    fn empty_success_is_not_cached() {
        let fake = FakeFetcher::new(vec![
            Ok(response(200, "")),
            Ok(response(200, "  ")),
            Ok(response(200, "<html>mod</html>")),
        ]);
        let polite = PoliteFetcher::new(fake.clone(), options());
        let page = url("https://example.org/empty");
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(
            fake.calls.lock().unwrap().len(),
            2,
            "пустой 200 — не ответ, кэшировать нечего"
        );
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(
            fake.calls.lock().unwrap().len(),
            3,
            "непустая страница кэшируется"
        );
    }

    #[test]
    fn missing_page_is_cached_briefly() {
        let fake = FakeFetcher::new(vec![Ok(response(404, "not found"))]);
        let polite = PoliteFetcher::new(fake.clone(), options());
        let page = url("https://example.org/deleted");
        assert_eq!(
            polite.get("site", &page).response.map(|r| r.status),
            Some(404)
        );
        assert_eq!(
            polite.get("site", &page).response.map(|r| r.status),
            Some(404)
        );
        assert_eq!(
            fake.calls.lock().unwrap().len(),
            1,
            "мёртвая страница не запрашивается снова сразу"
        );
    }

    #[test]
    fn challenge_latches_source() {
        let fake = FakeFetcher::new(vec![Ok(response(
            200,
            "<html><head><title>Just a moment...</title></head></html>",
        ))]);
        let polite = PoliteFetcher::new(fake.clone(), options());
        let first = polite.get("site", &url("https://example.org/mod"));
        assert_eq!(first.status, SourceStatus::NeedsHumanCheck);
        let second = polite.get("site", &url("https://example.org/other"));
        assert_eq!(second.status, SourceStatus::NeedsHumanCheck);
        assert_eq!(fake.calls.lock().unwrap().len(), 1);
    }

    /// Первый ответ — заглушка «вы человек?», но приходит только после сигнала:
    /// за это время человек успевает пройти проверку. Второй — настоящая страница.
    struct PassedMidRequest {
        calls: std::sync::atomic::AtomicUsize,
        release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
        started: std::sync::Mutex<std::sync::mpsc::Sender<()>>,
    }

    impl PageFetcher for PassedMidRequest {
        fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
            let call = self.calls.fetch_add(1, Ordering::SeqCst);
            let text = if call == 0 {
                let _ = self.started.lock().unwrap().send(());
                let _ = self.release.lock().unwrap().recv();
                "<html><head><title>Just a moment...</title></head></html>"
            } else {
                "<html>ok</html>"
            };
            Ok(response_at(url, text))
        }
    }

    fn response_at(url: &Url, text: &str) -> RawResponse {
        RawResponse {
            status: 200,
            text: text.to_string(),
            final_url: url.clone(),
            retry_after: None,
            etag: None,
            last_modified: None,
        }
    }

    #[test]
    fn challenge_from_before_the_pass_does_not_close_the_site_again() {
        // Человек прошёл проверку, пока скрытое окно ещё стояло на заглушке: её ответ
        // снова закрывал сайт на 10 минут — «прошёл, а пишет: пройдите».
        let (release, wait) = std::sync::mpsc::channel();
        let (started_tx, started) = std::sync::mpsc::channel();
        let fake = Arc::new(PassedMidRequest {
            calls: std::sync::atomic::AtomicUsize::new(0),
            release: std::sync::Mutex::new(wait),
            started: std::sync::Mutex::new(started_tx),
        });
        let mut opts = options();
        opts.retries = 0;
        let polite = Arc::new(PoliteFetcher::new(fake.clone(), opts));
        let page = url("https://example.org/mod");
        let request = {
            let polite = polite.clone();
            let page = page.clone();
            std::thread::spawn(move || polite.get("site", &page))
        };
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        polite.reset_source("site", &page);
        release.send(()).unwrap();
        let result = request.join().unwrap();
        assert_eq!(result.status, SourceStatus::Ok, "{:?}", result.message);
        assert_eq!(polite.status_of("site"), SourceStatus::Ok);
        assert_eq!(fake.calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn challenge_latch_expires() {
        let fake = FakeFetcher::new(vec![
            Ok(response(
                200,
                "<html><head><title>Just a moment...</title></head></html>",
            )),
            Ok(response(
                200,
                "<html><head><title>BMW E34</title></head><body>mod</body></html>",
            )),
        ]);
        let mut opts = options();
        opts.cooldown = Duration::from_millis(60);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let page = url("https://example.org/mod");
        assert_eq!(
            polite.get("site", &page).status,
            SourceStatus::NeedsHumanCheck
        );
        assert_eq!(polite.status_of("site"), SourceStatus::NeedsHumanCheck);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(polite.status_of("site"), SourceStatus::Ok);
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(fake.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn challenge_phrase_in_long_description_is_not_a_challenge() {
        let long = format!(
            "<html><head><title>BMW E34</title></head><body>{}</body></html>",
            "Описание мода. verify you are human. ".repeat(200)
        );
        assert!(!is_challenge_page(&long));
        assert!(is_challenge_page(
            "<html><body><p>Проверяем, человек ли вы</p></body></html>"
        ));
        assert!(is_challenge_page(
            "<html><body><div id=\"challenge-stage\"></div></body></html>"
        ));
    }

    #[test]
    fn cancelled_request_does_not_poison_source_status() {
        let fake = FakeFetcher::new(vec![Ok(response(
            200,
            "<html><head><title>BMW E34</title></head></html>",
        ))]);
        let polite = PoliteFetcher::new(fake.clone(), options());
        let page = url("https://example.org/mod");
        let cancel = AtomicBool::new(true);
        let result = polite.get_cancellable("site", &page, &cancel);
        assert_eq!(result.status, SourceStatus::Unavailable);
        assert_eq!(polite.status_of("site"), SourceStatus::Ok);
        assert!(fake.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn source_goes_unavailable_after_failures() {
        let fake = FakeFetcher::new(vec![
            Err(FetchError::Network),
            Err(FetchError::Network),
            Err(FetchError::Network),
        ]);
        let mut opts = options();
        opts.retries = 0;
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let page = url("https://example.org/mod");
        for _ in 0..3 {
            polite.clear_cache();
            assert_eq!(polite.get("site", &page).status, SourceStatus::Unavailable);
        }
        assert_eq!(polite.status_of("site"), SourceStatus::Unavailable);
        assert!(polite.message_of("site").is_some());
    }

    #[test]
    fn forced_check_forgives_our_pause_but_not_the_sites() {
        // «Проверить принудительно» не делала ни одного запроса, пока шла наша
        // десятиминутная пауза после трёх сбоев.
        let fake = FakeFetcher::new(vec![
            Err(FetchError::Network),
            Err(FetchError::Network),
            Err(FetchError::Network),
            Ok(response(200, "<html>ok</html>")),
        ]);
        let mut opts = options();
        opts.retries = 0;
        opts.cooldown = Duration::from_secs(600);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let page = url("https://example.org/mod");
        for _ in 0..3 {
            polite.clear_cache();
            polite.get("site", &page);
        }
        assert_eq!(polite.status_of("site"), SourceStatus::Unavailable);
        polite.forgive_failures();
        assert_eq!(polite.status_of("site"), SourceStatus::Ok);
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(fake.calls.lock().unwrap().len(), 4);
    }

    #[test]
    fn single_failure_does_not_latch_and_cooldown_expires() {
        // Одиночный сбой: следующий поиск снова пробует сайт.
        let fake = FakeFetcher::new(vec![
            Err(FetchError::Network),
            Ok(response(200, "<html>ok</html>")),
        ]);
        let mut opts = options();
        opts.retries = 0;
        opts.cooldown = Duration::from_millis(40);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        let page = url("https://example.org/mod");
        assert_eq!(polite.get("site", &page).status, SourceStatus::Unavailable);
        assert_eq!(
            polite.status_of("site"),
            SourceStatus::Ok,
            "один сбой не закрывает сайт"
        );
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);

        // Три сбоя подряд закрывают сайт на cooldown; после него — снова Ok и запрос проходит.
        let fake = FakeFetcher::new(vec![
            Err(FetchError::Network),
            Err(FetchError::Network),
            Err(FetchError::Network),
            Ok(response(200, "<html>ok</html>")),
        ]);
        let mut opts = options();
        opts.retries = 0;
        opts.cooldown = Duration::from_millis(250);
        let polite = PoliteFetcher::new(fake.clone(), opts);
        for _ in 0..3 {
            polite.clear_cache();
            assert_eq!(polite.get("site", &page).status, SourceStatus::Unavailable);
        }
        assert_eq!(polite.status_of("site"), SourceStatus::Unavailable);
        assert_eq!(fake.calls.lock().unwrap().len(), 3);
        std::thread::sleep(Duration::from_millis(350));
        assert_eq!(polite.status_of("site"), SourceStatus::Ok);
        assert!(polite.message_of("site").is_none(), "причина сбоя снята");
        assert_eq!(polite.get("site", &page).status, SourceStatus::Ok);
        assert_eq!(fake.calls.lock().unwrap().len(), 4);
    }

    #[test]
    fn http_client_reads_charset_and_posts_form() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("GET /mod HTTP/1.1"), "{request}");
            let body = b"\xcf\xf0\xe8\xe2\xe5\xf2";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=windows-1251\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        let client = HttpPageFetcher::new().unwrap();
        let response = client.get(&url(&format!("http://{address}/mod"))).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(response.text, "Привет");
        server.join().unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            assert!(request.starts_with("POST /search HTTP/1.1"), "{request}");
            assert!(request.contains("q=bmw&page=1"), "{request}");
            let body = b"<html>ok</html>";
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(body).unwrap();
        });
        let response = client
            .post_form(
                &url(&format!("http://{address}/search")),
                &[("q", "bmw"), ("page", "1")],
            )
            .unwrap();
        assert_eq!(response.text, "<html>ok</html>");
        server.join().unwrap();
    }

    struct PanicsOnce {
        calls: std::sync::atomic::AtomicUsize,
    }

    impl PageFetcher for PanicsOnce {
        fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
            if self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                panic!("сбой загрузчика");
            }
            Ok(RawResponse {
                status: 200,
                text: "<html>ok</html>".to_string(),
                final_url: url.clone(),
                retry_after: None,
                etag: None,
                last_modified: None,
            })
        }
    }

    #[test]
    fn site_keeps_working_after_a_panic_inside_a_request() {
        // Паника посреди запроса «отравляла» замок сайта: дальше любой запрос к нему
        // падал до перезапуска программы.
        let fetcher = PoliteFetcher::new(
            Arc::new(PanicsOnce {
                calls: std::sync::atomic::AtomicUsize::new(0),
            }),
            options(),
        );
        let url = Url::parse("https://site.test/mod/1").unwrap();
        let first =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fetcher.get("site", &url)));
        assert!(first.is_err());
        let second =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| fetcher.get("site", &url)));
        let second = second.expect("второй запрос не падает");
        assert_eq!(second.status, SourceStatus::Ok);
        assert_eq!(fetcher.status_of("site"), SourceStatus::Ok);
    }

    /// Сайт, который отвечает медленно или всё время перегружен.
    struct SlowSite {
        delay: Duration,
        status: u16,
        calls: std::sync::atomic::AtomicUsize,
    }

    impl PageFetcher for SlowSite {
        fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
            self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(self.delay);
            Ok(RawResponse {
                status: self.status,
                text: "<html>ok</html>".to_string(),
                final_url: url.clone(),
                retry_after: None,
                etag: None,
                last_modified: None,
            })
        }
    }

    fn slow(delay_ms: u64, status: u16) -> Arc<SlowSite> {
        Arc::new(SlowSite {
            delay: Duration::from_millis(delay_ms),
            status,
            calls: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    #[test]
    fn cancelled_request_does_not_wait_behind_a_slow_one() {
        let site = slow(600, 200);
        let fetcher = Arc::new(PoliteFetcher::new(site.clone(), options()));
        let busy = {
            let fetcher = fetcher.clone();
            std::thread::spawn(move || {
                fetcher.get("site", &Url::parse("https://site.test/a").unwrap())
            })
        };
        // Ждём, пока медленный запрос действительно занял очередь сайта: 50 мс сна под
        // нагрузкой не хватало, и «отменяемый» запрос сам уходил в 600-мс загрузку.
        let waited = Instant::now();
        while site.calls.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            assert!(
                waited.elapsed() < Duration::from_secs(5),
                "медленный запрос не начался"
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let stopper = {
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(50));
                cancel.store(true, Ordering::Relaxed);
            })
        };
        let started = Instant::now();
        let result =
            fetcher.get_cancellable("site", &Url::parse("https://site.test/b").unwrap(), &cancel);
        assert!(
            started.elapsed() < Duration::from_millis(400),
            "{:?}",
            started.elapsed()
        );
        assert!(result.response.is_none());
        stopper.join().unwrap();
        assert_eq!(busy.join().unwrap().status, SourceStatus::Ok);
        assert_eq!(
            site.calls.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "второй запрос не ушёл"
        );
        // Отмена — не сбой сайта.
        assert_eq!(fetcher.status_of("site"), SourceStatus::Ok);
    }

    #[test]
    fn cancel_stops_retries_of_an_overloaded_site() {
        let site = slow(0, 503);
        let mut opts = options();
        opts.retries = 2;
        opts.backoff = Duration::from_secs(5);
        opts.max_backoff = Duration::from_secs(60);
        let fetcher = PoliteFetcher::new(site.clone(), opts);
        let cancel = Arc::new(AtomicBool::new(false));
        let stopper = {
            let cancel = cancel.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(80));
                cancel.store(true, Ordering::Relaxed);
            })
        };
        let started = Instant::now();
        let result =
            fetcher.get_cancellable("site", &Url::parse("https://site.test/x").unwrap(), &cancel);
        stopper.join().unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "{:?}",
            started.elapsed()
        );
        assert!(result.response.is_none());
        assert_eq!(site.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn cancel_cuts_the_polite_pause() {
        let site = slow(0, 200);
        let mut opts = options();
        opts.pause = Duration::from_secs(5);
        let fetcher = PoliteFetcher::new(site.clone(), opts);
        assert_eq!(
            fetcher
                .get("site", &Url::parse("https://site.test/1").unwrap())
                .status,
            SourceStatus::Ok
        );
        let cancel = AtomicBool::new(true);
        let started = Instant::now();
        let result =
            fetcher.get_cancellable("site", &Url::parse("https://site.test/2").unwrap(), &cancel);
        assert!(started.elapsed() < Duration::from_millis(200));
        assert!(result.response.is_none());
        assert_eq!(site.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
        // Без отмены пауза соблюдается, а кэш отвечает сразу.
        assert_eq!(
            fetcher
                .get("site", &Url::parse("https://site.test/1").unwrap())
                .status,
            SourceStatus::Ok
        );
    }
}

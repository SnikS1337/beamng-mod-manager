//! Вежливый HTTP-слой: один запрос на сайт, паузы с джиттером, кэш страниц,
//! повторы на 429/503 и временные ошибки шлюза, защёлка Cloudflare.

use std::collections::HashMap;
use std::io::Read;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use encoding_rs::{Encoding, UTF_8, WINDOWS_1251};
use reqwest::blocking::Client;
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, RETRY_AFTER};
use url::Url;

pub const USER_AGENT: &str = "BeamModManager/0.4 (personal use; update checks)";
const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

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
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("сайт не ответил вовремя")]
    Timeout,
    #[error("нет соединения с сайтом")]
    Network,
    #[error("ответ сайта слишком большой")]
    TooLarge,
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
    fn post_form(&self, url: &Url, form: &[(&str, &str)]) -> Result<RawResponse, FetchError> {
        let _ = (url, form);
        Err(FetchError::Unsupported)
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

pub fn default_fetcher() -> Arc<dyn PageFetcher> {
    match HttpPageFetcher::new() {
        Ok(fetcher) => Arc::new(fetcher),
        Err(_) => Arc::new(OfflineFetcher),
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
            .redirect(reqwest::redirect::Policy::limited(10))
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
        })
    }
}

impl PageFetcher for HttpPageFetcher {
    fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
        self.send(self.client.get(url.clone()))
    }

    fn post_form(&self, url: &Url, form: &[(&str, &str)]) -> Result<RawResponse, FetchError> {
        let body = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(form)
            .finish();
        self.send(
            self.client
                .post(url.clone())
                .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(body),
        )
    }
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
    let value = value?.trim();
    if let Ok(seconds) = value.parse::<u64>() {
        return Some(Duration::from_secs(seconds.min(3600)));
    }
    let when = chrono::DateTime::parse_from_rfc2822(value).ok()?;
    let when = Duration::from_secs(when.timestamp().max(0) as u64);
    let now = now.duration_since(UNIX_EPOCH).ok()?;
    Some(when.saturating_sub(now))
}

pub fn decode_body(bytes: &[u8], content_type: Option<&str>) -> String {
    let declared = content_type.and_then(charset_of);
    let encoding = if let Some(label) = declared {
        Encoding::for_label(label.as_bytes()).unwrap_or(UTF_8)
    } else if let Some((encoding, _)) = Encoding::for_bom(bytes) {
        encoding
    } else if let Some(label) = sniff_meta_charset(bytes) {
        Encoding::for_label(label.as_bytes()).unwrap_or(UTF_8)
    } else if std::str::from_utf8(bytes).is_ok() {
        UTF_8
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
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_lowercase();
    let start = head.find("charset=")? + "charset=".len();
    let rest = &head[start..];
    let value: String = rest
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .collect();
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
    const BODY: &[&str] = &[
        "проверяем, человек ли вы",
        "verify you are human",
        "checking if the site connection is secure",
        "id=\"challenge-",
        "cf-challenge",
    ];
    BODY.iter().any(|marker| lower.contains(marker))
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
    status: SourceStatus,
    message: Option<String>,
}

impl Default for HostInner {
    fn default() -> Self {
        Self {
            last_request: None,
            failures: 0,
            blocked_until: None,
            status: SourceStatus::Ok,
            message: None,
        }
    }
}

struct HostEntry {
    gate: Mutex<()>,
    inner: Mutex<HostInner>,
}

struct CacheEntry {
    stored: Instant,
    response: RawResponse,
}

pub struct PoliteFetcher {
    fetcher: Arc<dyn PageFetcher>,
    options: PoliteOptions,
    hosts: Mutex<HashMap<String, Arc<HostEntry>>>,
    sources: Mutex<HashMap<String, SourceStatus>>,
    source_messages: Mutex<HashMap<String, String>>,
    cache: Mutex<HashMap<String, CacheEntry>>,
}

impl PoliteFetcher {
    pub fn new(fetcher: Arc<dyn PageFetcher>, options: PoliteOptions) -> Self {
        Self {
            fetcher,
            options,
            hosts: Mutex::new(HashMap::new()),
            sources: Mutex::new(HashMap::new()),
            source_messages: Mutex::new(HashMap::new()),
            cache: Mutex::new(HashMap::new()),
        }
    }

    pub fn status_of(&self, source: &str) -> SourceStatus {
        self.sources
            .lock()
            .unwrap()
            .get(source)
            .copied()
            .unwrap_or(SourceStatus::Ok)
    }

    pub fn message_of(&self, source: &str) -> Option<String> {
        self.source_messages.lock().unwrap().get(source).cloned()
    }

    pub fn get(&self, source: &str, url: &Url) -> FetchResult {
        self.fetch(source, url, None)
    }

    pub fn post_form(&self, source: &str, url: &Url, form: &[(&str, &str)]) -> FetchResult {
        let form = form
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect();
        self.fetch(source, url, Some(form))
    }

    fn fetch(&self, source: &str, url: &Url, form: Option<Vec<(String, String)>>) -> FetchResult {
        let key = cache_key(url, form.as_deref());
        if let Some(response) = self.cached(&key) {
            return FetchResult::ok(response);
        }
        let host = url.host_str().unwrap_or_default().to_lowercase();
        let entry = {
            let mut hosts = self.hosts.lock().unwrap();
            hosts
                .entry(host)
                .or_insert_with(|| {
                    Arc::new(HostEntry {
                        gate: Mutex::new(()),
                        inner: Mutex::new(HostInner::default()),
                    })
                })
                .clone()
        };
        {
            let inner = entry.inner.lock().unwrap();
            if let Some(until) = inner.blocked_until
                && until > Instant::now()
            {
                let minutes = until.duration_since(Instant::now()).as_secs().div_ceil(60);
                let message = inner
                    .message
                    .clone()
                    .unwrap_or_else(|| format!("Сайт отдыхает ещё {minutes} мин."));
                self.remember(source, SourceStatus::Unavailable, Some(message.clone()));
                return FetchResult::failed(SourceStatus::Unavailable, message);
            }
            if inner.status == SourceStatus::NeedsHumanCheck {
                let message = inner
                    .message
                    .clone()
                    .unwrap_or_else(|| "Сайт просит подтвердить, что вы человек.".to_string());
                self.remember(source, SourceStatus::NeedsHumanCheck, Some(message.clone()));
                return FetchResult::failed(SourceStatus::NeedsHumanCheck, message);
            }
        }
        let _guard = entry.gate.lock().unwrap();
        if let Some(response) = self.cached(&key) {
            return FetchResult::ok(response);
        }
        self.wait_for_slot(&entry, url);

        let mut last_message = "Сайт не ответил.".to_string();
        for attempt in 0..=self.options.retries {
            let result = match &form {
                Some(form) => {
                    let pairs: Vec<(&str, &str)> = form
                        .iter()
                        .map(|(key, value)| (key.as_str(), value.as_str()))
                        .collect();
                    self.fetcher.post_form(url, &pairs)
                }
                None => self.fetcher.get(url),
            };
            entry.inner.lock().unwrap().last_request = Some(Instant::now());
            match result {
                Ok(response) => {
                    if is_challenge_page(&response.text) {
                        let message = "Сайт просит подтвердить, что вы человек.".to_string();
                        let mut inner = entry.inner.lock().unwrap();
                        inner.status = SourceStatus::NeedsHumanCheck;
                        inner.message = Some(message.clone());
                        self.remember(source, SourceStatus::NeedsHumanCheck, Some(message.clone()));
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
                        if attempt < self.options.retries {
                            let wait = response
                                .retry_after
                                .unwrap_or_else(|| self.retry_backoff(attempt));
                            std::thread::sleep(wait.min(self.options.max_backoff));
                            continue;
                        }
                    } else if matches!(response.status, 502 | 504 | 520..=524) && attempt == 0 {
                        last_message =
                            format!("Сайт временно недоступен (код {}).", response.status);
                        std::thread::sleep(self.options.backoff);
                        continue;
                    } else {
                        let success = (200..300).contains(&response.status);
                        if success {
                            if !response.text.is_empty() {
                                self.store_cache(key, response.clone());
                            }
                            let mut inner = entry.inner.lock().unwrap();
                            inner.failures = 0;
                            inner.status = SourceStatus::Ok;
                            inner.message = None;
                            self.remember(source, SourceStatus::Ok, None);
                            return FetchResult::ok(response);
                        }
                        if matches!(response.status, 404 | 410) {
                            let mut inner = entry.inner.lock().unwrap();
                            inner.failures = 0;
                            inner.status = SourceStatus::Ok;
                            self.remember(source, SourceStatus::Ok, None);
                            return FetchResult::ok(response);
                        }
                        last_message = format!("Сайт вернул ошибку {}.", response.status);
                    }
                }
                Err(error) => {
                    last_message = error.user_message();
                    if attempt < self.options.retries && matches!(error, FetchError::Timeout) {
                        std::thread::sleep(self.retry_backoff(attempt));
                        continue;
                    }
                    break;
                }
            }
        }
        self.mark_failure(&entry, source, last_message)
    }

    fn wait_for_slot(&self, entry: &HostEntry, url: &Url) {
        let last = {
            let inner = entry.inner.lock().unwrap();
            inner.last_request
        };
        if let Some(last) = last {
            let jitter = jitter_for(url, self.options.jitter);
            let target = last + self.options.pause + jitter;
            let now = Instant::now();
            if target > now {
                std::thread::sleep(target - now);
            }
        }
    }

    fn retry_backoff(&self, attempt: usize) -> Duration {
        let factor = 1u32 << attempt.min(6);
        (self.options.backoff * factor).min(self.options.max_backoff)
    }

    fn mark_failure(&self, entry: &HostEntry, source: &str, message: String) -> FetchResult {
        let mut inner = entry.inner.lock().unwrap();
        inner.failures += 1;
        inner.status = SourceStatus::Unavailable;
        inner.message = Some(message.clone());
        if inner.failures >= self.options.failure_limit {
            inner.blocked_until = Some(Instant::now() + self.options.cooldown);
            inner.failures = 0;
        }
        drop(inner);
        self.remember(source, SourceStatus::Unavailable, Some(message.clone()));
        FetchResult::failed(SourceStatus::Unavailable, message)
    }

    fn remember(&self, source: &str, status: SourceStatus, message: Option<String>) {
        self.sources
            .lock()
            .unwrap()
            .insert(source.to_string(), status);
        let mut messages = self.source_messages.lock().unwrap();
        match message {
            Some(message) => {
                messages.insert(source.to_string(), message);
            }
            None => {
                messages.remove(source);
            }
        }
    }

    fn cached(&self, key: &str) -> Option<RawResponse> {
        let mut cache = self.cache.lock().unwrap();
        let entry = cache.get(key)?;
        if entry.stored.elapsed() > self.options.cache_ttl {
            cache.remove(key);
            return None;
        }
        Some(entry.response.clone())
    }

    fn store_cache(&self, key: String, response: RawResponse) {
        let mut cache = self.cache.lock().unwrap();
        if cache.len() >= self.options.cache_capacity
            && let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.stored)
                .map(|(key, _)| key.clone())
        {
            cache.remove(&oldest);
        }
        cache.insert(
            key,
            CacheEntry {
                stored: Instant::now(),
                response,
            },
        );
    }

    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
    }

    pub fn reset_source(&self, source: &str, url: &Url) {
        if let Some(host) = url.host_str() {
            let hosts = self.hosts.lock().unwrap();
            if let Some(entry) = hosts.get(&host.to_lowercase()) {
                let mut inner = entry.inner.lock().unwrap();
                *inner = HostInner::default();
            }
        }
        self.sources
            .lock()
            .unwrap()
            .insert(source.to_string(), SourceStatus::Ok);
        self.source_messages.lock().unwrap().remove(source);
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
        assert_eq!(missing.calls.lock().unwrap().len(), 2);
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

    #[test]
    fn source_goes_unavailable_after_failures() {
        let fake = FakeFetcher::new(vec![
            Err(FetchError::Network),
            Err(FetchError::Network),
            Err(FetchError::Network),
        ]);
        let polite = PoliteFetcher::new(fake.clone(), options());
        let page = url("https://example.org/mod");
        for _ in 0..3 {
            polite.clear_cache();
            assert_eq!(polite.get("site", &page).status, SourceStatus::Unavailable);
        }
        assert_eq!(polite.status_of("site"), SourceStatus::Unavailable);
        assert!(polite.message_of("site").is_some());
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
}

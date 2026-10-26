//! Страницы Modland за Cloudflare грузятся скрытым WebView2: обычный запрос сайт
//! не пускает. Окно живёт за краем экрана, страницы грузит по одной; куки общие
//! с окном проверки «вы человек?». Капчу программа не решает и не обходит.

use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use bmm_sources::net::{FetchError, PageFetcher, RawResponse, is_challenge_page};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use url::Url;

const LABEL: &str = "modland-pages";
const LOAD_TIMEOUT: Duration = Duration::from_secs(15);
// Cloudflare иногда снимает проверку сам за несколько секунд; ждём, как legacy.
const SELF_CLEAR_WAIT: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(120);
const EVAL_TIMEOUT: Duration = Duration::from_secs(5);
/// Те же пределы, что у обычного HTTP: страница и картинка не должны
/// занимать больше памяти только потому, что их принёс WebView2.
const MAX_HTML_BYTES: usize = 16 * 1024 * 1024;
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;

pub struct BrowserPages {
    app: Mutex<Option<AppHandle>>,
    window: Mutex<Option<WebviewWindow>>,
    one: Mutex<()>,
}

impl BrowserPages {
    pub fn new() -> Self {
        Self {
            app: Mutex::new(None),
            window: Mutex::new(None),
            one: Mutex::new(()),
        }
    }

    /// Приложение создано — теперь можно открыть скрытое окно браузера.
    pub fn attach(&self, app: AppHandle) {
        *self.app.lock().unwrap() = Some(app);
    }

    fn window(&self) -> Result<WebviewWindow, FetchError> {
        let mut held = self.window.lock().map_err(|_| FetchError::Network)?;
        if let Some(window) = held.as_ref() {
            // Закрытое окно осталось в кэше — открываем заново.
            if window.url().is_ok() {
                return Ok(window.clone());
            }
            *held = None;
        }
        let app = self
            .app
            .lock()
            .map_err(|_| FetchError::Network)?
            .clone()
            .ok_or(FetchError::Network)?;
        if let Some(existing) = app.get_webview_window(LABEL) {
            *held = Some(existing.clone());
            return Ok(existing);
        }
        let blank: Url = "about:blank"
            .parse()
            .expect("статический адрес about:blank");
        let created = WebviewWindowBuilder::new(&app, LABEL, WebviewUrl::External(blank))
            .title("Modland")
            .position(-32000.0, -32000.0)
            .inner_size(1024.0, 900.0)
            .decorations(false)
            .skip_taskbar(true)
            .focused(false)
            // Окно не должно мешать: страницы грузятся и без показа, а проверку
            // «вы человек?» человек проходит в отдельном видимом окне.
            .visible(false)
            .build()
            .map_err(|_| FetchError::Network)?;
        *held = Some(created.clone());
        Ok(created)
    }

    fn guard(&self) -> Result<MutexGuard<'_, ()>, FetchError> {
        self.one.lock().map_err(|_| FetchError::Network)
    }

    fn begin(&self, window: &WebviewWindow, url: &Url) -> Result<String, FetchError> {
        if !is_web_url(url) {
            return Err(FetchError::Message(
                "Открывать можно только страницы http и https.".to_string(),
            ));
        }
        let token = format!(
            "bmm-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|time| time.as_nanos())
                .unwrap_or_default()
        );
        // Метка на старом документе: по ней видно, что пришёл уже новый, а не прежняя страница.
        let _ = window.eval(format!(
            "document.documentElement && document.documentElement.setAttribute('data-bmm-fetch','{token}')"
        ));
        if window.url().ok().is_some_and(|current| current == *url) {
            // Повторная загрузка того же адреса: navigate может не перезагрузить страницу.
            let _ = window.eval("location.reload()");
        } else {
            window
                .navigate(url.clone())
                .map_err(|_| FetchError::Network)?;
        }
        Ok(token)
    }
}

impl PageFetcher for BrowserPages {
    fn get(&self, url: &Url) -> Result<RawResponse, FetchError> {
        let _one = self.guard()?;
        let window = self.window()?;
        let token = self.begin(&window, url)?;
        wait_loaded(&window, url, &token)?;
        let mut html = document_html(&window)?;
        let until = Instant::now() + SELF_CLEAR_WAIT;
        while is_challenge_page(&html) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(600));
            if let Ok(next) = document_html(&window) {
                html = next;
            }
        }
        if html.is_empty() {
            return Err(FetchError::Network);
        }
        if looks_like_error_page(&html) {
            return Err(FetchError::Network);
        }
        let final_url = window.url().unwrap_or_else(|_| url.clone());
        Ok(RawResponse {
            status: 200,
            text: html,
            final_url,
            retry_after: None,
            etag: None,
            last_modified: None,
        })
    }

    fn get_bytes(&self, url: &Url) -> Result<Option<(String, Vec<u8>)>, FetchError> {
        let _one = self.guard()?;
        let window = self.window()?;
        let token = self.begin(&window, url)?;
        wait_loaded(&window, url, &token)?;
        let mut page = document_html(&window).unwrap_or_default();
        let until = Instant::now() + SELF_CLEAR_WAIT;
        while is_challenge_page(&page) && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(600));
            if let Ok(next) = document_html(&window) {
                page = next;
            }
        }
        if is_challenge_page(&page) {
            return Ok(None);
        }
        let value = eval_json_timeout(&window, &image_script(), Duration::from_secs(20))?;
        image_from_script(&value, MAX_IMAGE_BYTES)
    }
}

/// Ответ скрипта картинки: `null` — картинки нет, иначе `{type, data}` с base64.
fn image_from_script(
    value: &serde_json::Value,
    max_bytes: usize,
) -> Result<Option<(String, Vec<u8>)>, FetchError> {
    if value.is_null() {
        return Ok(None);
    }
    let content_type = value
        .get("type")
        .and_then(|value| value.as_str())
        .unwrap_or_default()
        .to_string();
    let encoded = value
        .get("data")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| FetchError::Message("сайт отдал некорректную картинку".to_string()))?;
    if bytes.is_empty() || bytes.len() > max_bytes {
        return Ok(None);
    }
    Ok(Some((content_type, bytes)))
}

/// Картинку читаем из самой страницы: адрес уже открыт, поэтому это свой источник без CORS.
const IMAGE_SCRIPT: &str = r#"(() => {
  try {
    // Синхронный XHR не даёт включить responseType; бинарные байты читаем как latin-1.
    const xhr = new XMLHttpRequest();
    xhr.open('GET', location.href, false);
    xhr.overrideMimeType('text/plain; charset=x-user-defined');
    xhr.send(null);
    if (xhr.status < 200 || xhr.status >= 300) return null;
    const text = xhr.responseText;
    if (!text.length) return null;
    if (text.length > __MAX_IMAGE_BYTES__) return null;
    const bytes = new Uint8Array(text.length);
    for (let i = 0; i < text.length; i += 1) bytes[i] = text.charCodeAt(i) & 0xff;
    let binary = '';
    for (let i = 0; i < bytes.length; i += 8192) {
      binary += String.fromCharCode.apply(null, bytes.subarray(i, i + 8192));
    }
    return { type: xhr.getResponseHeader('Content-Type') || '', data: btoa(binary) };
  } catch (error) {
    return null;
  }
})()"#;

fn image_script() -> String {
    IMAGE_SCRIPT.replace("__MAX_IMAGE_BYTES__", &MAX_IMAGE_BYTES.to_string())
}

fn wait_loaded(window: &WebviewWindow, target: &Url, token: &str) -> Result<(), FetchError> {
    let Some(target_host) = target.host_str() else {
        return Err(FetchError::Network);
    };
    let base = base_host(target_host);
    let deadline = Instant::now() + LOAD_TIMEOUT;
    loop {
        if Instant::now() >= deadline {
            return Err(FetchError::Timeout);
        }
        let script = r#"(() => {
            const root = document.documentElement;
            return {
                ready: document.readyState,
                host: location.hostname,
                token: root ? root.getAttribute('data-bmm-fetch') : null
            };
        })()"#;
        if let Ok(state) = eval_json(window, script) {
            let ready = state.get("ready").and_then(|value| value.as_str()) == Some("complete");
            let same_site = state
                .get("host")
                .and_then(|value| value.as_str())
                .is_some_and(|host| base_host(host) == base);
            let fresh = state.get("token").and_then(|value| value.as_str()) != Some(token);
            if ready && same_site && fresh {
                return Ok(());
            }
        }
        std::thread::sleep(POLL);
    }
}

pub(crate) fn document_html(window: &WebviewWindow) -> Result<String, FetchError> {
    let value = eval_json(window, &html_script(MAX_HTML_BYTES))?;
    html_from_value(&value, MAX_HTML_BYTES)
}

fn html_script(limit: usize) -> String {
    format!(
        "(() => {{ const root = document.documentElement; if (!root) return ''; const html = root.outerHTML; return html.length > {limit} ? null : html; }})()"
    )
}

fn html_from_value(value: &serde_json::Value, limit: usize) -> Result<String, FetchError> {
    if value.is_null() {
        return Err(FetchError::TooLarge);
    }
    let html = value.as_str().unwrap_or_default();
    if html.len() > limit {
        return Err(FetchError::TooLarge);
    }
    Ok(html.to_string())
}

fn eval_json(window: &WebviewWindow, script: &str) -> Result<serde_json::Value, FetchError> {
    eval_json_timeout(window, script, EVAL_TIMEOUT)
}

fn eval_json_timeout(
    window: &WebviewWindow,
    script: &str,
    timeout: Duration,
) -> Result<serde_json::Value, FetchError> {
    let (sender, receiver) = mpsc::channel();
    window
        .eval_with_callback(script, move |result| {
            let _ = sender.send(result);
        })
        .map_err(|_| FetchError::Network)?;
    let raw = receiver
        .recv_timeout(timeout)
        .map_err(|_| FetchError::Timeout)?;
    serde_json::from_str(&raw).map_err(|_| FetchError::Network)
}

fn is_web_url(url: &Url) -> bool {
    matches!(url.scheme(), "http" | "https")
}

fn base_host(host: &str) -> String {
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() > 2 {
        labels[labels.len() - 2..].join(".")
    } else {
        host
    }
}

fn looks_like_error_page(html: &str) -> bool {
    html.contains("id=\"main-frame-error\"") || html.contains("chrome-error://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_from_script_decodes_base64() {
        let value = serde_json::json!({"type": "image/png", "data": "iVBORw0KGgo="});
        let (kind, bytes) = image_from_script(&value, MAX_IMAGE_BYTES).unwrap().unwrap();
        assert_eq!(kind, "image/png");
        assert_eq!(bytes, b"\x89PNG\r\n\x1a\n");
        assert!(
            image_from_script(&serde_json::Value::Null, MAX_IMAGE_BYTES)
                .unwrap()
                .is_none()
        );
        let empty = serde_json::json!({"type": "image/png", "data": ""});
        assert!(
            image_from_script(&empty, MAX_IMAGE_BYTES)
                .unwrap()
                .is_none()
        );
        let broken = serde_json::json!({"type": "image/png", "data": "@@@"});
        assert!(image_from_script(&broken, MAX_IMAGE_BYTES).is_err());
    }

    #[test]
    fn image_payload_limit_is_enforced() {
        assert!(
            image_script().contains(&MAX_IMAGE_BYTES.to_string()),
            "в скрипт подставлен настоящий предел"
        );
        let bytes = b"\x89PNG\r\n\x1a\nx";
        let value = serde_json::json!({
            "type": "image/png",
            "data": BASE64.encode(bytes),
        });
        assert!(image_from_script(&value, bytes.len()).unwrap().is_some());
        assert!(
            image_from_script(&value, bytes.len() - 1)
                .unwrap()
                .is_none(),
            "картинка сверх предела не доходит до кеша"
        );
    }

    #[test]
    fn html_limit_is_checked_before_and_after_the_script() {
        assert!(html_script(12).contains("> 12 ? null"));
        assert!(matches!(
            html_from_value(&serde_json::Value::Null, 12),
            Err(FetchError::TooLarge)
        ));
        assert_eq!(
            html_from_value(&serde_json::json!("abc"), 3).unwrap(),
            "abc"
        );
        assert!(matches!(
            html_from_value(&serde_json::json!("abcd"), 3),
            Err(FetchError::TooLarge)
        ));
    }

    #[test]
    fn only_http_and_https_are_web_urls() {
        for raw in ["https://modland.net/x", "http://example.org/a"] {
            assert!(is_web_url(&Url::parse(raw).unwrap()), "{raw}");
        }
        for raw in [
            "javascript:alert(1)",
            "file:///C:/Windows/System32/calc.exe",
            "data:text/html,hello",
        ] {
            assert!(!is_web_url(&Url::parse(raw).unwrap()), "{raw}");
        }
    }

    #[test]
    fn subdomains_share_the_same_site() {
        assert_eq!(base_host("www.modland.net"), "modland.net");
        assert_eq!(base_host("temp2.modland.net"), "modland.net");
        assert_eq!(base_host("MODLAND.NET"), "modland.net");
    }

    #[test]
    fn chrome_error_page_is_not_a_mod_page() {
        assert!(looks_like_error_page(
            "<html><body><div id=\"main-frame-error\">ERR_NAME_NOT_RESOLVED</div></body></html>"
        ));
        assert!(looks_like_error_page(
            "<html><body>chrome-error://chromewebdata/</body></html>"
        ));
        assert!(!looks_like_error_page(
            "<html><body>Toyota Sienna</body></html>"
        ));
    }
}

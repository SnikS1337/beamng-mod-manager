//! Выпуски из папки вместо GitHub: для проверки самообновления на сборочной
//! машине. Отвечает на любой разрешённый адрес файлом с тем же именем, что
//! последняя часть адреса (`latest.json`, `ModManagerBeamNG.zip`); нет файла —
//! 404. Адреса, размер, SHA-256 и версия exe проверяются как обычно:
//! подменяется только сеть.
//!
//! Папка не должна отдавать чужие файлы: имя обязано быть простым (без `..`,
//! слэшей, процентов и запрещённых в Windows символов).

use std::fs::File;
use std::path::{Path, PathBuf};

use url::Url;

use crate::download::{HttpResponse, HttpTransport, TransportError};

pub struct LocalFeedTransport {
    dir: PathBuf,
}

impl LocalFeedTransport {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl HttpTransport for LocalFeedTransport {
    fn send(&self, url: &Url) -> Result<HttpResponse, TransportError> {
        let Some(raw_name) = url
            .path_segments()
            .and_then(|mut segments| segments.next_back())
        else {
            return Ok(not_found());
        };
        // Адрес в URL закодирован: имя файла раскодируем.
        let Some(name) = percent_decode(raw_name) else {
            return Ok(not_found());
        };
        if !is_safe_file_name(&name) {
            return Ok(not_found());
        }
        let path = self.dir.join(&name);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(_) => return Ok(not_found()),
        };
        let content_length = file.metadata().ok().map(|metadata| metadata.len());
        Ok(HttpResponse {
            status: 200,
            content_length,
            location: None,
            body: Box::new(file),
        })
    }
}

fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex_value(*bytes.get(index + 1)?)?;
            let low = hex_value(*bytes.get(index + 2)?)?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn not_found() -> HttpResponse {
    HttpResponse {
        status: 404,
        content_length: Some(0),
        location: None,
        body: Box::new(std::io::empty()),
    }
}

fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains('%')
        && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|'])
        && !name.chars().any(char::is_control)
}

//! Разбор JSON5 с пределом вложенности.
//!
//! json5 не ограничивает вложенность, а глубокий `[[[[…` роняет поток переполнением
//! стека — вся программа закрывается. Так можно уронить её подменённым или
//! испорченным `db.json` (его пишет и сама игра, и Lua-моды), `library.json`,
//! настройками или info.json мода. Настоящие файлы заметно мельче предела.

use serde::de::DeserializeOwned;

/// Предел вложенности скобок для всех файлов JSON5 программы.
pub const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Json5Error {
    /// Скобок глубже [`MAX_DEPTH`] — файл не разбираем вовсе.
    TooDeep,
    Syntax(String),
}

impl std::fmt::Display for Json5Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooDeep => write!(f, "слишком глубокая вложенность"),
            Self::Syntax(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for Json5Error {}

/// `json5::from_str`, но сначала проверяет вложенность.
pub fn from_str<T: DeserializeOwned>(text: &str) -> Result<T, Json5Error> {
    if depth(text) > MAX_DEPTH {
        return Err(Json5Error::TooDeep);
    }
    json5::from_str(text).map_err(|error| Json5Error::Syntax(error.to_string()))
}

/// Наибольшая вложенность `[`/`{` вне строк и комментариев. Останавливается,
/// как только превышен [`MAX_DEPTH`]: дальше считать незачем.
pub fn depth(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut max = 0usize;
    let mut quote: Option<u8> = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            index += 1;
            continue;
        }
        if block_comment {
            if byte == b'*' && bytes.get(index + 1) == Some(&b'/') {
                block_comment = false;
                index += 2;
                continue;
            }
            index += 1;
            continue;
        }
        if let Some(active) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'"' | b'\'' => quote = Some(byte),
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                line_comment = true;
                index += 2;
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                block_comment = true;
                index += 2;
                continue;
            }
            b'[' | b'{' => {
                depth += 1;
                max = max.max(depth);
                if depth > MAX_DEPTH {
                    return depth;
                }
            }
            b']' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        index += 1;
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn deep_text_is_rejected_before_json5() {
        let bomb = "[".repeat(100_000);
        assert!(depth(&bomb) > MAX_DEPTH);
        assert_eq!(from_str::<Value>(&bomb), Err(Json5Error::TooDeep));

        let deep = format!("{}{}", "[".repeat(MAX_DEPTH), "]".repeat(MAX_DEPTH));
        assert_eq!(depth(&deep), MAX_DEPTH);
        assert!(from_str::<Value>(&deep).is_ok());
    }

    #[test]
    fn brackets_in_strings_and_comments_do_not_count() {
        assert_eq!(depth(r#"{"text":"[[[[[[[["}"#), 1);
        assert_eq!(depth(r#"{'a':'[[\'[['}"#), 1);
        assert_eq!(depth("// [[[[\n{ /* ]]] */ }"), 1);
    }

    #[test]
    fn syntax_errors_are_reported() {
        assert!(matches!(
            from_str::<Value>("{oops"),
            Err(Json5Error::Syntax(_))
        ));
        assert_eq!(from_str::<Value>("{a: 1,}").unwrap()["a"], 1);
    }
}

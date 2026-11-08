//! Цвет окна под цветом темы.
//!
//! Пока WebView перерисовывает страницу (после «Свернуть» — «Развернуть», при
//! запуске), видно окно под ней. Было — чёрный прямоугольник на пару секунд, в светлой
//! теме особенно заметный. Теперь окно того же цвета, что фон программы.

use tauri::window::Color;
use tauri::{Manager, WebviewWindow};

use crate::error::AppError;

/// `#rrggbb` → цвет; что-то другое — `None`.
pub(crate) fn parse_hex(value: &str) -> Option<Color> {
    let hex = value.trim().strip_prefix('#')?;
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some(Color(channel(0)?, channel(2)?, channel(4)?, 255))
}

#[tauri::command]
pub fn window_background(window: WebviewWindow, color: String) -> Result<(), AppError> {
    let parsed = parse_hex(&color).ok_or_else(|| AppError::new("color", "Некорректный цвет."))?;
    // Только своё главное окно: окна сайтов живут со своими цветами.
    let target = window
        .app_handle()
        .get_webview_window("main")
        .unwrap_or(window);
    target
        .set_background_color(Some(parsed))
        .map_err(|error| AppError::new("window", error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_theme_colors() {
        assert_eq!(parse_hex("#0e0f11"), Some(Color(14, 15, 17, 255)));
        assert_eq!(parse_hex(" #F1F2F4 "), Some(Color(241, 242, 244, 255)));
    }

    #[test]
    fn rejects_anything_else() {
        for bad in [
            "",
            "#fff",
            "0e0f11",
            "#0e0f1",
            "#0e0f11aa",
            "#zzzzzz",
            "rgb(1,2,3)",
        ] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
    }
}

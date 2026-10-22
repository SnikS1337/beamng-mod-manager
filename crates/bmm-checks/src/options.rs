//! Пределы одной проверки обновлений.

use std::time::Duration;

use chrono::TimeDelta;

use bmm_domain::settings::AppSettings;

/// Пока результат свежее, повторно не проверяем (часы; настройка `CheckTtlHours`).
pub const DEFAULT_TTL_HOURS: i64 = 6;
/// Сколько страниц ленты читать на сайт по умолчанию.
pub const DEFAULT_FEED_PAGES_BASE: u32 = 3;
/// Каждые столько привязок добавляют ещё одну страницу ленты.
pub const DEFAULT_BINDINGS_PER_EXTRA_FEED_PAGE: usize = 50;
/// Потолок страниц ленты на сайт.
pub const DEFAULT_MAX_FEED_PAGES_PER_SOURCE: u32 = 10;
/// Потолок открытых карточек на сайт за проверку.
pub const DEFAULT_MAX_CARD_CHECKS_PER_SOURCE: usize = 150;
/// Общий предел времени одной проверки.
pub const DEFAULT_MAX_DURATION: Duration = Duration::from_secs(5 * 60);

/// Сколько запросов можно потратить на проверку и что считать свежим.
///
/// Значения по умолчанию повторяют legacy: TTL 6 ч, 3 страницы ленты плюс
/// страница на каждые 50 привязок, не больше 10 страниц, 150 карточек на сайт
/// и 5 минут на всю проверку.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateCheckOptions {
    /// TTL результата проверки; `Force` его игнорирует.
    pub ttl: TimeDelta,
    pub feed_pages_base: u32,
    pub bindings_per_extra_feed_page: usize,
    pub max_feed_pages_per_source: u32,
    /// Сколько карточек открывать на сайте за одну проверку.
    pub max_card_checks_per_source: usize,
    /// Игнорировать TTL и интервалы (кнопка «проверить принудительно»).
    pub force: bool,
    /// Общий предел времени одной проверки; остаток переносится на следующую.
    pub max_duration: Duration,
}

impl Default for UpdateCheckOptions {
    fn default() -> Self {
        Self {
            ttl: TimeDelta::hours(DEFAULT_TTL_HOURS),
            feed_pages_base: DEFAULT_FEED_PAGES_BASE,
            bindings_per_extra_feed_page: DEFAULT_BINDINGS_PER_EXTRA_FEED_PAGE,
            max_feed_pages_per_source: DEFAULT_MAX_FEED_PAGES_PER_SOURCE,
            max_card_checks_per_source: DEFAULT_MAX_CARD_CHECKS_PER_SOURCE,
            force: false,
            max_duration: DEFAULT_MAX_DURATION,
        }
    }
}

impl UpdateCheckOptions {
    /// Те же пределы, но с другим флагом «принудительно».
    pub fn with_force(&self, force: bool) -> Self {
        Self {
            force,
            ..self.clone()
        }
    }

    /// Берёт TTL и лимит карточек из настроек приложения (остальное — по умолчанию).
    pub fn from_settings(settings: &AppSettings) -> Self {
        let check = settings.to_check_options();
        Self {
            ttl: TimeDelta::hours((check.ttl.as_secs() / 3600) as i64),
            max_card_checks_per_source: check.max_cards.max(1) as usize,
            ..Self::default()
        }
    }

    /// TTL в целых часах — удобно для интерфейса и логов.
    pub fn ttl_hours(&self) -> i64 {
        self.ttl.num_hours()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_legacy() {
        let options = UpdateCheckOptions::default();
        assert_eq!(options.ttl, TimeDelta::hours(6));
        assert_eq!(options.feed_pages_base, 3);
        assert_eq!(options.bindings_per_extra_feed_page, 50);
        assert_eq!(options.max_feed_pages_per_source, 10);
        assert_eq!(options.max_card_checks_per_source, 150);
        assert_eq!(options.max_duration, Duration::from_secs(300));
        assert!(!options.force);
    }

    #[test]
    fn settings_override_ttl_and_cards() {
        let settings = AppSettings {
            check_ttl_hours: 12,
            max_card_checks_per_source: 40,
            ..AppSettings::default()
        };
        let options = UpdateCheckOptions::from_settings(&settings);
        assert_eq!(options.ttl, TimeDelta::hours(12));
        assert_eq!(options.max_card_checks_per_source, 40);
        assert_eq!(options.feed_pages_base, 3);
    }

    #[test]
    fn force_copy_keeps_limits() {
        let options = UpdateCheckOptions::default().with_force(true);
        assert!(options.force);
        assert_eq!(
            options.max_card_checks_per_source,
            DEFAULT_MAX_CARD_CHECKS_PER_SOURCE
        );
    }
}

//! Что говорит файл за кнопкой «Скачать» о найденном обновлении.
//!
//! Сайты-перезаливщики подписывают моды как придётся: пишут новую версию над прежним
//! файлом («Smoke Dust FX 1.14.1» при 0.14.1 у автора) или не трогают подпись, когда
//! файл уже новый. Имя и размер самого файла показывают, что на самом деле лежит.
//! Имя без версии ничего не говорит — тогда решает размер.

use std::sync::LazyLock;

use fancy_regex::Regex;

use crate::filename;
use crate::library::FileSnapshot;
use crate::versioning::{UpdateStatus, VersionComparer, VersionOrder};

/// Вывод по файлу. `None` из [`judge`] — файл ничего не меняет.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileVerdict {
    /// Это тот же файл, что у вас: размер совпал, а в имени нет версии новее вашей.
    SameFile,
    /// Сайт пишет новую версию, а файл подписан вашей или старее (и размер другой):
    /// возможно, автор не переименовал файл, а возможно, подпись сайта неверна.
    LabelOlder { file_version: String },
    /// На странице версия прежняя, а в имени файла — новее вашей.
    FileNewer { file_version: String },
}

impl FileVerdict {
    pub fn code(&self) -> &'static str {
        match self {
            FileVerdict::SameFile => "same_file",
            FileVerdict::LabelOlder { .. } => "label_older",
            FileVerdict::FileNewer { .. } => "file_newer",
        }
    }

    pub fn file_version(&self) -> Option<&str> {
        match self {
            FileVerdict::SameFile => None,
            FileVerdict::LabelOlder { file_version } | FileVerdict::FileNewer { file_version } => {
                Some(file_version)
            }
        }
    }
}

/// Что установлено у человека.
#[derive(Debug, Clone, Copy)]
pub struct Installed<'a> {
    pub version: Option<&'a str>,
    pub size: Option<i64>,
}

/// Подпись страницы, к которой относится снимок файла: версия, иначе размер.
pub fn page_key(version: Option<&str>, size: Option<i64>) -> String {
    match (version, size) {
        (Some(version), _) => version.to_string(),
        (None, Some(size)) => format!("size:{size}"),
        (None, None) => "size:".to_string(),
    }
}

/// «0.38», «0.39.2» — так BeamNG нумерует саму игру; в имени файла это почти всегда
/// версия игры, под которую сделан мод, а не версия мода.
static GAME_VERSION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^0\.[1-4]\d(?:\.\d{1,2})?$").unwrap());

/// Версия мода из имени файла, если она там есть.
pub fn version_in_name(name: &str) -> Option<String> {
    filename::from_file_name(name)
        .version
        .filter(|version| !GAME_VERSION.is_match(version).unwrap_or(false))
}

/// Совпадает ли размер файла на сайте с вашим. `None` — сравнить нечем или сайт
/// пишет размер слишком грубо («1.2 GB»).
pub fn same_size(installed: Option<i64>, file: &FileSnapshot) -> Option<bool> {
    let ours = installed.filter(|size| *size > 0)?;
    let theirs = file.size.filter(|size| *size > 0)?;
    if file.exact {
        return Some(ours == theirs);
    }
    const MIB: f64 = 1024.0 * 1024.0;
    // Файлообменник пишет мегабайты с двумя знаками. Больше гигабайта он пишет
    // гигабайтами — по ним одинаковыми покажутся разные файлы.
    if theirs as f64 >= 1024.0 * MIB {
        return None;
    }
    let shown = (theirs as f64 / MIB * 100.0).round() / 100.0;
    // Чьи мегабайты — двоичные или десятичные — сайт не говорит: подходит любые.
    let close = |value: f64| ((value * 100.0).round() / 100.0 - shown).abs() <= 0.0101;
    Some(close(ours as f64 / MIB) || close(ours as f64 / 1_000_000.0))
}

/// Вывод по файлу для находки со статусом `page_status` (решение по странице).
pub fn judge(
    installed: Installed<'_>,
    page_status: UpdateStatus,
    current_page_key: &str,
    file: &FileSnapshot,
) -> Option<FileVerdict> {
    // Снимок сделан при другой подписи страницы — он уже ни о чём не говорит.
    if file.page_key.as_deref() != Some(current_page_key) {
        return None;
    }
    let name = file.name.as_deref()?;
    let file_version = version_in_name(name);
    let order = VersionComparer::compare(installed.version, file_version.as_deref());
    let same = same_size(installed.size, file);
    match page_status {
        UpdateStatus::Likely | UpdateStatus::Possible => {
            if same == Some(true) && order != VersionOrder::Newer {
                return Some(FileVerdict::SameFile);
            }
            if page_status == UpdateStatus::Likely
                && matches!(order, VersionOrder::Same | VersionOrder::Older)
            {
                return file_version.map(|file_version| FileVerdict::LabelOlder { file_version });
            }
            None
        }
        UpdateStatus::None => {
            if order == VersionOrder::Newer && same != Some(true) {
                return file_version.map(|file_version| FileVerdict::FileNewer { file_version });
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIB: i64 = 1024 * 1024;

    fn file(name: &str, size: Option<i64>, exact: bool, key: &str) -> FileSnapshot {
        FileSnapshot {
            offer: None,
            seen_at: None,
            url: "https://modsfire.com/abc".to_string(),
            name: Some(name.to_string()),
            size,
            exact,
            page_key: Some(key.to_string()),
        }
    }

    fn mine(version: Option<&str>, size: Option<i64>) -> Installed<'_> {
        Installed { version, size }
    }

    #[test]
    fn same_name_and_size_is_the_same_file() {
        // Smoke Dust FX: сайт пишет 1.14.1, а по ссылке лежит ваш файл.
        let ours = 4_321_987;
        let snapshot = file("smoke_dust_fx.zip", Some(ours), true, "1.14.1");
        assert_eq!(
            judge(
                mine(Some("0.14.1"), Some(ours)),
                UpdateStatus::Likely,
                "1.14.1",
                &snapshot
            ),
            Some(FileVerdict::SameFile)
        );
    }

    #[test]
    fn file_name_with_your_version_and_your_size_is_the_same_file() {
        let ours = 10 * MIB + 12_345;
        let snapshot = file("Dummy_v1.1.zip", Some(ours), true, "1.2");
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(ours)),
                UpdateStatus::Likely,
                "1.2",
                &snapshot
            ),
            Some(FileVerdict::SameFile)
        );
    }

    #[test]
    fn your_version_in_the_name_but_other_size_is_only_possible() {
        // Сайт пишет 1.2, файл подписан 1.1, а размер другой: автор мог не переименовать.
        let snapshot = file("Dummy_v1.1.zip", Some(12 * MIB), true, "1.2");
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(10 * MIB)),
                UpdateStatus::Likely,
                "1.2",
                &snapshot
            ),
            Some(FileVerdict::LabelOlder {
                file_version: "1.1".to_string()
            })
        );
        // Ещё старее вашей — тоже.
        let older = file("Dummy_v1.0.zip", Some(12 * MIB), true, "1.2");
        assert!(matches!(
            judge(
                mine(Some("1.1"), Some(10 * MIB)),
                UpdateStatus::Likely,
                "1.2",
                &older
            ),
            Some(FileVerdict::LabelOlder { .. })
        ));
    }

    #[test]
    fn name_without_version_is_decided_by_size() {
        let ours = 102_644;
        let same = file("acng-freeroam.zip", Some(ours), true, "1.2");
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(ours)),
                UpdateStatus::Likely,
                "1.2",
                &same
            ),
            Some(FileVerdict::SameFile)
        );
        // Другой размер, версии в имени нет: верим сайту.
        let other = file("acng-freeroam.zip", Some(ours + 5000), true, "1.2");
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(ours)),
                UpdateStatus::Likely,
                "1.2",
                &other
            ),
            None
        );
    }

    #[test]
    fn newer_version_in_the_name_confirms_the_update() {
        let snapshot = file("Dummy_v1.2.zip", Some(12 * MIB), true, "1.2");
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(10 * MIB)),
                UpdateStatus::Likely,
                "1.2",
                &snapshot
            ),
            None
        );
        // Даже при совпавшем размере новая версия в имени — не «тот же файл».
        let same_size = file("Dummy_v1.2.zip", Some(10 * MIB), true, "1.2");
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(10 * MIB)),
                UpdateStatus::Likely,
                "1.2",
                &same_size
            ),
            None
        );
    }

    #[test]
    fn site_is_silent_but_the_file_is_newer() {
        let snapshot = file("Dummy_v1.2.zip", Some(12 * MIB), true, "1.1");
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(10 * MIB)),
                UpdateStatus::None,
                "1.1",
                &snapshot
            ),
            Some(FileVerdict::FileNewer {
                file_version: "1.2".to_string()
            })
        );
        // Та же версия в имени, без версии или старее — ничего нового.
        for name in ["Dummy_v1.1.zip", "Dummy.zip", "Dummy_v1.0.zip"] {
            let snapshot = file(name, Some(12 * MIB), true, "1.1");
            assert_eq!(
                judge(
                    mine(Some("1.1"), Some(10 * MIB)),
                    UpdateStatus::None,
                    "1.1",
                    &snapshot
                ),
                None,
                "{name}"
            );
        }
    }

    #[test]
    fn game_version_in_the_name_is_not_the_mod_version() {
        // «car_0.39.zip» — сделано под игру 0.39, а у вас мод 0.3.
        assert_eq!(version_in_name("car_0.39.zip"), None);
        assert_eq!(version_in_name("car_0.38.3.zip"), None);
        let snapshot = file("car_0.39.zip", Some(12 * MIB), true, "0.3");
        assert_eq!(
            judge(
                mine(Some("0.3"), Some(10 * MIB)),
                UpdateStatus::None,
                "0.3",
                &snapshot
            ),
            None
        );
        assert_eq!(version_in_name("Dummy_v1.2.zip").as_deref(), Some("1.2"));
    }

    #[test]
    fn unknown_installed_version_does_not_invent_a_newer_file() {
        let snapshot = file("Dummy_v1.2.zip", Some(12 * MIB), true, "size:1");
        assert_eq!(
            judge(
                mine(None, Some(10 * MIB)),
                UpdateStatus::None,
                "size:1",
                &snapshot
            ),
            None
        );
    }

    #[test]
    fn snapshot_of_another_page_label_says_nothing() {
        // Файл смотрели, когда сайт писал 1.1; теперь пишет 1.2 — снимок устарел.
        let ours = 4_000_000;
        let snapshot = file("mod.zip", Some(ours), true, "1.1");
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(ours)),
                UpdateStatus::Likely,
                "1.2",
                &snapshot
            ),
            None
        );
        let mut no_key = snapshot.clone();
        no_key.page_key = None;
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(ours)),
                UpdateStatus::Likely,
                "1.1",
                &no_key
            ),
            None
        );
    }

    #[test]
    fn file_without_a_name_says_nothing() {
        let mut snapshot = file("x.zip", Some(5), true, "1.2");
        snapshot.name = None;
        assert_eq!(
            judge(
                mine(Some("1.1"), Some(5)),
                UpdateStatus::Likely,
                "1.2",
                &snapshot
            ),
            None
        );
    }

    #[test]
    fn rounded_size_of_a_file_host_is_compared_by_what_it_shows() {
        // modsfire пишет «243.84 MB»; ваш файл 243.84 МиБ ± округление.
        let shown = (243.84 * MIB as f64) as i64;
        let approx = file("E34.7z", Some(shown), false, "3.3");
        assert_eq!(same_size(Some(shown + 3000), &approx), Some(true));
        assert_eq!(same_size(Some(shown - 3000), &approx), Some(true));
        assert_eq!(same_size(Some(shown + 60_000), &approx), Some(false));
        // Десятичные мегабайты: 243.84 «MB» = 243 840 000 байт.
        let decimal = file("E34.7z", Some(shown), false, "3.3");
        assert_eq!(same_size(Some(243_840_000), &decimal), Some(true));
        // Больше гигабайта сайт пишет грубо — не судим.
        let huge = file("map.zip", Some(2 * 1024 * MIB), false, "1");
        assert_eq!(same_size(Some(2 * 1024 * MIB), &huge), None);
        // Точный размер — до байта.
        let exact = file("a.zip", Some(1000), true, "1");
        assert_eq!(same_size(Some(1001), &exact), Some(false));
        assert_eq!(same_size(None, &exact), None);
        assert_eq!(same_size(Some(0), &exact), None);
    }

    #[test]
    fn size_change_from_the_site_with_your_file_behind_the_link_is_the_same_file() {
        // Сайт показал другой размер («Возможно»), а по ссылке точно ваш файл.
        let ours = 7_777_777;
        let snapshot = file("map.zip", Some(ours), true, "size:9000000");
        assert_eq!(
            judge(
                mine(None, Some(ours)),
                UpdateStatus::Possible,
                "size:9000000",
                &snapshot
            ),
            Some(FileVerdict::SameFile)
        );
    }

    #[test]
    fn page_key_is_version_then_size() {
        assert_eq!(page_key(Some("1.2"), Some(5)), "1.2");
        assert_eq!(page_key(None, Some(5)), "size:5");
        assert_eq!(page_key(None, None), "size:");
    }

    #[test]
    fn verdict_codes_for_the_window() {
        assert_eq!(FileVerdict::SameFile.code(), "same_file");
        let older = FileVerdict::LabelOlder {
            file_version: "1.1".to_string(),
        };
        assert_eq!(older.code(), "label_older");
        assert_eq!(older.file_version(), Some("1.1"));
        assert_eq!(FileVerdict::SameFile.file_version(), None);
    }
}

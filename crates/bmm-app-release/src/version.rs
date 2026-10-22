//! Версии программы и правила «эта версия новее».
//!
//! Сравнение делает [`bmm_domain::versioning::VersionComparer`] — те же правила,
//! что и у проверки обновлений модов: «0.2» и «0.2.0» одна версия, «0.1.9»
//! старше «0.1.30» (числа, а не строки).

use std::fmt;
use std::str::FromStr;

use bmm_domain::versioning::{VersionComparer, VersionOrder};
use serde::Serialize;

/// Версия из четырёх чисел. Недостающие части — нули: `0.1.30` и `0.1.30.0`
/// одна и та же версия.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppVersion {
    parts: [u32; 4],
}

impl AppVersion {
    pub const fn new(major: u32, minor: u32, build: u32, revision: u32) -> Self {
        Self {
            parts: [major, minor, build, revision],
        }
    }

    /// «0.1.30», «v0.1.30», «0.1.30.0», «0.1.30+abc», «0.1.30-beta» → 0.1.30.0.
    /// Мусор, больше четырёх чисел, пустые части и отрицательные числа → `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        let without_v = trimmed.trim_start_matches(['v', 'V']);
        let cut = without_v.find(['+', '-', ' ']).unwrap_or(without_v.len());
        let core = &without_v[..cut];
        if core.is_empty() {
            return None;
        }

        let mut parts = [0u32; 4];
        let mut count = 0usize;
        for part in core.split('.') {
            if count == parts.len() || part.is_empty() {
                return None;
            }
            // Как System.Version: не больше int.MaxValue.
            let value: u32 = part.parse().ok()?;
            if value > i32::MAX as u32 {
                return None;
            }
            parts[count] = value;
            count += 1;
        }
        (count > 0).then_some(Self { parts })
    }

    pub const fn major(self) -> u32 {
        self.parts[0]
    }

    pub const fn minor(self) -> u32 {
        self.parts[1]
    }

    pub const fn build(self) -> u32 {
        self.parts[2]
    }

    pub const fn revision(self) -> u32 {
        self.parts[3]
    }

    /// Строго новее: на ту же или старую версию не «обновляемся».
    pub fn is_newer_than(self, current: &Self) -> bool {
        match VersionComparer::compare(Some(&current.to_string()), Some(&self.to_string())) {
            VersionOrder::Newer => true,
            VersionOrder::Older | VersionOrder::Same => false,
            // Наши версии всегда состоят из чисел и точек; запасной путь —
            // числовое сравнение, если сравнение домена чего-то не разобрало.
            VersionOrder::Unknown => self > *current,
        }
    }

    /// Сборка с машины сборки, которой можно обновляться: 0.x с номером
    /// («0.1.129», «0.2», «0.3.1»). 1.0.0.0 — без номера (`dotnet run`,
    /// папка разработки), 0.0.0.0 — тоже не выпуск.
    pub const fn is_release(self) -> bool {
        self.major() == 0 && (self.minor() > 0 || self.build() > 0)
    }

    /// Как показывать человеку: без хвостовых нулей
    /// («0.1.30», «0.2», «0.2.1», «0.2.0.4»).
    pub fn display(self) -> String {
        if self.revision() > 0 {
            format!(
                "{}.{}.{}.{}",
                self.major(),
                self.minor(),
                self.build(),
                self.revision()
            )
        } else if self.build() > 0 {
            format!("{}.{}.{}", self.major(), self.minor(), self.build())
        } else {
            format!("{}.{}", self.major(), self.minor())
        }
    }
}

impl fmt::Display for AppVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display())
    }
}

/// Текст не похож на версию программы.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("не удалось разобрать версию «{0}»")]
pub struct VersionError(pub String);

impl FromStr for AppVersion {
    type Err = VersionError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::parse(text).ok_or_else(|| VersionError(text.to_string()))
    }
}

impl Serialize for AppVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.display())
    }
}

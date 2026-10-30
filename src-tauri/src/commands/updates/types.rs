//! DTO проверки обновлений для окна.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "UpdateFinding")]
pub struct UpdateFindingDto {
    pub mod_id: String,
    pub mod_name: String,
    pub source: String,
    #[ts(type = "UpdateStatusText")]
    pub status: String,
    pub installed_version: Option<String>,
    pub latest_version: Option<String>,
    pub url: String,
    pub dismissed: bool,
    /// Картинка мода (как на карточке), если уже есть в кэше.
    pub preview_url: Option<String>,
    /// `vehicle`, `map` или `addon`. У дополнений картинка из архива случайная,
    /// поэтому её не показываем.
    #[serde(default)]
    #[ts(type = "ModRole | null")]
    pub role: Option<String>,
    pub installed_size: Option<i64>,
    pub latest_size: Option<i64>,
    /// Когда программа заметила изменение на сайте (RFC 3339).
    pub noticed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "UpdateSite")]
pub struct UpdateSiteDto {
    pub id: String,
    pub display_name: String,
    /// `ok`, `human_check`, `unavailable`, `off` (выключен в настройках).
    #[ts(type = "UpdateSiteStatus")]
    pub status: String,
    /// За сколькими модами следим на этом сайте.
    pub tracked: usize,
    /// Последняя проверка мода на этом сайте (RFC 3339).
    #[serde(default)]
    pub last_checked: Option<String>,
    /// Последнее объяснение от сайта из отчёта проверки.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "PageSuggestion")]
pub struct PageSuggestionDto {
    pub mod_id: String,
    pub mod_name: String,
    pub source: String,
    pub source_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "PageMatch")]
pub struct PageMatchDto {
    pub source: String,
    pub title: String,
    pub url: String,
    pub version: Option<String>,
    pub size: Option<i64>,
    pub author: Option<String>,
    pub similarity: f64,
    pub same_size: bool,
    pub same_version: bool,
    pub same_author: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "PageVerification")]
pub struct PageVerificationDto {
    pub page_source: String,
    pub page_title: String,
    pub page_url: String,
    pub page_size: Option<i64>,
    pub matches: Vec<PageMatchDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "UpdateSource")]
pub struct UpdateSourceDto {
    pub id: String,
    pub display_name: String,
    #[ts(type = "UpdateSourceStatus")]
    pub status: String,
    pub cards_fetched: u32,
    pub feed_pages_fetched: u32,
    pub postponed: usize,
    pub missing: usize,
    pub unreadable: usize,
    pub layout_changed: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "UpdateProgress")]
pub struct UpdateProgressDto {
    pub source: String,
    pub stage: String,
    pub done: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "UpdatesSnapshot")]
pub struct UpdatesSnapshotDto {
    pub findings: Vec<UpdateFindingDto>,
    pub hidden: Vec<UpdateFindingDto>,
    pub suggestions: Vec<PageSuggestionDto>,
    /// Сайты и их состояние — панель «Проверка».
    #[serde(default)]
    pub sites: Vec<UpdateSiteDto>,
    /// Последняя проверка любого мода (RFC 3339).
    #[serde(default)]
    pub last_checked: Option<String>,
    /// Страницы, которых несколько проверок подряд нет на сайте: мод удалили
    /// или перенесли. Сама программа за ними уже ничего не узнает.
    #[serde(default)]
    pub lost: Vec<LostPageDto>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "LostPage")]
pub struct LostPageDto {
    pub mod_id: String,
    pub mod_name: String,
    pub source: String,
    pub url: String,
    pub preview_url: Option<String>,
    #[serde(default)]
    #[ts(type = "ModRole | null")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "UpdatesEvent")]
pub struct UpdatesEventDto {
    pub request_id: u64,
    #[ts(type = "UpdatesPhase")]
    pub phase: String,
    pub progress: Option<UpdateProgressDto>,
    pub findings: Vec<UpdateFindingDto>,
    pub hidden: Vec<UpdateFindingDto>,
    pub suggestions: Vec<PageSuggestionDto>,
    pub sources: Vec<UpdateSourceDto>,
    pub new_count: usize,
    pub up_to_date: usize,
    pub requests: u32,
    pub cancelled: bool,
    pub stopped_by_time: bool,
    pub error: Option<String>,
    /// Поиск страниц: за сколькими найденными страницами программа стала следить сама.
    #[serde(default)]
    pub followed: usize,
}

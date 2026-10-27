//! DTO гаража и библиотеки.

use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CompatDto {
    pub mod_for: Option<String>,
    pub game: Option<String>,
    pub text: Option<String>,
    pub differs: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModCardDto {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub version: Option<String>,
    pub size_bytes: i64,
    pub enabled: bool,
    pub sub_folder: Option<String>,
    #[ts(type = "ModKind")]
    pub kind: String,
    pub preview_url: Option<String>,
    pub compat: CompatDto,
    #[ts(type = "UpdateStatusText")]
    pub update: String,
    pub update_version: Option<String>,
    pub unchecked: bool,
    #[ts(type = "PendingState")]
    pub pending: String,
    pub from_game_repo: bool,
    pub deleted: bool,
    pub copies: usize,
    pub saved_at: Option<String>,
    /// Почему игра не загрузит архив: `wrapped`, `loose`, `not_mod`; `None` — в порядке.
    #[ts(type = "PackagingProblem | null")]
    pub problem: Option<String>,
    /// Повторяет другой мод (основной в группе): в списке не показывается отдельно.
    pub duplicate_of: Option<DuplicateDto>,
    /// У основного в группе — остальные копии и версии.
    pub group: Vec<String>,
    /// В группе включено больше одного: игра загрузит все.
    pub group_conflict: bool,
    /// «vehicle», «map», «addon»; `None` — архив ещё не прочитан.
    #[ts(type = "ModRole | null")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "DuplicateOf")]
pub struct DuplicateDto {
    pub id: String,
    pub name: String,
    /// Та же копия (состав и размер совпали), иначе другая версия тех же машин или карт.
    pub exact: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FilterCounts {
    pub all: usize,
    pub enabled: usize,
    pub updates: usize,
    pub unchecked: usize,
    pub deleted: usize,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct LibrarySnapshot {
    pub mods: Vec<ModCardDto>,
    pub counts: FilterCounts,
    pub folder: Option<String>,
    pub folder_error: Option<String>,
    pub game_version: Option<String>,
    pub game_running: bool,
    pub warning: Option<String>,
    pub many_enabled: Option<String>,
    pub deleted: Vec<ModCardDto>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "DeletedTicket")]
pub struct DeletedTicketDto {
    pub id: String,
    pub file_name: String,
    pub backup_id: String,
    pub was_in_disabled: bool,
    pub name: String,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(rename = "VersionRow")]
pub struct VersionRowDto {
    pub backup_id: String,
    pub file_name: String,
    pub version: Option<String>,
    pub saved_at: Option<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ToggleResult {
    pub outcome: String,
    pub enabled: bool,
    pub message: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveDto {
    pub vehicles: Vec<String>,
    pub levels: Vec<String>,
    pub has_scripts: bool,
    pub lua_files: usize,
    pub files: usize,
    pub unpacked_bytes: u64,
    pub warnings: Vec<String>,
    pub unreadable: bool,
    pub title: Option<String>,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BindingDto {
    pub source: String,
    pub url: String,
    pub confirmed: bool,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ModDetails {
    pub id: String,
    pub name: String,
    pub file_name: String,
    pub version: Option<String>,
    pub size_bytes: i64,
    pub enabled: bool,
    pub installed_at: String,
    pub folder: Option<String>,
    #[ts(type = "ModKind")]
    pub kind: String,
    pub compat: CompatDto,
    pub archive: Option<ArchiveDto>,
    pub preview_url: Option<String>,
    pub binding: Option<BindingDto>,
    /// Пользователь выбрал «Не проверять обновления» для этого мода.
    pub checking_declined: bool,
}

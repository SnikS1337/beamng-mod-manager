//! TypeScript-типы DTO рождаются из Rust: `UPDATE_TS=1 cargo test -p bmm-desktop export_ts_types`.
//!
//! `generated.ts` — формы объектов (поля, null, вложенные DTO). Строковые значения
//! (`mod.kind`, `status`, `phase`) живут в `unions.ts` и сверяются тестами `ts_union`:
//! ts-rs видит только тип `String`, а не то, что в нём лежит.

use std::path::Path;

use ts_rs::{Config, TS};

const HEADER: &str = "\
// Сгенерировано из Rust. Не править руками.
// Формы полей взяты из DTO, строковые значения — из unions.ts.
// Пересобрать: UPDATE_TS=1 cargo test -p bmm-desktop export_ts_types

import type {
  AppUpdateStage,
  BackgroundStyle,
  BackgroundTheme,
  FindingSeverity,
  IncomingKind,
  ModKind,
  ModRole,
  PackagingProblem,
  PendingState,
  SearchOfferKind,
  SearchPhase,
  SearchPrice,
  SearchPriceSummary,
  SearchSourceStatus,
  UiMode,
  UpdateSiteStatus,
  UpdateSourceStatus,
  UpdateStatusText,
  UpdatesPhase,
} from \"./unions.js\";

";

fn declarations(cfg: &Config) -> Vec<String> {
    use crate::commands::app_update::{AppUpdateDto, AppUpdateManifestDto, AppUpdateStateDto};
    use crate::commands::download_progress::DownloadProgress;
    use crate::commands::download_window::{DownloadPanelEvent, DownloadRequest};
    use crate::commands::downloads::DownloadReady;
    use crate::commands::incoming::{
        IncomingArchiveDto, IncomingFindingDto, IncomingInstallRequest, IncomingInstallResult,
        IncomingItemDto, IncomingTicketDto, SimilarModDto,
    };
    use crate::commands::library::{
        ArchiveDto, BindingDto, BulkToggleResult, CompatDto, DeletedTicketDto, DuplicateDto,
        FilterCounts, LibrarySnapshot, ModCardDto, ModDetails, ToggleResult, VersionRowDto,
    };
    use crate::commands::search::{
        OfferDto, SearchCardDto, SearchEventDto, SearchListingDto, SearchSourceDto,
    };
    use crate::commands::settings::SettingsDto;
    use crate::commands::updates::{
        LostPageDto, PageMatchDto, PageSuggestionDto, PageVerificationDto, UpdateFindingDto,
        UpdateProgressDto, UpdateSiteDto, UpdateSourceDto, UpdatesEventDto, UpdatesSnapshotDto,
    };
    use crate::error::AppError;

    vec![
        AppError::decl(cfg),
        SettingsDto::decl(cfg),
        DownloadRequest::decl(cfg),
        DownloadPanelEvent::decl(cfg),
        DownloadProgress::decl(cfg),
        DownloadReady::decl(cfg),
        AppUpdateManifestDto::decl(cfg),
        AppUpdateStateDto::decl(cfg),
        AppUpdateDto::decl(cfg),
        IncomingFindingDto::decl(cfg),
        SimilarModDto::decl(cfg),
        IncomingItemDto::decl(cfg),
        IncomingArchiveDto::decl(cfg),
        IncomingInstallRequest::decl(cfg),
        IncomingInstallResult::decl(cfg),
        IncomingTicketDto::decl(cfg),
        OfferDto::decl(cfg),
        SearchListingDto::decl(cfg),
        SearchCardDto::decl(cfg),
        SearchSourceDto::decl(cfg),
        SearchEventDto::decl(cfg),
        CompatDto::decl(cfg),
        DuplicateDto::decl(cfg),
        ModCardDto::decl(cfg),
        FilterCounts::decl(cfg),
        LibrarySnapshot::decl(cfg),
        DeletedTicketDto::decl(cfg),
        VersionRowDto::decl(cfg),
        ArchiveDto::decl(cfg),
        BindingDto::decl(cfg),
        ModDetails::decl(cfg),
        ToggleResult::decl(cfg),
        BulkToggleResult::decl(cfg),
        UpdateFindingDto::decl(cfg),
        UpdateSiteDto::decl(cfg),
        PageSuggestionDto::decl(cfg),
        PageMatchDto::decl(cfg),
        PageVerificationDto::decl(cfg),
        UpdateSourceDto::decl(cfg),
        UpdateProgressDto::decl(cfg),
        LostPageDto::decl(cfg),
        UpdatesSnapshotDto::decl(cfg),
        UpdatesEventDto::decl(cfg),
    ]
}

fn generated(cfg: &Config) -> String {
    let mut text = String::from(HEADER);
    for declaration in declarations(cfg) {
        text.push_str("export ");
        text.push_str(&declaration);
        text.push_str("\n\n");
    }
    text
}

#[test]
fn export_ts_types() {
    let cfg = Config::new().with_large_int("number");
    let expected = generated(&cfg);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib/ipc/generated.ts");
    if std::env::var_os("UPDATE_TS").is_some() {
        std::fs::write(&path, &expected).unwrap();
        return;
    }
    let actual = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(
        actual.replace("\r\n", "\n"),
        expected.replace("\r\n", "\n"),
        "generated.ts разошёлся с DTO. Пересобери: UPDATE_TS=1 cargo test -p bmm-desktop export_ts_types"
    );
}

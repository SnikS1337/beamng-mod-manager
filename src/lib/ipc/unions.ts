// Строковые значения, которые Rust отдаёт в полях DTO.
// Сверяются тестами (`ts_union`): значения здесь должны совпадать с тем,
// что пишет Rust. Формы объектов живут в generated.ts и не правятся руками.

export type ModKind = "vehicle" | "level" | "sound" | "script" | "unknown";
export type PendingState = "none" | "enabling" | "disabling" | "queued";
export type ModRole = "vehicle" | "map" | "addon";
export type PackagingProblem = "wrapped" | "loose" | "not_mod";

export type UpdateStatusText = "none" | "likely" | "possible";
export type UpdateSiteStatus = "ok" | "human_check" | "unavailable" | "off";
export type UpdateSourceStatus = "ok" | "needsHumanCheck" | "unavailable";
export type UpdatesPhase = "progress" | "done" | "pages" | "error" | "auto";

export type IncomingKind = "new" | "update" | "alreadyInstalled";
export type FindingSeverity = "info" | "warning" | "block";

export type SearchSourceStatus = "ok" | "needsHumanCheck" | "unavailable";
export type SearchPrice = "unknown" | "free" | "paid";
export type SearchPriceSummary = SearchPrice | "mixed";
export type SearchOfferKind = "direct" | "externalHost" | "paidLink";
export type SearchPhase = "partial" | "done" | "error" | "regroup";

export type BackgroundStyle = "accent" | "aurora" | "sunset" | "ocean" | "nebula" | "mono";
export type UiMode = "dark" | "light" | "system";
export type BackgroundTheme = "night" | "deep" | "dusk" | "forest" | "graphite";

export type AppUpdateStage =
  | "idle"
  | "checking"
  | "upToDate"
  | "available"
  | "downloading"
  | "readyToRestart"
  | "failed";

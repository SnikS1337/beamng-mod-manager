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
} from "./unions.js";

export type AppError = { code: string, message: string, };

export type SettingsDto = { allowDownloads: boolean, modsFolder: string | null, userFolder: string | null, downloadsFolder: string | null, deleteDownloadsAfterInstall: boolean, checkTtlHours: number, autoCheckOnStart: boolean, checkAppUpdates: boolean, autoCheckEveryHours: number, checkDisabledMods: boolean, disabledSources: Array<string>, keepBackups: number, maxCardChecksPerSource: number, liquidGlass: boolean, mode: UiMode, theme: BackgroundTheme, background: BackgroundStyle, uiAnimations: boolean, animatedBackground: boolean, fireflies: boolean, backgroundTilt: boolean, activeModsWarnAbove: number, firstRunDone: boolean, watchFolders: Array<string>, };

export type DownloadRequest = { modName: string, installedVersion: string | null, latestVersion: string | null, pageUrl: string, previewUrl: string | null, role: ModRole | null, };

export type DownloadPanelEvent = { step: number, title: string, detail: string, error: string | null, };

export type DownloadProgress = { 
/**
 * Скачано байт.
 */
received: number, 
/**
 * Размер файла, если сайт его сообщил.
 */
total: number | null, 
/**
 * Байт в секунду за последние секунды; `None`, пока считать не из чего.
 */
speed: number | null, };

export type DownloadReady = { path: string, };

export type AppUpdateManifest = { version: string, url: string, size: number, sha256: string, notes: string | null, published: string | null, };

export type AppUpdateState = { stage: AppUpdateStage, manifest: AppUpdateManifest | null, progress: number, error: string | null, checkedAt: string | null, };

export type AppUpdateSnapshot = { currentVersion: string, state: AppUpdateState, };

export type IncomingFindingDto = { severity: FindingSeverity, code: string, message: string, entry: string | null, };

export type SimilarModDto = { fileName: string, title: string, };

export type IncomingItemDto = { zipPath: string, fileName: string, size: number, title: string, kind: IncomingKind, replaces: string | null, replacesEnabled: boolean, replacesSubFolder: string | null, locked: boolean, repacked: boolean, blocked: boolean, warnings: boolean, findings: Array<IncomingFindingDto>, 
/**
 * Похоже, такая машина уже стоит другим модом.
 */
similarTo: SimilarModDto | null, };

export type IncomingArchiveDto = { source: string, problem: string | null, workDir: string, items: Array<IncomingItemDto>, };

export type IncomingInstallRequest = { zipPath: string, fileName: string, replaces: string | null, allowWarnings: boolean, };

export type IncomingInstallResult = { success: boolean, message: string | null, 
/**
 * Установлено, но запись библиотеки обновилась не полностью.
 */
warning: string | null, fileName: string, replaces: string | null, backupId: string | null, wasDisabled: boolean, sameName: boolean, };

export type IncomingTicket = { fileName: string, replaces: string | null, backupId: string | null, wasDisabled: boolean, sameName: boolean, };

export type SearchOffer = { kind: SearchOfferKind, host: string, url: string, fileId: string | null, sizeBytes: number | null, };

export type SearchListing = { source: string, sourceId: string, url: string, title: string, 
/**
 * Название без рубрики сайта и версии («автомобиль LADA 2106 v1.0» → «LADA 2106»).
 */
name: string, modVersion: string | null, gameVersion: string | null, author: string | null, category: string | null, sizeBytes: number | null, previewUrl: string | null, price: SearchPrice, offers: Array<SearchOffer>, publishedAt: string | null, updatedAt: string | null, externalId: string | null, installed: boolean, };

export type SearchCard = { id: string, title: string, price: SearchPriceSummary, possibleSameAs: Array<string>, relevance: number, installed: boolean, 
/**
 * «vehicle», «map», «addon» или нет, если сайт не сказал.
 */
kind: ModRole | null, listings: Array<SearchListing>, };

export type SearchSource = { id: string, displayName: string, status: SearchSourceStatus, count: number, note: string | null, pending: boolean, };

export type SearchEvent = { requestId: number, phase: SearchPhase, cards: Array<SearchCard>, sources: Array<SearchSource>, error: string | null, };

export type CompatDto = { modFor: string | null, game: string | null, text: string | null, differs: boolean, };

export type DuplicateOf = { id: string, name: string, 
/**
 * Та же копия (состав и размер совпали), иначе другая версия тех же машин или карт.
 */
exact: boolean, };

export type ModCardDto = { id: string, name: string, fileName: string, version: string | null, sizeBytes: number, enabled: boolean, subFolder: string | null, kind: ModKind, previewUrl: string | null, compat: CompatDto, update: UpdateStatusText, updateVersion: string | null, unchecked: boolean, pending: PendingState, fromGameRepo: boolean, deleted: boolean, copies: number, savedAt: string | null, 
/**
 * Почему игра не загрузит архив: `wrapped`, `loose`, `not_mod`; `None` — в порядке.
 */
problem: PackagingProblem | null, 
/**
 * Повторяет другой мод (основной в группе): в списке не показывается отдельно.
 */
duplicateOf: DuplicateOf | null, 
/**
 * У основного в группе — остальные копии и версии.
 */
group: Array<string>, 
/**
 * В группе включено больше одного: игра загрузит все.
 */
groupConflict: boolean, 
/**
 * «vehicle», «map», «addon»; `None` — архив ещё не прочитан.
 */
role: ModRole | null, };

export type FilterCounts = { all: number, enabled: number, updates: number, unchecked: number, deleted: number, };

export type LibrarySnapshot = { mods: Array<ModCardDto>, counts: FilterCounts, folder: string | null, folderError: string | null, gameVersion: string | null, gameRunning: boolean, warning: string | null, manyEnabled: string | null, deleted: Array<ModCardDto>, };

export type DeletedTicket = { id: string, fileName: string, backupId: string, wasInDisabled: boolean, name: string, };

export type VersionRow = { backupId: string, fileName: string, version: string | null, savedAt: string | null, sizeBytes: number, };

export type ArchiveDto = { vehicles: Array<string>, levels: Array<string>, hasScripts: boolean, luaFiles: number, files: number, unpackedBytes: number, warnings: Array<string>, unreadable: boolean, title: string | null, };

export type BindingDto = { source: string, url: string, confirmed: boolean, };

export type ModDetails = { id: string, name: string, fileName: string, version: string | null, sizeBytes: number, enabled: boolean, installedAt: string, folder: string | null, kind: ModKind, compat: CompatDto, archive: ArchiveDto | null, previewUrl: string | null, binding: BindingDto | null, 
/**
 * Пользователь выбрал «Не проверять обновления» для этого мода.
 */
checkingDeclined: boolean, };

export type ToggleResult = { outcome: string, enabled: boolean, message: string | null, };

export type BulkToggleResult = { changed: Array<string>, queued: Array<string>, failed: Array<string>, };

export type UpdateFinding = { modId: string, modName: string, source: string, status: UpdateStatusText, installedVersion: string | null, latestVersion: string | null, url: string, dismissed: boolean, 
/**
 * Картинка мода (как на карточке), если уже есть в кэше.
 */
previewUrl: string | null, 
/**
 * `vehicle`, `map` или `addon`. У дополнений картинка из архива случайная,
 * поэтому её не показываем.
 */
role: ModRole | null, installedSize: number | null, latestSize: number | null, 
/**
 * Когда программа заметила изменение на сайте (RFC 3339).
 */
noticedAt: string | null, };

export type UpdateSite = { id: string, displayName: string, 
/**
 * `ok`, `human_check`, `unavailable`, `off` (выключен в настройках).
 */
status: UpdateSiteStatus, 
/**
 * За сколькими модами следим на этом сайте.
 */
tracked: number, 
/**
 * Последняя проверка мода на этом сайте (RFC 3339).
 */
lastChecked: string | null, 
/**
 * Последнее объяснение от сайта из отчёта проверки.
 */
note: string | null, };

export type PageSuggestion = { modId: string, modName: string, source: string, sourceId: string, url: string, };

export type PageMatch = { source: string, title: string, url: string, version: string | null, size: number | null, author: string | null, similarity: number, sameSize: boolean, sameVersion: boolean, sameAuthor: boolean, };

export type PageVerification = { pageSource: string, pageTitle: string, pageUrl: string, pageSize: number | null, matches: Array<PageMatch>, };

export type UpdateSource = { id: string, displayName: string, status: UpdateSourceStatus, cardsFetched: number, feedPagesFetched: number, postponed: number, missing: number, unreadable: number, layoutChanged: boolean, note: string | null, };

export type UpdateProgress = { source: string, stage: string, done: number, total: number, };

export type LostPage = { modId: string, modName: string, source: string, url: string, previewUrl: string | null, role: ModRole | null, };

export type CoverageDto = { 
/**
 * Всего модов.
 */
mods: number, 
/**
 * Со страницей, за которой программа следит.
 */
tracked: number, 
/**
 * Без страницы: их обновления программа не видит.
 */
withoutPage: number, 
/**
 * Человек сам сказал «не проверять».
 */
declined: number, };

export type UpdatesSnapshot = { findings: Array<UpdateFinding>, hidden: Array<UpdateFinding>, suggestions: Array<PageSuggestion>, 
/**
 * Сайты и их состояние — панель «Проверка».
 */
sites: Array<UpdateSite>, 
/**
 * Последняя проверка любого мода (RFC 3339).
 */
lastChecked: string | null, 
/**
 * Страницы, которых несколько проверок подряд нет на сайте: мод удалили
 * или перенесли. Сама программа за ними уже ничего не узнает.
 */
lost: Array<LostPage>, 
/**
 * За сколькими модами программа вообще следит: без этого «Новых версий нет»
 * звучало как «всё проверено», хотя моды без страницы не проверялись.
 */
coverage: CoverageDto, };

export type UpdatesEvent = { requestId: number, phase: UpdatesPhase, progress: UpdateProgress | null, findings: Array<UpdateFinding>, hidden: Array<UpdateFinding>, suggestions: Array<PageSuggestion>, sources: Array<UpdateSource>, newCount: number, upToDate: number, requests: number, cancelled: boolean, stoppedByTime: boolean, error: string | null, 
/**
 * Сколько модов программа проверяет — для честного итога.
 */
coverage: CoverageDto | null, 
/**
 * Поиск страниц: за сколькими найденными страницами программа стала следить сама.
 */
followed: number, };


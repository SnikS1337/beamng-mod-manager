import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  FindPageResult,
  PageSuggestion,
  PageVerification,
  UpdateFinding,
  UpdateSource,
  UpdatesEvent,
  UpdatesSnapshot,
} from "./generated.js";

export type {
  FindPageResult,
  LostPage,
  PageMatch,
  PageSuggestion,
  PageVerification,
  UpdateFinding,
  UpdateProgress,
  UpdateSite,
  UpdateSource,
  UpdatesEvent,
  UpdatesSnapshot,
} from "./generated.js";
export type { UpdateStatusText } from "./unions.js";

type Listener = (event: UpdatesEvent) => void;
const listeners = new Set<Listener>();
let mockSnapshot: UpdatesSnapshot = mockInitial();
let mockModlandNeedsCheck = true;
let mockSources: UpdateSource[] = mockSourceReports();
const mockUnfollowed: UpdateFinding[] = [];

function mockInitial(): UpdatesSnapshot {
  return {
    coverage: { mods: 12, tracked: 9, withoutPage: 2, declined: 1 },
    lost: [
      {
        modId: "mock-lost",
        modName: "Gavril Roamer Classic",
        source: "stmods",
        url: "https://stmods.org/beamng_drive/mods/roamer_classic/",
        previewUrl: null,
        role: "vehicle",
      },
    ],
    findings: [
      {
        modId: "mock-1",
        modName: "BMW M2 G87",
        source: "stmods",
        status: "likely",
        installedVersion: "1.0",
        latestVersion: "1.5",
        url: "https://stmods.org/beamng_drive/mods/bmw_m2_g87/",
        dismissed: false,
        previewUrl: null,
        role: "vehicle",
        installedSize: 142 * 1024 * 1024,
        latestSize: 151 * 1024 * 1024,
        noticedAt: new Date(Date.now() - 2 * 86400000).toISOString(),
      },
      {
        modId: "mock-2",
        modName: "Toyota Sienna 2011-2017",
        source: "modsgaming",
        status: "possible",
        installedVersion: "2.0",
        latestVersion: "2.2",
        url: "https://modsgaming.ru/load/beamng_drive/mashiny/38-1-0-160200",
        dismissed: false,
        previewUrl: null,
        role: "vehicle",
        installedSize: 410 * 1024 * 1024,
        latestSize: 438 * 1024 * 1024,
        noticedAt: new Date(Date.now() - 21 * 86400000).toISOString(),
      },
      {
        modId: "mock-3",
        modName: "Карта «Приозерск»",
        source: "modland",
        status: "likely",
        installedVersion: null,
        latestVersion: "0.1",
        url: "https://www.modland.net/beamng.drive-mods/maps/karta-priozersk.html",
        dismissed: false,
        previewUrl: null,
        role: "map",
        installedSize: null,
        latestSize: null,
        noticedAt: null,
      },
    ],
    hidden: [
      {
        modId: "mock-4",
        modName: "Gavril D-Series Offroad",
        source: "stmods",
        status: "possible",
        installedVersion: "0.9",
        latestVersion: "1.0",
        url: "https://stmods.org/beamng_drive/mods/gavril/",
        dismissed: true,
        previewUrl: null,
        role: "vehicle",
        installedSize: null,
        latestSize: null,
        noticedAt: null,
      },
    ],
    suggestions: [
      {
        modId: "mock-5",
        modName: "Audi RS6 C7",
        source: "stmods",
        sourceId: "audi-rs6",
        url: "https://stmods.org/beamng_drive/mods/audi_rs6_c7/",
      },
    ],
    sites: [
      { id: "modland", displayName: "Modland", status: "human_check", tracked: 2, lastChecked: null, note: null },
      { id: "modsgaming", displayName: "ModsGaming", status: "ok", tracked: 5, lastChecked: null, note: null },
      { id: "stmods", displayName: "STMods", status: "ok", tracked: 9, lastChecked: null, note: null },
      { id: "beamng", displayName: "Репозиторий BeamNG", status: "ok", tracked: 0, lastChecked: null, note: null },
    ],
    lastChecked: new Date(Date.now() - 40 * 60000).toISOString(),
  };
}

function mockSourceReports(): UpdateSource[] {
  return [
    {
      id: "modland",
      displayName: "Modland",
      status: mockModlandNeedsCheck ? "needsHumanCheck" : "ok",
      cardsFetched: mockModlandNeedsCheck ? 0 : 12,
      feedPagesFetched: mockModlandNeedsCheck ? 0 : 3,
      postponed: 0,
      missing: 0,
      unreadable: 0,
      layoutChanged: false,
      note: mockModlandNeedsCheck ? "Сайт просит подтвердить, что вы человек." : null,
    },
    {
      id: "stmods",
      displayName: "STMods",
      status: "ok",
      cardsFetched: 8,
      feedPagesFetched: 2,
      postponed: 0,
      missing: 1,
      unreadable: 0,
      layoutChanged: false,
      note: null,
    },
    {
      id: "modsgaming",
      displayName: "ModsGaming",
      status: "ok",
      cardsFetched: 9,
      feedPagesFetched: 2,
      postponed: 0,
      missing: 0,
      unreadable: 0,
      layoutChanged: false,
      note: null,
    },
    {
      id: "beamng",
      displayName: "BeamNG (официальный)",
      status: "ok",
      cardsFetched: 5,
      feedPagesFetched: 1,
      postponed: 0,
      missing: 0,
      unreadable: 0,
      layoutChanged: false,
      note: null,
    },
  ];
}

function emit(event: UpdatesEvent): void {
  for (const listener of listeners) listener(event);
}

export async function listenUpdates(handler: Listener): Promise<UnlistenFn> {
  if (!isTauri()) {
    listeners.add(handler);
    return () => listeners.delete(handler);
  }
  return listen<UpdatesEvent>("updates://event", (event) => handler(event.payload));
}

export async function updatesSnapshot(): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_snapshot");
  return clone(mockSnapshot);
}

export async function updatesStart(force: boolean, requestId: number): Promise<void> {
  if (isTauri()) {
    await invoke("updates_start", { force, requestId });
    return;
  }
  mockStart(requestId);
}

export async function updatesCancel(): Promise<void> {
  if (isTauri()) await invoke("updates_cancel");
}

export async function updatesRetrySource(source: string, requestId: number): Promise<void> {
  if (isTauri()) {
    await invoke("updates_retry_source", { source, requestId });
    return;
  }
  mockStart(requestId);
}

export async function updatesCheckMod(modId: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_check_mod", { modId });
  return clone(mockSnapshot);
}

export async function updatesDeclineMod(modId: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_decline_mod", { modId });
  return clone(mockSnapshot);
}

export async function updatesUndoDecline(modId: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_undo_decline", { modId });
  return clone(mockSnapshot);
}

export async function updatesResumeMod(modId: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_resume_mod", { modId });
  return clone(mockSnapshot);
}

export async function updatesDismiss(modId: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_dismiss", { modId });
  const finding = mockSnapshot.findings.find((item) => item.modId === modId);
  if (finding) {
    mockSnapshot.findings = mockSnapshot.findings.filter((item) => item.modId !== modId);
    mockSnapshot.hidden = [...mockSnapshot.hidden, { ...finding, dismissed: true }];
  }
  return clone(mockSnapshot);
}

export async function updatesRestore(modId: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_restore", { modId });
  const finding = mockSnapshot.hidden.find((item) => item.modId === modId);
  if (finding) {
    mockSnapshot.hidden = mockSnapshot.hidden.filter((item) => item.modId !== modId);
    mockSnapshot.findings = [...mockSnapshot.findings, { ...finding, dismissed: false }];
  }
  return clone(mockSnapshot);
}

export async function updatesFindPages(requestId: number): Promise<void> {
  if (isTauri()) {
    await invoke("updates_find_pages", { requestId });
    return;
  }
  window.setTimeout(() => {
    emit({
      ...clone(mockSnapshot),
      requestId,
      phase: "pages",
      progress: null,
      sources: mockSources,
      newCount: 0,
      upToDate: 0,
      requests: 6,
      cancelled: false,
      stoppedByTime: false,
      followed: 0,
      error: null,
    });
  }, 500);
}

/** Найти страницу одного мода, не дожидаясь 14-дневной паузы. */
export async function updatesFindPage(modId: string): Promise<FindPageResult> {
  if (isTauri()) return invoke<FindPageResult>("updates_find_page", { modId });
  return { snapshot: clone(mockSnapshot), skippedSites: ["Modland"] };
}

/** Открыть страницу и поискать её же на других сайтах — без привязки. */
export async function updatesVerifyPage(modId: string): Promise<PageVerification> {
  if (isTauri()) return invoke<PageVerification>("updates_verify_page", { modId });
  return {
    pageSource: "stmods",
    pageTitle: "Проверочная страница",
    pageUrl: "https://stmods.org/",
    pageSize: null,
    matches: [],
  };
}

export async function updatesConfirmPage(modId: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_confirm_page", { modId });
  mockSnapshot.suggestions = mockSnapshot.suggestions.filter((item) => item.modId !== modId);
  return clone(mockSnapshot);
}

export async function updatesRejectPage(modId: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_reject_page", { modId });
  mockSnapshot.suggestions = mockSnapshot.suggestions.filter((item) => item.modId !== modId);
  return clone(mockSnapshot);
}

export async function updatesUndoPage(suggestion: PageSuggestion): Promise<UpdatesSnapshot> {
  if (isTauri()) {
    return invoke<UpdatesSnapshot>("updates_undo_page", {
      modId: suggestion.modId,
      source: suggestion.source,
      sourceId: suggestion.sourceId,
      url: suggestion.url,
    });
  }
  if (!mockSnapshot.suggestions.some((item) => item.modId === suggestion.modId)) {
    mockSnapshot.suggestions = [...mockSnapshot.suggestions, suggestion];
  }
  return clone(mockSnapshot);
}

/** «Это другой мод» для страницы, за которой программа уже следит. */
export async function updatesUnfollow(modId: string, source: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_unfollow", { modId, source });
  const index = mockSnapshot.findings.findIndex(
    (item) => item.modId === modId && item.source === source,
  );
  if (index < 0) throw new Error("Программа не следит за этой страницей.");
  mockUnfollowed.push(mockSnapshot.findings[index]);
  mockSnapshot.findings = mockSnapshot.findings.filter((_, at) => at !== index);
  return clone(mockSnapshot);
}

/** «Страница пропала»: перестать следить; `searchAgain` — разрешить искать новую сразу. */
export async function updatesForgetLost(
  modId: string,
  source: string,
  searchAgain: boolean,
): Promise<UpdatesSnapshot> {
  if (isTauri()) {
    return invoke<UpdatesSnapshot>("updates_forget_lost", { modId, source, searchAgain });
  }
  const lost = mockSnapshot.lost ?? [];
  const page = lost.find((item) => item.modId === modId && item.source === source);
  if (!page) throw new Error("Программа не следит за этой страницей.");
  mockSnapshot.lost = lost.filter((item) => item !== page);
  return clone(mockSnapshot);
}

/** «Отменить» после «Это другой мод». */
export async function updatesRefollow(modId: string, source: string): Promise<UpdatesSnapshot> {
  if (isTauri()) return invoke<UpdatesSnapshot>("updates_refollow", { modId, source });
  const index = mockUnfollowed.findIndex((item) => item.modId === modId && item.source === source);
  if (index < 0) throw new Error("Вернуть уже нечего.");
  const [finding] = mockUnfollowed.splice(index, 1);
  mockSnapshot.findings = [...mockSnapshot.findings, finding];
  return clone(mockSnapshot);
}

/** Сайты за Cloudflare: проверку «вы человек?» проходит человек в отдельном окне. */
export const CHECK_SITES: Record<string, string> = { modland: "Modland", modhub: "ModHub" };

/** «Пройти проверку» сайта (`modland`, `modhub`). `true` — сайт пустил программу. */
export async function updatesOpenCheck(source: string): Promise<boolean> {
  if (!isTauri()) {
    await new Promise((resolve) => setTimeout(resolve, 900));
    if (source === "modland") mockModlandNeedsCheck = false;
    mockSources = mockSources.map((item) =>
      item.id === source
        ? { ...item, status: "ok", cardsFetched: 12, feedPagesFetched: 3, note: null }
        : item,
    );
    return true;
  }
  return invoke<boolean>("updates_open_check", { source });
}

/** Какой сайт просит проверку по адресу страницы: для «Указать страницу». */
export function checkSiteOf(url: string): string {
  try {
    const host = new URL(url).hostname.toLowerCase();
    if (host === "modhub.us" || host.endsWith(".modhub.us")) return "modhub";
  } catch {
    // Адрес ещё не дописан — по умолчанию Modland, как раньше.
  }
  return "modland";
}

function mockStart(requestId: number): void {
  window.setTimeout(() => {
    emit({
      ...clone(mockSnapshot),
      requestId,
      phase: "progress",
      progress: { source: "stmods", stage: "карточки", done: 4, total: 8 },
      sources: mockSources,
      newCount: 0,
      upToDate: 0,
      requests: 7,
      cancelled: false,
      stoppedByTime: false,
      followed: 0,
      error: null,
    });
  }, 250);
  window.setTimeout(() => {
    emit({
      ...clone(mockSnapshot),
      requestId,
      phase: "done",
      progress: null,
      sources: mockSources,
      newCount: 1,
      upToDate: 18,
      requests: 31,
      cancelled: false,
      stoppedByTime: false,
      followed: 0,
      error: null,
    });
  }, 900);
}

function clone(snapshot: UpdatesSnapshot): UpdatesSnapshot {
  return {
    findings: snapshot.findings.map((item) => ({ ...item })),
    hidden: snapshot.hidden.map((item) => ({ ...item })),
    suggestions: snapshot.suggestions.map((item) => ({ ...item })),
    sites: snapshot.sites?.map((item) => ({ ...item })),
    lastChecked: snapshot.lastChecked ?? null,
    lost: snapshot.lost?.map((item) => ({ ...item })),
    coverage: { ...snapshot.coverage },
  };
}

import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type UpdateStatusText = "none" | "likely" | "possible";

export interface UpdateFinding {
  modId: string;
  modName: string;
  source: string;
  status: UpdateStatusText;
  installedVersion: string | null;
  latestVersion: string | null;
  url: string;
  dismissed: boolean;
}

export interface PageSuggestion {
  modId: string;
  modName: string;
  source: string;
  sourceId: string;
  url: string;
}

export interface UpdateSource {
  id: string;
  displayName: string;
  status: "ok" | "needsHumanCheck" | "unavailable";
  cardsFetched: number;
  feedPagesFetched: number;
  postponed: number;
  missing: number;
  unreadable: number;
  layoutChanged: boolean;
  note: string | null;
}

export interface UpdateProgress {
  source: string;
  stage: string;
  done: number;
  total: number;
}

export interface UpdatesSnapshot {
  findings: UpdateFinding[];
  hidden: UpdateFinding[];
  suggestions: PageSuggestion[];
}

export interface UpdatesEvent extends UpdatesSnapshot {
  requestId: number;
  phase: "progress" | "done" | "pages" | "error";
  progress: UpdateProgress | null;
  sources: UpdateSource[];
  newCount: number;
  upToDate: number;
  requests: number;
  cancelled: boolean;
  stoppedByTime: boolean;
  error: string | null;
}

type Listener = (event: UpdatesEvent) => void;
const listeners = new Set<Listener>();
let mockSnapshot: UpdatesSnapshot = mockInitial();
let mockModlandNeedsCheck = true;
let mockSources: UpdateSource[] = mockSourceReports();

function mockInitial(): UpdatesSnapshot {
  return {
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
      error: null,
    });
  }, 500);
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

export async function updatesOpenModland(): Promise<boolean> {
  if (!isTauri()) {
    await new Promise((resolve) => setTimeout(resolve, 900));
    mockModlandNeedsCheck = false;
    mockSources = mockSources.map((item) =>
      item.id === "modland"
        ? { ...item, status: "ok", cardsFetched: 12, feedPagesFetched: 3, note: null }
        : item,
    );
    return true;
  }
  return invoke<boolean>("updates_open_modland");
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
      error: null,
    });
  }, 900);
}

function clone(snapshot: UpdatesSnapshot): UpdatesSnapshot {
  return {
    findings: snapshot.findings.map((item) => ({ ...item })),
    hidden: snapshot.hidden.map((item) => ({ ...item })),
    suggestions: snapshot.suggestions.map((item) => ({ ...item })),
  };
}

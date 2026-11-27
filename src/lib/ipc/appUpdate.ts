import { invoke, isTauri } from "@tauri-apps/api/core";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { stableListen } from "#lib/shell/stableListen.js";

import type { AppUpdateSnapshot } from "./generated.js";

export type {
  AppUpdateManifest,
  AppUpdateSnapshot,
  AppUpdateState,
} from "./generated.js";
export type { AppUpdateStage } from "./unions.js";

let mock: AppUpdateSnapshot = {
  currentVersion: "1.0.2",
  revision: 1,
  state: {
    stage: "available",
    manifest: {
      version: "1.0.3",
      url: "https://github.com/SnikS1337/beamng-mod-manager/releases/download/v1.0.3/BeamModManager-1.0.3.zip",
      size: 18 * 1024 * 1024,
      sha256: "demo",
      notes: "Исправления поиска и обновлений.",
      published: "2026-10-22T12:00:00Z",
    },
    progress: 0,
    error: null,
    checkedAt: new Date().toISOString(),
  },
};

const listeners = new Set<(progress: number, revision: number) => void>();
const stateListeners = new Set<(snapshot: AppUpdateSnapshot) => void>();

export async function listenAppUpdateProgress(
  handler: (progress: number, revision: number) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    listeners.add(handler);
    return () => listeners.delete(handler);
  }
  return stableListen<{ progress: number; revision: number }>("app-update://event", (event) =>
    handler(event.payload.progress, event.payload.revision),
  );
}

export async function listenAppUpdateState(
  handler: (snapshot: AppUpdateSnapshot) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    stateListeners.add(handler);
    return () => stateListeners.delete(handler);
  }
  return stableListen<AppUpdateSnapshot>("app-update://state", (event) =>
    handler(clone(event.payload)),
  );
}

export async function appUpdateState(): Promise<AppUpdateSnapshot> {
  if (isTauri()) return invoke<AppUpdateSnapshot>("app_update_state");
  return clone(mock);
}

export async function appUpdateCheck(): Promise<AppUpdateSnapshot> {
  if (isTauri()) return invoke<AppUpdateSnapshot>("app_update_check");
  mock = {
    ...mock,
    revision: mock.revision + 1,
    state: { ...mock.state, stage: "available", progress: 0, error: null },
  };
  emitMockState();
  return clone(mock);
}

export async function appUpdateDownload(): Promise<AppUpdateSnapshot> {
  if (isTauri()) return invoke<AppUpdateSnapshot>("app_update_download");
  mock = {
    ...mock,
    revision: mock.revision + 1,
    state: { ...mock.state, stage: "downloading", progress: 0 },
  };
  emitMockState();
  for (const listener of listeners) listener(0.4, mock.revision);
  mock = {
    ...mock,
    state: { ...mock.state, stage: "downloading", progress: 0.4 },
  };
  for (const listener of listeners) listener(1, mock.revision);
  mock = {
    ...mock,
    revision: mock.revision + 1,
    state: { ...mock.state, stage: "readyToRestart", progress: 1 },
  };
  emitMockState();
  return clone(mock);
}

export async function appUpdateCancel(): Promise<AppUpdateSnapshot> {
  if (isTauri()) return invoke<AppUpdateSnapshot>("app_update_cancel");
  mock = {
    ...mock,
    revision: mock.revision + 1,
    state: {
      ...mock.state,
      stage: "failed",
      error: "Загрузка обновления отменена.",
      progress: 0,
    },
  };
  emitMockState();
  return clone(mock);
}

export async function appUpdateRestart(): Promise<void> {
  if (isTauri()) await invoke("app_update_restart");
}

export function applyAppUpdateSnapshot(
  current: AppUpdateSnapshot | null,
  incoming: AppUpdateSnapshot,
): AppUpdateSnapshot {
  return current && incoming.revision < current.revision ? current : incoming;
}

export function applyAppUpdateProgress(
  current: AppUpdateSnapshot | null,
  progress: number,
  revision: number,
): AppUpdateSnapshot | null {
  if (!current || current.revision !== revision) return current;
  return { ...current, state: { ...current.state, progress } };
}

function emitMockState(): void {
  for (const listener of stateListeners) listener(clone(mock));
}

function clone(snapshot: AppUpdateSnapshot): AppUpdateSnapshot {
  return {
    currentVersion: snapshot.currentVersion,
    revision: snapshot.revision,
    state: {
      ...snapshot.state,
      manifest: snapshot.state.manifest ? { ...snapshot.state.manifest } : null,
    },
  };
}

import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AppUpdateSnapshot } from "./generated.js";

export type {
  AppUpdateManifest,
  AppUpdateSnapshot,
  AppUpdateState,
} from "./generated.js";
export type { AppUpdateStage } from "./unions.js";

let mock: AppUpdateSnapshot = {
  currentVersion: "1.0.2",
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

const listeners = new Set<(progress: number) => void>();

export async function listenAppUpdateProgress(
  handler: (progress: number) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    listeners.add(handler);
    return () => listeners.delete(handler);
  }
  return listen<{ progress: number }>("app-update://event", (event) =>
    handler(event.payload.progress),
  );
}

export async function listenAppUpdateState(
  handler: (snapshot: AppUpdateSnapshot) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<AppUpdateSnapshot>("app-update://state", (event) =>
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
    state: { ...mock.state, stage: "available", progress: 0, error: null },
  };
  return clone(mock);
}

export async function appUpdateDownload(): Promise<AppUpdateSnapshot> {
  if (isTauri()) return invoke<AppUpdateSnapshot>("app_update_download");
  for (const listener of listeners) listener(0.4);
  mock = {
    ...mock,
    state: { ...mock.state, stage: "downloading", progress: 0.4 },
  };
  for (const listener of listeners) listener(1);
  mock = {
    ...mock,
    state: { ...mock.state, stage: "readyToRestart", progress: 1 },
  };
  return clone(mock);
}

export async function appUpdateCancel(): Promise<AppUpdateSnapshot> {
  if (isTauri()) return invoke<AppUpdateSnapshot>("app_update_cancel");
  mock = {
    ...mock,
    state: {
      ...mock.state,
      stage: "failed",
      error: "Загрузка обновления отменена.",
      progress: 0,
    },
  };
  return clone(mock);
}

export async function appUpdateRestart(): Promise<void> {
  if (isTauri()) await invoke("app_update_restart");
}

function clone(snapshot: AppUpdateSnapshot): AppUpdateSnapshot {
  return {
    currentVersion: snapshot.currentVersion,
    state: {
      ...snapshot.state,
      manifest: snapshot.state.manifest ? { ...snapshot.state.manifest } : null,
    },
  };
}

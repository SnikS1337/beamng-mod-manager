import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  DownloadPanelEvent,
  DownloadProgress,
  DownloadReady,
  DownloadRequest,
} from "./generated.js";

export type {
  DownloadPanelEvent,
  DownloadProgress,
  DownloadReady,
  DownloadRequest,
} from "./generated.js";

const DEMO: DownloadRequest = {
  modName: "Dacia Sandero Stepway",
  installedVersion: "1.0",
  latestVersion: "1.5",
  pageUrl: "https://modsfire.com/abc123",
  previewUrl: null,
  role: "vehicle",
};

export async function downloadOpen(request: DownloadRequest): Promise<void> {
  if (!isTauri()) throw new Error("Окно загрузки доступно в приложении.");
  await invoke("download_open", { request });
}

export async function downloadClose(): Promise<void> {
  if (isTauri()) await invoke("download_close");
}

export async function downloadPanelState(): Promise<DownloadRequest | null> {
  if (!isTauri()) return DEMO;
  return invoke<DownloadRequest | null>("download_panel_state");
}

/** Готовый архив, если событие пришло раньше, чем панель успела подписаться. */
export async function downloadReady(): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("download_ready");
}

export async function listenDownloadReady(
  handler: (ready: DownloadReady) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DownloadReady>("download://ready", (event) => handler(event.payload));
}

export async function listenDownloadContext(
  handler: (request: DownloadRequest) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DownloadRequest>("download://context", (event) => handler(event.payload));
}

export async function listenDownloadState(
  handler: (event: DownloadPanelEvent) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DownloadPanelEvent>("download://state", (event) => handler(event.payload));
}

/** Ход загрузки: байты, размер, скорость — несколько раз в секунду. */
export async function listenDownloadProgress(
  handler: (progress: DownloadProgress) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DownloadProgress>("download://progress", (event) => handler(event.payload));
}

/** «Обновить все»: окно откроет страницы модов по очереди, установка — в конце. */
export async function downloadOpenBatch(requests: DownloadRequest[]): Promise<void> {
  if (!isTauri()) throw new Error("Окно загрузки доступно в приложении.");
  await invoke("download_open_batch", { requests });
}

/** Очередь пачки; пустая — обычное обновление одного мода. */
export async function downloadBatch(): Promise<DownloadRequest[]> {
  if (!isTauri()) return [];
  return invoke<DownloadRequest[]>("download_batch");
}

export async function downloadBatchGoto(index: number): Promise<void> {
  if (isTauri()) await invoke("download_batch_goto", { index });
}

export async function listenDownloadBatch(
  handler: (requests: DownloadRequest[]) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DownloadRequest[]>("download://batch", (event) => handler(event.payload));
}

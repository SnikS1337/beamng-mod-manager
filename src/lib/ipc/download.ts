import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  DownloadJob,
  DownloadPanelEvent,
  DownloadProgress,
  DownloadRequest,
} from "./generated.js";

export type {
  DownloadJob,
  DownloadJobState,
  DownloadPanelEvent,
  DownloadProgress,
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

/** Пост Telegram-канала — в приложении Telegram; программа ждёт скачанный архив. */
export async function telegramOpen(url: string): Promise<void> {
  if (!isTauri()) throw new Error("Открыть Telegram можно в приложении.");
  await invoke("telegram_open", { url });
}

export async function downloadClose(): Promise<void> {
  if (isTauri()) await invoke("download_close");
}

export async function downloadPanelState(): Promise<DownloadRequest | null> {
  if (!isTauri()) return DEMO;
  return invoke<DownloadRequest | null>("download_panel_state");
}

/** Докачанные архивы, если события пришли раньше, чем панель успела подписаться. */
export async function downloadReady(): Promise<DownloadJob[]> {
  if (!isTauri()) return [];
  return invoke<DownloadJob[]>("download_ready");
}

/** Все загрузки окна скачивания: идущие, докачанные и оборванные. */
export async function downloadJobs(): Promise<DownloadJob[]> {
  if (!isTauri()) return [];
  return invoke<DownloadJob[]>("download_jobs");
}

/** Оборвать идущие загрузки: человек решил выйти из программы. */
export async function downloadCancelAll(): Promise<void> {
  if (isTauri()) await invoke("download_cancel_all");
}

/** Показать окно скачивания поверх остальных. */
export async function downloadFocus(): Promise<void> {
  if (isTauri()) await invoke("download_focus");
}

export async function listenDownloadReady(
  handler: (job: DownloadJob) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DownloadJob>("download://ready", (event) => handler(event.payload));
}

export async function listenDownloadJobs(
  handler: (jobs: DownloadJob[]) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DownloadJob[]>("download://jobs", (event) => handler(event.payload));
}

/** Окно закрывают (Alt+F4), а файл ещё качается: панель должна спросить. */
export async function listenDownloadCloseAsked(handler: () => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen("download://close-asked", () => handler());
}

/** Во время пачки нажали «Обновить» у другого мода: он встал в конец очереди. */
export async function listenDownloadBatchAdded(
  handler: (request: DownloadRequest) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<DownloadRequest>("download://batch-added", (event) => handler(event.payload));
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

import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ModRole } from "./library.js";

export interface DownloadRequest {
  modName: string;
  installedVersion: string | null;
  latestVersion: string | null;
  pageUrl: string;
  previewUrl: string | null;
  role: ModRole | null;
}

export interface DownloadPanelEvent {
  step: number;
  title: string;
  detail: string;
  error?: string | null;
}

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

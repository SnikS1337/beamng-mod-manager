import { invoke, isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export type ModSource = "repo" | "modland" | "stmods" | "modsgaming";

export interface ModEntry {
  id: number;
  title: string;
  file: string;
  version: string;
  enabled: boolean;
  updateAvailable: boolean;
  source: ModSource;
  sizeMb: number;
}

export interface EnvInfo {
  appVersion: string;
  os: string;
  arch: string;
  backend: "tauri" | "mock";
}

const TITLES = [
  "Gavril D-Series",
  "Ibishu Covet",
  "ETK 800-Series",
  "Bruckell Bastion",
  "Cherrier FCV",
  "Soliad Wendover",
  "Gavril Roamer",
  "Ibishu Pessima",
  "Hirochi Sunburst",
  "Autobello Piccolina",
  "Wentward DT40L",
  "Civetta Bolide",
] as const;

const SOURCES: ModSource[] = ["repo", "modland", "stmods", "modsgaming"];

function mockLibrary(count: number): ModEntry[] {
  return Array.from({ length: count }, (_, i) => {
    const base = TITLES[i % TITLES.length];
    const round = Math.floor(i / TITLES.length);
    return {
      id: i,
      title: round === 0 ? base : `${base} · вариант ${round + 1}`,
      file: `bmm_mod_${String(i).padStart(5, "0")}.zip`,
      version: `1.${i % 9}.${i % 7}`,
      enabled: i % 3 !== 0,
      updateAvailable: i % 11 === 0,
      source: SOURCES[i % SOURCES.length],
      sizeMb: Math.round((3 + (i % 97) * 1.7) * 10) / 10,
    };
  });
}

export async function loadLibrary(
  count: number,
): Promise<{ items: ModEntry[]; ms: number; backend: "tauri" | "mock" }> {
  const started = performance.now();

  if (isTauri()) {
    const items = await invoke<ModEntry[]>("poc_library", { count });
    return { items, ms: performance.now() - started, backend: "tauri" };
  }

  const items = mockLibrary(count);
  // имитация IPC, чтобы браузерный режим вёл себя похоже на окно Tauri
  await new Promise((resolve) => setTimeout(resolve, 120));
  return { items, ms: performance.now() - started, backend: "mock" };
}

export async function getEnvInfo(): Promise<EnvInfo> {
  if (isTauri()) {
    return invoke<EnvInfo>("poc_environment");
  }

  return {
    appVersion: "web-preview",
    os: navigator.platform || "web",
    arch: "-",
    backend: "mock",
  };
}

export async function openModlandProbe(): Promise<void> {
  if (isTauri()) {
    await invoke("open_modland_probe");
    return;
  }

  window.open("https://www.modland.net/beamng-drive-mods.html", "_blank", "noopener");
}

export async function windowAction(action: "minimize" | "maximize" | "close"): Promise<void> {
  if (!isTauri()) return;

  const win = getCurrentWindow();
  if (action === "minimize") await win.minimize();
  else if (action === "maximize") await win.toggleMaximize();
  else await win.close();
}

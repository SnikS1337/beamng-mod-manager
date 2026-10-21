import { invoke, isTauri } from "@tauri-apps/api/core";

export type ModKind = "vehicle" | "level" | "sound" | "script" | "unknown";
export type PendingState = "none" | "enabling" | "disabling" | "queued";

export interface CompatDto {
  modFor: string | null;
  game: string | null;
  text: string | null;
  differs: boolean;
}

export interface ModCardDto {
  id: string;
  name: string;
  fileName: string;
  version: string | null;
  sizeBytes: number;
  enabled: boolean;
  subFolder: string | null;
  kind: ModKind;
  previewUrl: string | null;
  compat: CompatDto;
  update: "none" | "likely" | "possible";
  pending: PendingState;
}

export interface FilterCounts {
  all: number;
  enabled: number;
  updates: number;
  unchecked: number;
  deleted: number;
}

export interface LibrarySnapshot {
  mods: ModCardDto[];
  counts: FilterCounts;
  folder: string | null;
  folderError: string | null;
  gameVersion: string | null;
  gameRunning: boolean;
  warning: string | null;
}

export interface ArchiveDto {
  vehicles: string[];
  levels: string[];
  hasScripts: boolean;
  luaFiles: number;
  files: number;
  unpackedBytes: number;
  warnings: string[];
  unreadable: boolean;
  title: string | null;
}

export interface BindingDto {
  source: string;
  url: string;
  confirmed: boolean;
}

export interface ModDetails {
  id: string;
  name: string;
  fileName: string;
  version: string | null;
  sizeBytes: number;
  enabled: boolean;
  installedAt: string;
  folder: string | null;
  kind: ModKind;
  compat: CompatDto;
  archive: ArchiveDto | null;
  previewUrl: string | null;
  binding: BindingDto | null;
}

export interface ToggleResult {
  outcome: string;
  enabled: boolean;
  message: string | null;
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

const KINDS: ModKind[] = ["vehicle", "level", "sound", "script", "unknown"];

function mockSnapshot(count = 5000): LibrarySnapshot {
  const mods: ModCardDto[] = Array.from({ length: count }, (_, index) => {
    const base = TITLES[index % TITLES.length];
    const round = Math.floor(index / TITLES.length);
    const name = round === 0 ? base : `${base} · ${round + 1}`;
    const enabled = index % 3 !== 0;
    return {
      id: `mock-${index}`,
      name,
      fileName: `bmm_mod_${String(index).padStart(5, "0")}.zip`,
      version: `1.${index % 9}.${index % 7}`,
      sizeBytes: Math.round((3 + (index % 97) * 1.7) * 10) * 1024 * 1024,
      enabled,
      subFolder: index % 13 === 0 ? "repo" : null,
      kind: KINDS[index % KINDS.length],
      previewUrl: null,
      compat: {
        modFor: index % 7 === 0 ? "0.38" : null,
        game: "0.39",
        text: index % 7 === 0 ? "0.38 — у вас 0.39, может не работать" : "0.39, как у вас",
        differs: index % 7 === 0,
      },
      update: index % 17 === 0 ? "likely" : "none",
      pending: "none",
    };
  });
  const enabled = mods.filter((mod) => mod.enabled).length;
  const updates = mods.filter((mod) => mod.update !== "none").length;
  return {
    mods,
    counts: {
      all: mods.length,
      enabled,
      updates,
      unchecked: 0,
      deleted: 0,
    },
    folder: "C:\\Users\\Player\\AppData\\Local\\BeamNG\\BeamNG.drive\\current\\mods",
    folderError: null,
    gameVersion: "0.39.4",
    gameRunning: false,
    warning: null,
  };
}

export async function libraryLoad(): Promise<LibrarySnapshot> {
  if (isTauri()) {
    return invoke<LibrarySnapshot>("library_load");
  }
  return mockSnapshot();
}

export async function libraryRefresh(): Promise<LibrarySnapshot> {
  if (isTauri()) {
    return invoke<LibrarySnapshot>("library_refresh");
  }
  return mockSnapshot();
}

export async function modToggle(id: string, enabled: boolean): Promise<ToggleResult> {
  if (isTauri()) {
    return invoke<ToggleResult>("mod_toggle", { id, enabled });
  }
  return { outcome: "done", enabled, message: null };
}

export async function modDetails(id: string): Promise<ModDetails> {
  if (isTauri()) {
    return invoke<ModDetails>("mod_details", { id });
  }
  const index = Number(id.replace("mock-", ""));
  return {
    id,
    name: TITLES[index % TITLES.length] ?? "Демо-мод",
    fileName: `bmm_mod_${String(index).padStart(5, "0")}.zip`,
    version: `1.${index % 9}.${index % 7}`,
    sizeBytes: Math.round((3 + (index % 97) * 1.7) * 10) * 1024 * 1024,
    enabled: index % 3 !== 0,
    installedAt: new Date().toISOString(),
    folder: "C:\\Users\\Player\\AppData\\Local\\BeamNG\\BeamNG.drive\\current\\mods",
    kind: KINDS[index % KINDS.length],
    compat: {
      modFor: index % 7 === 0 ? "0.38" : "0.39",
      game: "0.39",
      text: index % 7 === 0 ? "0.38 — у вас 0.39, может не работать" : "0.39, как у вас",
      differs: index % 7 === 0,
    },
    archive: {
      vehicles: [`${TITLES[index % TITLES.length]} · demo`],
      levels: [],
      hasScripts: index % 4 === 0,
      luaFiles: index % 4 === 0 ? 3 : 0,
      files: 24,
      unpackedBytes: 88 * 1024 * 1024,
      warnings: [],
      unreadable: false,
      title: `${TITLES[index % TITLES.length]} · demo`,
    },
    previewUrl: null,
    binding: null,
  };
}

export async function modPreview(id: string): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("mod_preview", { id });
}

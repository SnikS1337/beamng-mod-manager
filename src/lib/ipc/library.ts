import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

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
  updateVersion: string | null;
  unchecked: boolean;
  pending: PendingState;
  fromGameRepo: boolean;
  deleted: boolean;
  copies: number;
  savedAt: string | null;
  /** Почему игра не загрузит архив; null — в порядке. */
  problem: PackagingProblem | null;
  /** Повторяет другой мод — основной в группе; отдельной карточки не показываем. */
  duplicateOf: DuplicateOf | null;
  /** У основного в группе — id остальных копий и версий. */
  group: string[];
  /** В группе включено больше одного: игра загрузит все. */
  groupConflict: boolean;
  /** По составу архива: своя машина, своя карта или дополнение; null — ещё не прочитан. */
  role: ModRole | null;
}

export type ModRole = "vehicle" | "map" | "addon";

export interface DuplicateOf {
  id: string;
  name: string;
  /** Тот же архив; иначе другая версия тех же машин или карт. */
  exact: boolean;
}

export type PackagingProblem = "wrapped" | "loose" | "not_mod";

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
  manyEnabled: string | null;
  deleted: ModCardDto[];
}

export interface DeletedTicket {
  id: string;
  fileName: string;
  backupId: string;
  wasInDisabled: boolean;
  name: string;
}

export interface VersionRow {
  backupId: string;
  fileName: string;
  version: string | null;
  savedAt: string | null;
  sizeBytes: number;
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
  checkingDeclined: boolean;
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

let mockLibrary: LibrarySnapshot | null = null;
const mockDeletedCards = new Map<string, ModCardDto>();

function recount(snapshot: LibrarySnapshot): void {
  const visible = snapshot.mods.filter((mod) => !mod.duplicateOf);
  snapshot.counts.all = visible.length;
  snapshot.counts.enabled = visible.filter((mod) => mod.enabled).length;
  snapshot.counts.updates = visible.filter((mod) => mod.update !== "none").length;
  snapshot.counts.unchecked = visible.filter((mod) => mod.unchecked).length;
  snapshot.counts.deleted = snapshot.deleted.length;
}

export function mockAddInstalled(
  entries: { fileName: string; name: string; replaces?: string | null }[],
): void {
  const snapshot = mockLibrary;
  if (!snapshot) return;
  for (const entry of entries) {
    const replaced = entry.replaces
      ? snapshot.mods.find(
          (mod) => mod.fileName.toLowerCase() === entry.replaces?.toLowerCase(),
        )
      : undefined;
    if (replaced) {
      replaced.fileName = entry.fileName;
      replaced.name = entry.name;
      continue;
    }
    const existing = snapshot.mods.find(
      (mod) => mod.fileName.toLowerCase() === entry.fileName.toLowerCase(),
    );
    if (existing) {
      existing.name = entry.name;
      continue;
    }
    snapshot.mods.unshift({
      id: `mock-${Date.now() + snapshot.mods.length}`,
      name: entry.name,
      fileName: entry.fileName,
      version: null,
      sizeBytes: 88 * 1024 * 1024,
      enabled: true,
      subFolder: null,
      kind: "vehicle",
      previewUrl: null,
      compat: { modFor: null, game: null, text: null, differs: false },
      update: "none",
      updateVersion: null,
      unchecked: false,
      pending: "none",
      fromGameRepo: false,
      deleted: false,
      copies: 0,
      savedAt: null,
      problem: null,
      duplicateOf: null,
      group: [],
      groupConflict: false,
      role: null,
    });
  }
  recount(snapshot);
}

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
      updateVersion: index % 17 === 0 ? `2.${index % 5}` : null,
      unchecked: index % 11 === 0,
      pending: "none",
      fromGameRepo: index % 13 === 0,
      deleted: false,
      copies: 0,
      savedAt: null,
      problem: index === 4 ? "loose" : index === 9 ? "not_mod" : null,
      duplicateOf: index === 6 ? { id: "mock-1", name: TITLES[1], exact: true } : null,
      group: index === 1 ? ["mock-6"] : [],
      groupConflict: index === 1,
      role: index % 5 === 2 ? "addon" : index % 5 === 3 ? "map" : "vehicle",
    };
  });
  const enabled = mods.filter((mod) => mod.enabled).length;
  const updates = mods.filter((mod) => mod.update !== "none").length;
  const unchecked = mods.filter((mod) => mod.unchecked).length;
  return {
    mods,
    counts: {
      all: mods.length,
      enabled,
      updates,
      unchecked,
      deleted: 0,
    },
    folder: "C:\\Users\\Player\\AppData\\Local\\BeamNG\\BeamNG.drive\\current\\mods",
    folderError: null,
    gameVersion: "0.39.4",
    gameRunning: false,
    warning: null,
    manyEnabled: "Включено 3 333 мода. Игра может грузиться дольше",
    deleted: [],
  };
}

export async function libraryLoad(): Promise<LibrarySnapshot> {
  if (isTauri()) {
    return invoke<LibrarySnapshot>("library_load");
  }
  mockLibrary ??= mockSnapshot();
  return mockLibrary;
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
    checkingDeclined: false,
  };
}

export async function modPreview(id: string): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("mod_preview", { id });
}

export async function modDelete(id: string): Promise<DeletedTicket> {
  if (isTauri()) {
    return invoke<DeletedTicket>("mod_delete", { id });
  }
  const snapshot = await libraryLoad();
  const index = snapshot.mods.findIndex((mod) => mod.id === id);
  if (index < 0) throw new Error("Мод не найден.");
  const [card] = snapshot.mods.splice(index, 1);
  mockDeletedCards.set(id, card);
  recount(snapshot);
  return {
    id,
    fileName: card.fileName,
    backupId: `0000000001_20261021-120000-000_${card.fileName}`,
    wasInDisabled: false,
    name: card.name,
  };
}

export async function modUndelete(id: string): Promise<void> {
  if (isTauri()) {
    await invoke("mod_undelete", { id });
    return;
  }
  const snapshot = await libraryLoad();
  const card = mockDeletedCards.get(id);
  if (!card) return;
  mockDeletedCards.delete(id);
  snapshot.mods.push(card);
  recount(snapshot);
}

export async function modForgetDeleted(id: string): Promise<void> {
  if (isTauri()) {
    await invoke("mod_forget_deleted", { id });
    return;
  }
  const snapshot = await libraryLoad();
  const card = mockDeletedCards.get(id);
  if (!card) return;
  mockDeletedCards.delete(id);
  snapshot.deleted.push({
    ...card,
    deleted: true,
    enabled: false,
    copies: 1,
    savedAt: new Date().toISOString(),
  });
  recount(snapshot);
}

export async function modRestoreDeleted(fileName: string, backupId?: string): Promise<void> {
  if (isTauri()) {
    await invoke("mod_restore_deleted", { fileName, backupId: backupId ?? null });
    return;
  }
  const snapshot = await libraryLoad();
  const index = snapshot.deleted.findIndex((mod) => mod.fileName === fileName);
  if (index < 0) return;
  const [card] = snapshot.deleted.splice(index, 1);
  snapshot.mods.push({
    ...card,
    id: `mock-${Date.now()}`,
    deleted: false,
    enabled: true,
    copies: 0,
    savedAt: null,
  });
  recount(snapshot);
}

export async function modVersions(id: string): Promise<VersionRow[]> {
  if (isTauri()) return invoke<VersionRow[]>("mod_versions", { id });
  return [
    {
      backupId: "0000000002_20261020-120000-000_demo.zip",
      fileName: "demo_v1.zip",
      version: "1.0",
      savedAt: "2026-10-20T12:00:00+00:00",
      sizeBytes: 88 * 1024 * 1024,
    },
  ];
}

export async function modRestoreVersion(id: string, backupId: string): Promise<void> {
  if (isTauri()) await invoke("mod_restore_version", { id, backupId });
}

/** Дочитаны названия модов из их архивов: карточки стоит перерисовать. */
export async function listenLibraryTitles(handler: () => void): Promise<UnlistenFn> {
  if (!isTauri()) return () => {};
  return listen<number>("library://titles", () => handler());
}

/** Переупаковать архив, который игра не загрузит (прежний уходит в копии). */
export async function modFixPackaging(id: string): Promise<void> {
  if (isTauri()) await invoke("mod_fix_packaging", { id });
}

/** «Указать страницу»: следить за модом по ссылке, которую вставил человек. */
export async function modBindPage(id: string, url: string): Promise<void> {
  if (isTauri()) {
    await invoke("mod_bind_page", { modId: id, url });
    return;
  }
  if (!/^https?:\/\/\S+\/\S+/.test(url.trim())) {
    throw new Error("Это не ссылка. Скопируйте адрес страницы мода из браузера.");
  }
}

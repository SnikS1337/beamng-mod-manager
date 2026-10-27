import { invoke, isTauri } from "@tauri-apps/api/core";

import { mockAddInstalled } from "./library";

export type IncomingKind = "new" | "update" | "alreadyInstalled";
export type FindingSeverity = "info" | "warning" | "block";

export interface IncomingFindingDto {
  severity: FindingSeverity;
  code: string;
  message: string;
  entry: string | null;
}

export interface IncomingItemDto {
  zipPath: string;
  fileName: string;
  size: number;
  title: string;
  kind: IncomingKind;
  replaces: string | null;
  replacesEnabled: boolean;
  replacesSubFolder: string | null;
  locked: boolean;
  repacked: boolean;
  blocked: boolean;
  warnings: boolean;
  findings: IncomingFindingDto[];
  /** Похоже, такая машина уже стоит другим модом. */
  similarTo?: { fileName: string; title: string } | null;
}

export interface IncomingArchiveDto {
  source: string;
  problem: string | null;
  workDir: string;
  items: IncomingItemDto[];
}

export interface IncomingInstallRequest {
  zipPath: string;
  fileName: string;
  replaces: string | null;
  allowWarnings: boolean;
}

export interface IncomingInstallResult {
  success: boolean;
  message: string | null;
  warning: string | null;
  fileName: string;
  replaces: string | null;
  backupId: string | null;
  wasDisabled: boolean;
  sameName: boolean;
}

export interface IncomingTicket {
  fileName: string;
  replaces: string | null;
  backupId: string | null;
  wasDisabled: boolean;
  sameName: boolean;
}

export const ARCHIVE_EXTENSIONS = [".zip", ".rar", ".7z"] as const;

export function isArchivePath(path: string): boolean {
  const lower = path.toLowerCase();
  const name = lower.split(/[\\/]/).pop() ?? lower;
  if (name.startsWith(".")) return false;
  return ARCHIVE_EXTENSIONS.some((extension) => lower.endsWith(extension));
}

export function archiveBaseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export async function incomingInspect(paths: string[]): Promise<IncomingArchiveDto[]> {
  if (isTauri()) {
    return invoke<IncomingArchiveDto[]>("incoming_inspect", { paths });
  }
  return paths.map(mockArchive);
}

export async function incomingInstall(
  requests: IncomingInstallRequest[],
): Promise<IncomingInstallResult[]> {
  if (isTauri()) {
    return invoke<IncomingInstallResult[]>("incoming_install", { requests });
  }
  mockAddInstalled(
    requests.map((request) => ({
      fileName: request.fileName,
      name: titleOf(request.fileName),
      replaces: request.replaces,
    })),
  );
  return requests.map((request) => ({
    success: true,
    message: null,
    warning: null,
    fileName: request.fileName,
    replaces: request.replaces,
    backupId: request.replaces ? `0000000001_20261022-120000-000_${request.replaces}` : null,
    wasDisabled: false,
    sameName: request.replaces?.toLowerCase() === request.fileName.toLowerCase(),
  }));
}

export async function incomingUndo(ticket: IncomingTicket): Promise<void> {
  if (isTauri()) {
    await invoke("incoming_undo", { ticket });
  }
}

export async function incomingDiscard(workDir: string): Promise<void> {
  if (isTauri()) {
    await invoke("incoming_discard", { workDir });
  }
}

/** Удалить скачанный архив после успешной установки (если включено в настройках). */
export async function incomingRemoveSource(path: string): Promise<boolean> {
  if (isTauri()) return invoke<boolean>("incoming_remove_source", { path });
  return false;
}

function titleOf(fileName: string): string {
  return fileName.replace(/\.(zip|rar|7z)$/i, "").replace(/[_-]+/g, " ");
}

function cleanFileName(path: string): string {
  const raw = archiveBaseName(path).replace(/\.(zip|rar|7z)$/i, "");
  return `${raw.replace(/\s*\(\d+\)$/, "").trim() || "mod"}.zip`;
}

function mockArchive(path: string): IncomingArchiveDto {
  const fileName = cleanFileName(path);
  const title = titleOf(fileName);
  const lower = path.toLowerCase();
  const findings: IncomingFindingDto[] = [];
  let blocked = false;

  if (lower.includes("broken")) {
    blocked = true;
    findings.push({
      severity: "block",
      code: "broken",
      message: "Архив повреждён и не читается.",
      entry: null,
    });
  }
  if (lower.includes("exe")) {
    findings.push({
      severity: "warning",
      code: "executable",
      message: "Сомнительные файлы: программы или скрипты Windows. Игра их не запускает — ставьте, если доверяете источнику.",
      entry: "tools/setup.exe",
    });
  }

  const base = {
    zipPath: path,
    size: 88 * 1024 * 1024,
    replacesEnabled: true,
    replacesSubFolder: null,
    locked: false,
    repacked: false,
    blocked,
    warnings: findings.some((finding) => finding.severity === "warning"),
    findings,
  } satisfies Omit<IncomingItemDto, "fileName" | "title" | "kind" | "replaces">;

  const items: IncomingItemDto[] = [];
  if (lower.includes("bigpack")) {
    for (let index = 1; index <= 14; index += 1) {
      items.push({
        ...base,
        fileName: `bigpack_vehicle_${index}.zip`,
        title: `Машина из большого модпака № ${index}`,
        similarTo:
          index === 3 ? { fileName: "Audi_RS3_8Y_2022.zip", title: "Audi RS3 Sportback 8Y" } : null,
        kind: index % 4 === 0 ? "update" : "new",
        replaces: index % 4 === 0 ? `bmm_mod_0000${index % 10}.zip` : null,
      });
    }
  } else if (lower.includes("pack")) {
    items.push(
      { ...base, fileName: "track_pack_1.zip", title: "Track pack · трасса", kind: "new", replaces: null },
      {
        ...base,
        fileName: "demo_update.zip",
        title: "Demo update",
        kind: "update",
        replaces: "bmm_mod_00003.zip",
      },
      {
        ...base,
        fileName: "bmm_mod_00006.zip",
        title: "Уже установленный мод",
        kind: "alreadyInstalled",
        replaces: "bmm_mod_00006.zip",
      },
    );
  } else if (lower.includes("update")) {
    items.push({
      ...base,
      fileName,
      title,
      kind: "update",
      replaces: "bmm_mod_00003.zip",
    });
  } else {
    items.push({ ...base, fileName, title, kind: "new", replaces: null });
  }

  return {
    source: path,
    problem: null,
    workDir: `mock-incoming-${Date.now()}-${Math.floor(Math.random() * 100000)}`,
    items,
  };
}

/** «Обновить» открыл страницу мода: программа полчаса смотрит «Загрузки», чтобы подхватить архив. */
export async function expectDownload(): Promise<void> {
  if (isTauri()) await invoke("downloads_expect").catch(() => {});
}

import type { AppUpdateSnapshot } from "#lib/ipc/appUpdate.js";

export interface NoticeView {
  version: string;
  /** «Обновить» → «Скачиваем 40 %» → «Перезапустить». */
  stage: "available" | "downloading" | "ready" | "failed";
  progress: number;
  error: string | null;
}

/**
 * Что показать в плашке «Доступна новая версия». `dismissed` — версия, которую
 * человек отложил кнопкой «Позже»: до следующего запуска о ней не напоминаем.
 */
export function noticeFor(
  snapshot: AppUpdateSnapshot | null,
  dismissed: string | null,
): NoticeView | null {
  const manifest = snapshot?.state.manifest;
  if (!snapshot || !manifest) return null;
  const base = { version: manifest.version, progress: snapshot.state.progress, error: null };
  switch (snapshot.state.stage) {
    case "available":
      return dismissed === manifest.version ? null : { ...base, stage: "available" };
    case "downloading":
      return { ...base, stage: "downloading" };
    case "readyToRestart":
      // «Позже» прячет и готовое обновление: раньше плашка оставалась на месте.
      return dismissed === manifest.version ? null : { ...base, stage: "ready", progress: 1 };
    case "failed":
      // Сбой показываем, только если человек уже начал обновление из плашки.
      return dismissed === manifest.version
        ? null
        : { ...base, stage: "failed", error: snapshot.state.error };
    default:
      return null;
  }
}

/**
 * Пункты заметок к выпуску (раздел CHANGES.md): без маркеров списка и без
 * подзаголовков «### Новое» / «### Исправлено» — это не пункты.
 */
export function noteLines(notes: string | null | undefined): string[] {
  if (!notes) return [];
  return notes
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"))
    .map((line) => line.replace(/^[-•*]\s*/, ""))
    .filter(Boolean);
}

/** Пара строк из заметок: что в новой версии, без всего списка. */
export function notesPreview(notes: string | null | undefined, lines = 2): string[] {
  return noteLines(notes).slice(0, lines);
}

/** Все пункты «Что нового» для настроек. Старые выпуски писали их в одну строку через « - ». */
export function releaseNoteItems(notes: string | null | undefined): string[] {
  const text = notes?.trim();
  if (!text) return [];
  const lines = noteLines(text);
  if (lines.length === 1) {
    const inline = lines[0]!.split(/\s+-\s+/).map((part) => part.trim()).filter(Boolean);
    if (inline.length > 1) return inline;
  }
  return lines;
}

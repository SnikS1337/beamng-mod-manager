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

/** Пара строк из заметок: что в новой версии, без всего списка. */
export function notesPreview(notes: string | null | undefined, lines = 2): string[] {
  if (!notes) return [];
  return notes
    .split(/\r?\n/)
    .map((line) => line.trim().replace(/^[-•]\s*/, ""))
    .filter(Boolean)
    .slice(0, lines);
}

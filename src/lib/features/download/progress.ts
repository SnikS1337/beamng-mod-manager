import type { DownloadProgress } from "#lib/ipc/generated.js";

/** Сколько секунд без новых байт считаем, что загрузка стоит. */
export const STALL_AFTER_MS = 10_000;

export interface ProgressView {
  /** 0–100, если сайт сообщил размер файла. */
  percent: number | null;
  /** «42 % · 78,4 из 187 МБ · 3,2 МБ/с». */
  line: string;
  /** Загрузка стоит: новых байт нет дольше {@link STALL_AFTER_MS}. */
  stalled: boolean;
  /** Подсказка, если стоит: «Новых данных нет 15 с». */
  stallText: string | null;
}

const MB = 1024 * 1024;

function number(value: number, digits: number): string {
  return value.toFixed(digits).replace(".", ",");
}

/** Объём с точностью, которая видна глазом: до 10 МБ — сотые, до 100 — десятые. */
export function megabytes(bytes: number): string {
  const mb = bytes / MB;
  if (mb >= 1024) return `${number(mb / 1024, 2)} ГБ`;
  if (mb >= 100) return `${Math.round(mb)} МБ`;
  if (mb >= 10) return `${number(mb, 1)} МБ`;
  return `${number(mb, 2)} МБ`;
}

export function speedText(bytesPerSecond: number): string {
  if (bytesPerSecond < MB) return `${Math.max(0, Math.round(bytesPerSecond / 1024))} КБ/с`;
  return `${number(bytesPerSecond / MB, 1)} МБ/с`;
}

/**
 * Строка хода загрузки. `changedAt` — когда последний раз прибавились байты,
 * `now` — текущее время (мс).
 */
export function progressView(
  progress: DownloadProgress,
  changedAt: number,
  now: number,
): ProgressView {
  const total = progress.total && progress.total > 0 ? progress.total : null;
  const percent =
    total === null ? null : Math.min(100, Math.floor((progress.received / total) * 100));
  const idle = now - changedAt;
  const stalled = idle >= STALL_AFTER_MS && (total === null || progress.received < total);
  const parts: string[] = [];
  if (percent !== null) parts.push(`${percent} %`);
  parts.push(
    total === null
      ? `${megabytes(progress.received)} скачано`
      : `${megabytes(progress.received)} из ${megabytes(total)}`,
  );
  if (stalled) parts.push("0 КБ/с");
  else if (progress.speed !== null && progress.speed !== undefined) {
    parts.push(speedText(progress.speed));
  }
  return {
    percent,
    line: parts.join(" · "),
    stalled,
    stallText: stalled ? `Новых данных нет ${Math.floor(idle / 1000)} с` : null,
  };
}

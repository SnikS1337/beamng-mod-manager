import { ProgressBarStatus } from "@tauri-apps/api/window";

/**
 * Кнопка программы на панели задач Windows заполняется, пока идёт загрузка или
 * проверка: видно, даже когда окно свёрнуто.
 */
export type TaskbarStatus = "none" | "normal" | "indeterminate" | "paused";

export interface TaskbarState {
  status: TaskbarStatus;
  /** 0–100, только для `normal` и `paused`. */
  progress?: number;
}

export function taskbarState(
  download: { percent: number | null; stalled: boolean } | null,
  checking: boolean,
): TaskbarState {
  // Загрузка важнее проверки: у неё есть проценты, и ждут её дольше.
  if (download) {
    if (download.percent === null) return { status: download.stalled ? "paused" : "indeterminate" };
    return { status: download.stalled ? "paused" : "normal", progress: download.percent };
  }
  // У проверки этапы по сайтам, общего процента нет — полоса «бежит».
  if (checking) return { status: "indeterminate" };
  return { status: "none" };
}

/** Одно и то же состояние второй раз в Windows не шлём: события хода идут часто. */
export function sameTaskbar(left: TaskbarState | null, right: TaskbarState): boolean {
  return left !== null && left.status === right.status && left.progress === right.progress;
}

const STATUS: Record<TaskbarStatus, ProgressBarStatus> = {
  none: ProgressBarStatus.None,
  normal: ProgressBarStatus.Normal,
  indeterminate: ProgressBarStatus.Indeterminate,
  paused: ProgressBarStatus.Paused,
};

/**
 * Что отдать окну. Tauri ждёт целое 0–100 (в Rust это u64): дробь или выход за
 * пределы — ошибка, и полоса не обновилась бы.
 */
export function toWindowProgress(state: TaskbarState): { status: ProgressBarStatus; progress?: number } {
  if (state.progress === undefined) return { status: STATUS[state.status] };
  const progress = Math.min(100, Math.max(0, Math.round(state.progress)));
  return { status: STATUS[state.status], progress: Number.isFinite(progress) ? progress : 0 };
}

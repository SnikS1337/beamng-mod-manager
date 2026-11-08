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

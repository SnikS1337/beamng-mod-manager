import type { UpdateProgress } from "#lib/ipc/updates.js";

/** Как в `bmm_checks::runner`: сайт закончил — сам по себе, не дожидаясь остальных. */
export const STAGE_SITE_DONE = "сайт проверен";
export const STAGE_SITE_FAILED = "сайт не открылся";

/** Что сейчас с сайтом в идущей проверке. */
export interface SiteLive {
  state: "running" | "done" | "failed";
  /** «лента» или «карточки» — пока идёт. */
  stage: string;
  done: number;
  total: number;
}

/** Событие «сайт закончил»: общую строку хода оно не меняет. */
export function isSiteFinish(progress: UpdateProgress | null | undefined): boolean {
  return progress?.stage === STAGE_SITE_DONE || progress?.stage === STAGE_SITE_FAILED;
}

/**
 * Ход по сайтам из событий проверки. Пустой `source` — общее «готово», «страницы» —
 * поиск страниц, а не сайт: их не считаем. Закончивший сайт поздним событием
 * хода обратно в «идёт» не возвращается.
 */
export function trackSite(
  known: Record<string, SiteLive>,
  progress: UpdateProgress,
): Record<string, SiteLive> {
  const site = progress.source;
  if (!site || site === "страницы") return known;
  if (progress.stage === STAGE_SITE_DONE || progress.stage === STAGE_SITE_FAILED) {
    return {
      ...known,
      [site]: {
        state: progress.stage === STAGE_SITE_DONE ? "done" : "failed",
        stage: "",
        done: 0,
        total: 0,
      },
    };
  }
  const old = known[site];
  if (old && old.state !== "running") return known;
  return {
    ...known,
    [site]: { state: "running", stage: progress.stage, done: progress.done, total: progress.total },
  };
}

/** Строка сайта в панели, пока идёт проверка. */
export function siteLiveText(live: SiteLive | undefined): { tone: string; text: string } {
  if (!live) return { tone: "wait", text: "ждёт очереди" };
  if (live.state === "done") return { tone: "ok", text: "проверен" };
  // Причину подробно скажет итог проверки — здесь коротко, строка узкая.
  if (live.state === "failed") return { tone: "bad", text: "не проверен" };
  const what = live.stage === "карточки" ? "моды" : "лента";
  return {
    tone: "busy",
    text: live.total > 0 ? `${what} ${Math.min(live.done, live.total)} из ${live.total}` : what,
  };
}

/** Доля для полоски сайта: 0–1, `null` — сколько всего, неизвестно. */
export function siteLiveShare(live: SiteLive | undefined): number | null {
  if (!live) return 0;
  if (live.state !== "running") return 1;
  if (live.total <= 0) return null;
  return Math.min(1, live.done / live.total);
}

/** Общая строка хода проверки над списком. */
export function progressLine(
  shown: UpdateProgress | null,
  siteName: (id: string) => string,
): string {
  if (!shown) return "Проверяем сайты…";
  if (shown.source === "страницы") {
    return shown.total > 0 ? `Ищем страницы: ${shown.done} из ${shown.total}` : "Ищем страницы…";
  }
  const where = shown.source ? siteName(shown.source) : "проверка";
  // «Сайт закончил» — не «готово 1 из 1»: другие сайты ещё могут идти.
  if (shown.stage === STAGE_SITE_DONE) return `${where} · проверен`;
  if (shown.stage === STAGE_SITE_FAILED) return `${where} · не проверен`;
  const stage =
    shown.stage === "лента" || shown.stage === "карточки" || shown.stage === "страницы"
      ? shown.stage
      : "готово";
  return shown.total > 0 ? `${where} · ${stage} ${shown.done} из ${shown.total}` : `${where} · ${stage}`;
}

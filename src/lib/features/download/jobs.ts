import type { DownloadJob, DownloadProgress } from "#lib/ipc/download.js";
import { progressView, type ProgressView } from "./progress.js";
import { plural } from "#lib/features/garage/format.js";

/** Ход одной загрузки и когда в последний раз прибавились байты. */
export interface Tracked {
  progress: DownloadProgress;
  changedAt: number;
}

/**
 * Новый ход загрузки — в её строку, по файлу. Было: ход двух загрузок писался в одну
 * полоску, и она прыгала то к первому моду, то ко второму.
 */
export function track(
  known: Record<string, Tracked>,
  progress: DownloadProgress,
  now: number,
): Record<string, Tracked> {
  const old = known[progress.path];
  const changedAt = !old || old.progress.received !== progress.received ? now : old.changedAt;
  return { ...known, [progress.path]: { progress, changedAt } };
}

export function downloading(jobs: DownloadJob[]): DownloadJob[] {
  return jobs.filter((job) => job.state === "downloading");
}

export function jobName(job: DownloadJob): string {
  return job.request?.modName || job.fileName || "Мод";
}

/** Загрузка, которая идёт для открытой сейчас страницы: её ход — в главной полоске. */
export function jobForPage(jobs: DownloadJob[], pageUrl: string | null | undefined): DownloadJob | null {
  if (!pageUrl) return null;
  const mine = downloading(jobs).filter((job) => job.request?.pageUrl === pageUrl);
  return mine.at(-1) ?? null;
}

export function viewOf(
  job: DownloadJob,
  known: Record<string, Tracked>,
  now: number,
): ProgressView | null {
  const tracked = known[job.path];
  return tracked ? progressView(tracked.progress, tracked.changedAt, now) : null;
}

export interface JobRow {
  job: DownloadJob;
  /** «Качается», «Скачан — проверим следом», «Не скачался: …». */
  text: string;
}

/**
 * Строки списка «Загрузки» в окне: всё, что идёт или ждёт, кроме того, что и так
 * видно в шагах (загрузка открытой страницы и архив, который сейчас проверяем).
 */
export function jobRows(
  jobs: DownloadJob[],
  options: {
    currentPage: string | null | undefined;
    reviewing: string | null;
    queued: string[];
    seen: Set<string>;
  },
): JobRow[] {
  const current = jobForPage(jobs, options.currentPage);
  const rows: JobRow[] = [];
  for (const job of jobs) {
    if (job.path === options.reviewing) continue;
    if (job.state === "downloading") {
      if (current && job.path === current.path && !options.reviewing) continue;
      rows.push({ job, text: "Качается" });
    } else if (job.state === "done") {
      if (options.queued.includes(job.path)) rows.push({ job, text: "Скачан — проверим следом" });
    } else if (job.state === "failed" && !options.seen.has(job.path)) {
      rows.push({ job, text: `Не скачался: ${job.error ?? "сайт не отдал файл"}` });
    }
  }
  return rows;
}

/**
 * Файлы, которые начали качаться заново (или впервые). Тот же путь бывает снова:
 * архив после установки удаляется, и следующая загрузка ложится под тем же именем.
 * Прошлые отметки «уже проверен» и старый ход к новой загрузке не относятся.
 */
export function restarted(before: DownloadJob[], after: DownloadJob[]): string[] {
  const was = new Set(
    before.filter((job) => job.state === "downloading").map((job) => job.path),
  );
  return downloading(after)
    .map((job) => job.path)
    .filter((path) => !was.has(path));
}

/** Вопрос перед закрытием, или `null`, если терять нечего. */
export function closeQuestion(jobs: DownloadJob[], queued: DownloadJob[]): string | null {
  const running = downloading(jobs);
  const parts: string[] = [];
  if (running.length > 0) {
    parts.push(
      running.length === 1
        ? `Ещё качается «${jobName(running[0])}» — загрузка оборвётся, файл удалится.`
        : `Ещё качаются ${running.length} ${plural(running.length, "мод", "мода", "модов")} — загрузки оборвутся, файлы удалятся.`,
    );
  }
  if (queued.length > 0) {
    parts.push(
      queued.length === 1
        ? `«${jobName(queued[0])}» скачан, но не установлен — архив останется в папке загрузок.`
        : `Скачаны, но не установлены: ${queued.map((job) => `«${jobName(job)}»`).join(", ")} — архивы останутся в папке загрузок.`,
    );
  }
  return parts.length > 0 ? parts.join(" ") : null;
}

/** Плашка главного окна: что качается и сколько. `null` — ничего не качается. */
export function chipText(
  jobs: DownloadJob[],
  known: Record<string, Tracked>,
  now: number,
): string | null {
  const running = downloading(jobs);
  if (running.length === 0) return null;
  if (running.length === 1) {
    const view = viewOf(running[0], known, now);
    const percent = view?.percent;
    return `Скачивается «${jobName(running[0])}»${percent !== null && percent !== undefined ? ` · ${percent} %` : ""}`;
  }
  return `Скачиваются ${running.length} ${plural(running.length, "мод", "мода", "модов")}`;
}

/**
 * Ход обновления для карточки мода: идёт ли загрузка для этого мода и сколько
 * процентов. `percent: null` — сайт не сказал размер, полоска «бежит».
 */
export function modProgress(
  jobs: DownloadJob[],
  known: Record<string, Tracked>,
  modId: string,
): { percent: number | null } | null {
  const job = downloading(jobs)
    .filter((item) => item.request?.modId === modId)
    .at(-1);
  if (!job) return null;
  const progress = known[job.path]?.progress;
  const total = progress?.total && progress.total > 0 ? progress.total : null;
  if (!progress || total === null) return { percent: null };
  return { percent: Math.min(100, Math.floor((progress.received / total) * 100)) };
}

/**
 * Общий ход всех загрузок — для кнопки на панели задач. Размер известен не у всех —
 * процента нет (полоса «бежит»); все стоят — пауза.
 */
export function overallProgress(
  jobs: DownloadJob[],
  known: Record<string, Tracked>,
  now: number,
): { percent: number | null; stalled: boolean } | null {
  const active = downloading(jobs);
  if (active.length === 0) return null;
  let received = 0;
  let total = 0;
  let sized = true;
  let moving = false;
  for (const job of active) {
    const tracked = known[job.path];
    const size = tracked?.progress.total;
    if (!tracked || !size || size <= 0) sized = false;
    else {
      received += Math.min(tracked.progress.received, size);
      total += size;
    }
    if (!tracked || !viewOf(job, known, now)?.stalled) moving = true;
  }
  return {
    percent: sized && total > 0 ? Math.min(100, Math.floor((received / total) * 100)) : null,
    stalled: !moving,
  };
}

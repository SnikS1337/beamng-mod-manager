import type { DownloadJob } from "#lib/ipc/download.js";
import type { Tracked } from "./jobs.js";
import { STALL_AFTER_MS } from "./progress.js";

/** Как часто сверять часы, пока идут загрузки: заметно чаще порога «стоит». */
export const STALL_TICK_MS = Math.min(2000, STALL_AFTER_MS / 4);

/**
 * Загрузки окна скачивания, как их видит главное окно. Пишет `DownloadsHost`,
 * читают значок в шапке, карточки гаража (полоска на обложке) и кнопка на панели задач.
 */
class LiveDownloads {
  jobs = $state<DownloadJob[]>([]);
  tracked = $state<Record<string, Tracked>>({});
  now = $state(Date.now());
}

export const liveDownloads = new LiveDownloads();

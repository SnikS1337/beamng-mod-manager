import type { DownloadJob } from "#lib/ipc/download.js";
import type { Tracked } from "./jobs.js";

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

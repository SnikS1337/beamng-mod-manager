import type { DownloadRequest } from "#lib/ipc/download.js";
import type { IncomingArchiveDto, IncomingItemDto } from "#lib/ipc/incoming.js";
import { installable, preselected } from "#lib/features/incoming/selection.js";
import { plural } from "#lib/features/garage/format.js";

/** Скачанный в пачке архив: какой мод, что внутри и что отмечено к установке. */
export interface Collected {
  request: DownloadRequest;
  archive: IncomingArchiveDto;
  selected: boolean[];
}

export function collect(request: DownloadRequest, archive: IncomingArchiveDto): Collected {
  return { request, archive, selected: archive.items.map(preselected) };
}

/** Место текущей страницы в очереди: по адресу страницы. */
export function positionOf(batch: DownloadRequest[], current: DownloadRequest | null): number {
  if (!current) return -1;
  return batch.findIndex((request) => request.pageUrl === current.pageUrl);
}

/** Следующая страница, ещё не скачанная; `null` — очередь пройдена. */
export function nextPosition(
  batch: DownloadRequest[],
  position: number,
  collected: Collected[],
): number | null {
  const done = new Set(collected.map((entry) => entry.request.pageUrl));
  for (let index = position + 1; index < batch.length; index += 1) {
    if (!done.has(batch[index].pageUrl)) return index;
  }
  return null;
}

/** Всё, что отмечено к установке во всех скачанных архивах. */
export function pickedItems(collected: Collected[]): IncomingItemDto[] {
  return collected.flatMap((entry) =>
    entry.archive.problem
      ? []
      : entry.archive.items.filter(
          (item, index) => entry.selected[index] && installable(item),
        ),
  );
}

export function modsLabel(count: number): string {
  return `${count} ${plural(count, "мод", "мода", "модов")}`;
}

/** «Мод 2 из 5 · скачано 1». */
export function batchLine(position: number, total: number, collected: number): string {
  const where = position >= 0 ? `Мод ${position + 1} из ${total}` : `${modsLabel(total)} в очереди`;
  return collected > 0 ? `${where} · скачано ${collected}` : where;
}

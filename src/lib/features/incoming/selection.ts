import type { IncomingItemDto } from "#lib/ipc/incoming.js";

/** Можно ли поставить пункт вообще. */
export function installable(item: IncomingItemDto): boolean {
  return !item.blocked && !item.locked && item.kind !== "alreadyInstalled";
}

/**
 * Можно ли удалить исходный архив после установки: всё, что в нём есть, уже стоит.
 * Заблокированная строка (битый вложенный архив, «ещё N модов не разобраны») — не
 * поставлена, и вместе с исходником пропала бы навсегда.
 */
export function wholeArchiveInstalled(
  items: IncomingItemDto[],
  installedFiles: Set<string>,
): boolean {
  return items.every(
    (item) =>
      item.kind === "alreadyInstalled" ||
      (installable(item) && installedFiles.has(item.fileName.toLowerCase())),
  );
}

/** Программы и скрипты Windows в архиве: ставить или нет — решает пользователь. */
export function suspiciousFiles(item: IncomingItemDto): string[] {
  return item.findings
    .filter((finding) => finding.code === "executable" && finding.entry)
    .map((finding) => finding.entry as string);
}

/** Галочка по умолчанию: сомнительный мод пользователь отмечает сам. */
export function preselected(item: IncomingItemDto): boolean {
  return installable(item) && suspiciousFiles(item).length === 0;
}

/** «tools/a.bat, tools/b.ps1 и ещё 2» — коротко, какие файлы сомнительные. */
export function suspiciousText(files: string[], shown = 3): string {
  const names = files.slice(0, shown).join(", ");
  const rest = files.length - shown;
  return rest > 0 ? `${names} и ещё ${rest}` : names;
}

/**
 * Отмена установки: от последнего мода к первому, не останавливаясь на ошибке —
 * иначе одна неудача оставила бы остальные моды поставленными. Возвращает первую ошибку.
 */
export async function undoAll<T>(
  tickets: T[],
  undo: (ticket: T) => Promise<unknown>,
  describe: (reason: unknown) => string,
): Promise<string | null> {
  let problem: string | null = null;
  for (let index = tickets.length - 1; index >= 0; index -= 1) {
    try {
      await undo(tickets[index]);
    } catch (reason) {
      problem ??= describe(reason);
    }
  }
  return problem;
}

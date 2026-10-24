import type { IncomingItemDto } from "#lib/ipc/incoming.js";

/** Можно ли поставить пункт вообще. */
export function installable(item: IncomingItemDto): boolean {
  return !item.blocked && !item.locked && item.kind !== "alreadyInstalled";
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

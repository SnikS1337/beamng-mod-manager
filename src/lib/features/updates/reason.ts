import { formatSize } from "$lib/features/garage/format";
import type { UpdateFinding } from "$lib/ipc/updates";

/** «633 МБ → 647 МБ», если размер на сайте другой. */
export function sizeChange(finding: UpdateFinding): string {
  if (!finding.installedSize || !finding.latestSize || finding.installedSize === finding.latestSize) return "";
  return `${formatSize(finding.installedSize)} → ${formatSize(finding.latestSize)}`;
}

const PRERELEASE = /(alpha|beta|rc)/i;

/** Почему программа считает, что мод обновился. Только то, что знает наверняка. */
export function reason(finding: UpdateFinding): string {
  const size = sizeChange(finding);
  if (finding.status === "likely") return "Версия на сайте выше установленной.";
  const ours = finding.installedVersion;
  const theirs = finding.latestVersion;
  if (ours && theirs && PRERELEASE.test(ours) && !PRERELEASE.test(theirs)) {
    return `У вас сборка ${ours}, на сайте — ${theirs} без пометки. Возможно, вышла финальная версия, а возможно, это тот же файл.`;
  }
  if (!ours && size) {
    return `Версия у вас не указана, а размер на сайте другой: ${size}. Возможно, вышло обновление.`;
  }
  if (size) return `Версия та же, но размер на сайте изменился: ${size}. Возможно, автор перезалил файл.`;
  return "На сайте что-то изменилось, а версия не указана. Проверьте описание на странице.";
}

import { formatSize } from "#lib/features/garage/format.js";
import type { UpdateFinding } from "#lib/ipc/updates.js";

/** «633 МБ → 647 МБ», если размер на сайте другой. */
export function sizeChange(finding: UpdateFinding): string {
  if (!finding.installedSize || !finding.latestSize || finding.installedSize === finding.latestSize) return "";
  return `${formatSize(finding.installedSize)} → ${formatSize(finding.latestSize)}`;
}

const PRERELEASE = /(alpha|beta|rc)/i;

/** Почему программа считает, что мод обновился. Только то, что знает наверняка. */
export function reason(finding: UpdateFinding): string {
  const size = sizeChange(finding);
  const file = finding.fileName ? `«${finding.fileName}»` : "файл";
  if (finding.fileVerdict === "same_file") {
    return `Сайт пишет ${finding.latestVersion ?? "что мод изменился"}, но по ссылке лежит ${file} — тот же, что у вас: имя и размер совпали. Обновлением не считаем.`;
  }
  if (finding.fileVerdict === "label_older") {
    return `Сайт пишет ${finding.latestVersion ?? "новую версию"}, а файл по ссылке подписан ${finding.fileVersion ?? "прежней версией"}: ${file}. Возможно, автор не переименовал файл, а возможно, подпись сайта неверна.`;
  }
  if (finding.fileVerdict === "file_newer") {
    return `На странице версия не менялась, а по ссылке уже ${file} — версия ${finding.fileVersion ?? "новее"}. Похоже, автор обновил файл, не меняя подписи.`;
  }
  if (finding.authorVersion) {
    return `Сайт пишет ${finding.latestVersion ?? "версию новее"}, но у автора на BeamNG последняя — ${finding.authorVersion}, как у вас. Похоже, перезалив подписан неверно.`;
  }
  if (finding.status === "likely") {
    return finding.installedVersion ? "На сайте вышла новая версия." : "На сайте вышла версия новее вашего файла.";
  }
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

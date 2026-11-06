import { formatSize, plural } from "#lib/features/garage/format.js";
import type { UpdateFinding } from "#lib/ipc/updates.js";

/** «633 МБ → 647 МБ», если размер на сайте другой. */
export function sizeChange(finding: UpdateFinding): string {
  if (!finding.installedSize || !finding.latestSize || finding.installedSize === finding.latestSize) return "";
  return `${formatSize(finding.installedSize)} → ${formatSize(finding.latestSize)}`;
}

const PRERELEASE = /(alpha|beta|rc)/i;

/** Во сколько раз должен разойтись размер, чтобы страница казалась чужой. */
const OTHER_MOD_RATIO = 4;

/**
 * Похоже, страница о другом моде: версии ничего не решают (их нет или они те же), а
 * размер разошёлся в разы. Было: Lada 2101 — у вас 353 МБ («Rework»), на странице
 * 16 МБ — и это показывалось как «Возможно, вышло обновление».
 */
export function looksLikeOtherMod(finding: UpdateFinding): boolean {
  if (finding.status !== "possible" || finding.fileVerdict || finding.authorVersion) return false;
  const ours = finding.installedSize ?? 0;
  const theirs = finding.latestSize ?? 0;
  if (ours <= 0 || theirs <= 0) return false;
  const ratio = Math.max(ours, theirs) / Math.min(ours, theirs);
  return ratio >= OTHER_MOD_RATIO;
}

function times(finding: UpdateFinding): number {
  const ours = finding.installedSize ?? 1;
  const theirs = finding.latestSize ?? 1;
  return Math.round(Math.max(ours, theirs) / Math.min(ours, theirs));
}

/** Почему программа считает, что мод обновился. Только то, что знает наверняка. */
export function reason(finding: UpdateFinding): string {
  const text = why(finding);
  // Скачать по ссылке на сайте всё равно не выйдет — об этом лучше знать сразу.
  return finding.fileGone ? `${text} Файл по ссылке «Скачать» удалён с файлообменника.` : text;
}

function why(finding: UpdateFinding): string {
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
  if (looksLikeOtherMod(finding)) {
    const smaller = (finding.latestSize ?? 0) < (finding.installedSize ?? 0);
    const count = times(finding);
    return `Размер на сайте ${smaller ? "меньше" : "больше"} вашего в ${count} ${plural(count, "раз", "раза", "раз")} (${size}), а по версиям не понять. Похоже, страница о другом моде — если так, нажмите «Это другой мод».`;
  }
  if (ours && theirs && PRERELEASE.test(ours) && !PRERELEASE.test(theirs)) {
    return `У вас сборка ${ours}, на сайте — ${theirs} без пометки. Возможно, вышла финальная версия, а возможно, это тот же файл.`;
  }
  if (!ours && size) {
    const site = theirs ? `, а сайт пишет ${theirs}` : "";
    return `Версия у вас не указана${site}, и размер на сайте другой: ${size}. Возможно, вышло обновление.`;
  }
  if (size) return `Версия та же, но размер на сайте изменился: ${size}. Возможно, автор перезалил файл.`;
  return "На сайте что-то изменилось, а версия не указана. Проверьте описание на странице.";
}

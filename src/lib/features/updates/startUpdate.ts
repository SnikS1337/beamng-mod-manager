import type { DownloadRequest } from "#lib/ipc/download.js";
import type { UpdateFinding, UpdatesSnapshot } from "#lib/ipc/updates.js";
import { planUpdate } from "./fallback.js";

/**
 * «Обновить» одинаково везде: в разделе «Обновления», в панели мода и на карточке.
 * Своё окно загрузки (программа сама поставит скачанное); не открылось — страница
 * в браузере и наблюдение за «Загрузками». Сайт обновления не открывается — вопрос
 * про запасной сайт живёт в разделе «Обновления», туда и ведём.
 */

export function requestOf(finding: UpdateFinding): DownloadRequest {
  return {
    modName: finding.modName,
    installedVersion: finding.installedVersion,
    latestVersion: finding.latestVersion,
    pageUrl: finding.url,
    previewUrl: finding.previewUrl ?? null,
    role: finding.role ?? null,
    modId: finding.modId,
  };
}

export interface UpdateDeps {
  /** Окно загрузки программы. */
  openWindow: (request: DownloadRequest) => Promise<void>;
  /** Страница в браузере; `false` — не открылась. */
  openBrowser: (url: string) => Promise<boolean>;
  /** Следить за «Загрузками»: скачанный архив программа предложит поставить. */
  expectDownload: () => Promise<void>;
  toast: (text: string) => void;
}

export const BROWSER_HINT =
  "Скачайте архив на странице — программа заметит его в «Загрузках» и предложит установить";

/** Окно загрузки, а если его нет — браузер и «Загрузки». */
export async function openUpdate(finding: UpdateFinding, deps: UpdateDeps): Promise<void> {
  try {
    await deps.openWindow(requestOf(finding));
    return;
  } catch {
    // Ниже — прежний путь: системный браузер и «Загрузки».
  }
  if (!(await deps.openBrowser(finding.url))) return;
  await deps.expectDownload();
  deps.toast(BROWSER_HINT);
}

/** Находка для мода: сначала среди видимых, потом среди скрытых («не показывать»). */
export function findingOf(snapshot: UpdatesSnapshot, modId: string): UpdateFinding | null {
  return (
    snapshot.findings.find((item) => item.modId === modId) ??
    snapshot.hidden.find((item) => item.modId === modId) ??
    null
  );
}

/** Адрес раздела «Обновления», где сразу откроется вопрос про этот мод. */
export function updateRoute(modId: string): string {
  return `/updates?update=${encodeURIComponent(modId)}`;
}

export type GarageUpdate =
  /** Обновления уже нет (поставили, сняли страницу). */
  | { kind: "gone" }
  /** Окно загрузки (или браузер) открыто. */
  | { kind: "opened" }
  /** Сайт не открывается — спросить в «Обновлениях», откуда качать. */
  | { kind: "ask"; route: string };

/** «Обновить» из гаража: то же, что в разделе «Обновления». */
export async function updateFromGarage(
  modId: string,
  snapshot: () => Promise<UpdatesSnapshot>,
  deps: UpdateDeps,
): Promise<GarageUpdate> {
  const current = await snapshot();
  const finding = findingOf(current, modId);
  if (!finding) return { kind: "gone" };
  // Вопрос про запасной сайт показывается у видимых находок. Скрытую («не показывать»)
  // человек всё же решил обновить — открываем как есть.
  const visible = current.findings.some((item) => item.modId === modId);
  if (visible && planUpdate(finding).kind !== "open") return { kind: "ask", route: updateRoute(modId) };
  await openUpdate(finding, deps);
  return { kind: "opened" };
}

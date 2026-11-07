import { plural } from "#lib/features/garage/format.js";

/** Telegram-канал с модами: архивы отдаёт только приложение Telegram. */
export const TELEGRAM_SOURCE = "telegram";
const CHANNEL = "m0dsbeamng";

/** Пост канала: `https://t.me/m0dsbeamng/18360` (или `t.me/s/…`). */
export function isTelegramPost(raw: string): boolean {
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return false;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return false;
  const host = url.hostname.toLowerCase();
  if (host !== "t.me" && host !== "www.t.me" && host !== "telegram.me") return false;
  const parts = url.pathname.split("/").filter(Boolean);
  const rest = parts[0] === "s" ? parts.slice(1) : parts;
  return rest.length === 2 && rest[0].toLowerCase() === CHANNEL && /^[1-9]\d{0,9}$/.test(rest[1]);
}

/**
 * «Обновить все»: окно загрузки ведёт только по сайтам; моды из Telegram качаются
 * в самом Telegram, по одному.
 */
export function splitForBatch<T extends { pageUrl: string }>(
  requests: readonly T[],
): { window: T[]; telegram: T[] } {
  const window: T[] = [];
  const telegram: T[] = [];
  for (const request of requests) (isTelegramPost(request.pageUrl) ? telegram : window).push(request);
  return { window, telegram };
}

/**
 * Что делает «Обновить все»: один мод — как «Обновить» у него, несколько с сайтов —
 * окно загрузки по ним, из Telegram — первый открываем, если с сайтов нечего, а про
 * остальные говорим, сколько их.
 */
export function planUpdateAll<T extends { pageUrl: string }>(
  requests: readonly T[],
): { single: T | null; batch: T[]; telegramLeft: number } {
  const { window, telegram } = splitForBatch(requests);
  if (window.length === 0) {
    return { single: telegram[0] ?? null, batch: [], telegramLeft: Math.max(0, telegram.length - 1) };
  }
  if (window.length === 1) return { single: window[0], batch: [], telegramLeft: telegram.length };
  return { single: null, batch: window, telegramLeft: telegram.length };
}

export const TELEGRAM_OPENED =
  "Открыли пост в Telegram — скачайте архив, программа заметит его и предложит установить";

export function telegramLeftText(count: number): string {
  return `Ещё ${count} ${plural(count, "мод", "мода", "модов")} из Telegram: скачайте их там — кнопка «Обновить» у каждого`;
}

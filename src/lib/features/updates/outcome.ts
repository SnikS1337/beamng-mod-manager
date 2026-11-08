import { plural } from "#lib/features/garage/format.js";
import type { UpdatesEvent } from "#lib/ipc/updates.js";

/** Итог последней проверки — строка в панели «Проверка», видна и после тоста. */
export type CheckOutcome = {
  text: string;
  tone: "ok" | "new" | "warn";
  /** Когда закончилась, мс. */
  at: number;
};

type Done = Pick<UpdatesEvent, "cancelled" | "newCount" | "requests" | "sources">;

/**
 * Что сказать о законченной проверке одной строкой. Было: проверка без новых версий
 * ничего не оставляла на экране — тост уходил, и непонятно, проверялось ли вообще.
 * Остановленная проверка итога не меняет (`null`).
 */
export function outcomeOf(
  event: Done,
  options: { site?: string; background?: boolean; now?: number } = {},
): CheckOutcome | null {
  if (event.cancelled) return null;
  const at = options.now ?? Date.now();
  const who = options.site ? `${options.site}: ` : options.background ? "Фоновая проверка: " : "";
  const answered = event.sources.some((source) => source.status === "ok");
  if (event.newCount > 0) {
    const mods = `${event.newCount} ${plural(event.newCount, "мод", "мода", "модов")}`;
    return { text: `${who}нашли обновления — ${mods}`, tone: "new", at };
  }
  if (event.requests > 0 || answered) {
    return { text: `${who}новых обновлений нет`, tone: "ok", at };
  }
  if (event.sources.length === 0) return { text: `${who}проверять нечего`, tone: "warn", at };
  return { text: `${who}сайты не ответили`, tone: "warn", at };
}

/** Первая буква — заглавная (после имени сайта текст идёт со строчной). */
export function outcomeText(outcome: CheckOutcome): string {
  return outcome.text.charAt(0).toUpperCase() + outcome.text.slice(1);
}

import { describe, expect, it } from "vitest";
import type { UpdateSource } from "#lib/ipc/updates.js";
import { outcomeOf, outcomeText } from "./outcome";

const site = (status: UpdateSource["status"]): UpdateSource =>
  ({ id: "stmods", displayName: "STMods", status }) as UpdateSource;
const base = { cancelled: false, newCount: 0, requests: 5, sources: [site("ok")] };

describe("итог проверки в панели", () => {
  it("без новых версий — так и пишем, а не молчим", () => {
    // Было: проверка без новых версий не оставляла на экране ничего.
    expect(outcomeOf(base, { now: 1 })).toEqual({ text: "новых обновлений нет", tone: "ok", at: 1 });
    expect(outcomeText(outcomeOf(base)!)).toBe("Новых обновлений нет");
  });

  it("нашли — сколько", () => {
    expect(outcomeOf({ ...base, newCount: 1 })?.text).toBe("нашли обновления — 1 мод");
    expect(outcomeOf({ ...base, newCount: 3 })?.tone).toBe("new");
  });

  it("чья проверка: сайт или фоновая", () => {
    expect(outcomeText(outcomeOf(base, { site: "ModHub" })!)).toBe("ModHub: новых обновлений нет");
    expect(outcomeText(outcomeOf(base, { background: true })!)).toBe("Фоновая проверка: новых обновлений нет");
  });

  it("остановили — итог прежний; сайты не ответили — не «нет обновлений»", () => {
    expect(outcomeOf({ ...base, cancelled: true })).toBeNull();
    const silent = { ...base, requests: 0, sources: [site("unavailable")] };
    expect(outcomeOf(silent)).toMatchObject({ text: "сайты не ответили", tone: "warn" });
    expect(outcomeOf({ ...base, requests: 0, sources: [] })?.text).toBe("проверять нечего");
    // Лента ответила без отдельных запросов за карточками — проверка была.
    expect(outcomeOf({ ...base, requests: 0 })?.text).toBe("новых обновлений нет");
  });
});

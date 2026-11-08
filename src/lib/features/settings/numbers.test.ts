import { describe, expect, it } from "vitest";
import { NUMBER_RANGES, wholeIn, wholeNumbers } from "./numbers";

describe("числа в настройках", () => {
  it("дробное округляется, лишнее прижимается к краю", () => {
    expect(wholeIn(2.5, "keepBackups")).toBe(3);
    expect(wholeIn(999, "keepBackups")).toBe(20);
    expect(wholeIn(-4, "keepBackups")).toBe(1);
    expect(wholeIn(1e12, "maxCardChecksPerSource")).toBe(500);
    expect(wholeIn(0, "autoCheckEveryHours")).toBe(0);
    expect(wholeIn(0, "checkTtlHours")).toBe(1);
  });

  it("правит только числовые поля и не трогает исходник", () => {
    const draft = { keepBackups: 7.4, checkTtlHours: 300, mode: "dark", activeModsWarnAbove: Number.NaN };
    const fixed = wholeNumbers(draft);
    expect(fixed).toEqual({ keepBackups: 7, checkTtlHours: 168, mode: "dark", activeModsWarnAbove: Number.NaN });
    expect(draft.keepBackups).toBe(7.4);
  });

  it("пределы совпадают с подписями полей в настройках", async () => {
    // @ts-expect-error — node:fs в тестах есть, а типов для него в проекте нет.
    const { readFileSync } = await import("node:fs");
    const page: string = readFileSync("src/routes/(app)/settings/+page.svelte", "utf8");
    const fields: Record<string, string> = {
      "keep-backups": "keepBackups",
      "warn-above": "activeModsWarnAbove",
      ttl: "checkTtlHours",
      cards: "maxCardChecksPerSource",
      "auto-every": "autoCheckEveryHours",
    };
    for (const [id, field] of Object.entries(fields)) {
      const input = page.match(new RegExp(`id="${id}"[\\s\\S]*?min="(\\d+)"[\\s\\S]*?max="(\\d+)"`));
      expect(input, id).not.toBeNull();
      expect([Number(input![1]), Number(input![2])], id).toEqual(
        NUMBER_RANGES[field as keyof typeof NUMBER_RANGES],
      );
    }
  });
});

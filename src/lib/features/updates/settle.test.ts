import { describe, expect, it } from "vitest";
import { settle } from "./settle";

describe("итог вызова бэкенда", () => {
  it("успех несёт значение", async () => {
    expect(await settle(async () => 5)).toEqual({ ok: true, value: 5 });
  });

  it("отказ не превращается в успех", async () => {
    const result = await settle(async () => {
      throw new Error("Страница не найдена");
    });
    expect(result.ok).toBe(false);
    if (!result.ok) expect(String(result.error)).toContain("Страница не найдена");
  });
});

import { describe, expect, it } from "vitest";
import { STALL_TICK_MS } from "./live.svelte";
import { STALL_AFTER_MS } from "./progress";

describe("часы для «загрузка стоит»", () => {
  it("тикают заметно чаще порога, иначе «стоит» запаздывало бы", () => {
    expect(STALL_TICK_MS).toBeGreaterThan(0);
    expect(STALL_TICK_MS * 4).toBeLessThanOrEqual(STALL_AFTER_MS);
  });
});

import { afterEach, describe, expect, it, vi } from "vitest";
import { FLASH_MS, conflictFlash } from "./conflictFlash.svelte";

describe("вспышка у второй включённой копии", () => {
  afterEach(() => vi.useRealTimers());

  it("горит своё время и гаснет", async () => {
    vi.useFakeTimers();
    await conflictFlash.flash("w");
    expect(conflictFlash.id).toBe("w");
    vi.advanceTimersByTime(FLASH_MS);
    expect(conflictFlash.id).toBeNull();
  });

  it("повтор на той же карточке начинает заново и не гаснет раньше", async () => {
    vi.useFakeTimers();
    await conflictFlash.flash("w");
    vi.advanceTimersByTime(FLASH_MS - 100);
    await conflictFlash.flash("w");
    expect(conflictFlash.id).toBe("w");
    vi.advanceTimersByTime(200);
    expect(conflictFlash.id).toBe("w");
    vi.advanceTimersByTime(FLASH_MS);
    expect(conflictFlash.id).toBeNull();
  });

  it("другая карточка перехватывает вспышку", async () => {
    vi.useFakeTimers();
    await conflictFlash.flash("a");
    await conflictFlash.flash("b");
    expect(conflictFlash.id).toBe("b");
    vi.advanceTimersByTime(FLASH_MS);
    expect(conflictFlash.id).toBeNull();
  });
});

import { describe, expect, it, vi } from "vitest";

describe("значок «Обновления»", () => {
  it("пересчитывается, когда библиотека изменилась (поставили обновление)", async () => {
    const target = new EventTarget();
    vi.stubGlobal("window", target);
    const ipc = await import("#lib/ipc/updates.js");
    const { updatesBadge } = await import("./updatesBadge.svelte");
    updatesBadge.start();
    await vi.waitFor(() => expect(updatesBadge.available).toBeGreaterThan(0));
    const before = updatesBadge.available;
    const first = (await ipc.updatesSnapshot()).findings[0];
    await ipc.updatesDismiss(first.modId);
    target.dispatchEvent(new Event("bmm:library-changed"));
    await vi.waitFor(() => expect(updatesBadge.available).toBe(before - 1));
    await ipc.updatesRestore(first.modId);
    vi.unstubAllGlobals();
  });
});

describe("кнопка на панели задач мигает о найденном", () => {
  it("только про новое и только когда окно в стороне", async () => {
    const { shouldAskAttention } = await import("./updatesBadge.svelte");
    expect(shouldAskAttention({ phase: "auto", newCount: 2 }, false)).toBe(true);
    expect(shouldAskAttention({ phase: "auto", newCount: 2 }, true)).toBe(false);
    expect(shouldAskAttention({ phase: "auto", newCount: 0 }, false)).toBe(false);
    expect(shouldAskAttention({ phase: "done", newCount: 2 }, false)).toBe(true);
    expect(shouldAskAttention({ phase: "pages", newCount: 2 }, false)).toBe(false);
    expect(shouldAskAttention({ phase: "progress", newCount: 2 }, false)).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import { trapFocus, wrapTarget } from "./focus";

describe("wrapTarget", () => {
  const items = ["a", "b", "c"];

  it("с последнего вперёд — на первый, с первого назад — на последний", () => {
    expect(wrapTarget(items, "c", false)).toBe("a");
    expect(wrapTarget(items, "a", true)).toBe("c");
  });

  it("в середине не мешает обычному Tab", () => {
    expect(wrapTarget(items, "b", false)).toBeNull();
    expect(wrapTarget(items, "b", true)).toBeNull();
  });

  it("фокус снаружи окна возвращается внутрь", () => {
    expect(wrapTarget(items, "outside", false)).toBe("a");
    expect(wrapTarget(items, null, true)).toBe("c");
    expect(wrapTarget([], "a", false)).toBeNull();
  });
});

describe("trapFocus", () => {
  it("после закрытия окна возвращает фокус туда, откуда его открыли", async () => {
    let focused = 0;
    const opener = { isConnected: true, focus: () => (focused += 1) };
    const previous = (globalThis as { document?: unknown }).document;
    (globalThis as { document?: unknown }).document = { activeElement: opener };
    try {
      const node = {
        querySelector: () => null,
        querySelectorAll: () => [],
        addEventListener: () => {},
        removeEventListener: () => {},
      } as unknown as HTMLElement;
      const trap = trapFocus(node);
      trap.destroy();
      await Promise.resolve();
      await new Promise((done) => setTimeout(done, 0));
      expect(focused).toBe(1);
    } finally {
      (globalThis as { document?: unknown }).document = previous;
    }
  });
});


import { describe, expect, it } from "vitest";
import { wrapTarget } from "./focus";

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

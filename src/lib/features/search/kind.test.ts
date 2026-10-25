import { describe, expect, it } from "vitest";
import { searchKindLabel } from "./kind";

describe("searchKindLabel", () => {
  it("называет тип, если сайт его сказал", () => {
    expect(searchKindLabel({ kind: "vehicle" })).toBe("Машина");
    expect(searchKindLabel({ kind: "map" })).toBe("Карта");
    expect(searchKindLabel({ kind: "addon" })).toBe("Дополнение");
    expect(searchKindLabel({ kind: null })).toBeNull();
  });
});

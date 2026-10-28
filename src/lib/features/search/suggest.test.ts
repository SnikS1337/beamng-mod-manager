import { describe, expect, it } from "vitest";
import { suggest } from "./suggest";

describe("suggest", () => {
  it("не подсказывает на короткий запрос", () => {
    expect(suggest("b", ["BMW M2"], ["BMW M5"])).toEqual([]);
    expect(suggest("   ", ["BMW M2"], [])).toEqual([]);
  });

  it("ставит совпадение с начала выше совпадения в середине", () => {
    const got = suggest("m2", ["BMW M2 G87", "M2 Competition"], []);
    expect(got.map((item) => item.value)).toEqual(["M2 Competition", "BMW M2 G87"]);
  });

  it("не повторяет запрос и одинаковые названия из разных источников", () => {
    const got = suggest("bmw", ["BMW", "BMW M2"], ["bmw m2", "BMW E34"]);
    expect(got).toEqual([
      { value: "BMW M2", source: "history" },
      { value: "BMW E34", source: "library" },
    ]);
  });

  it("ищет без учёта регистра и показывает источник", () => {
    const got = suggest("нива", [], ["LADA Niva 4x4", "Нива 2121"]);
    expect(got).toEqual([{ value: "Нива 2121", source: "library" }]);
    expect(suggest("NIVA", ["Niva 2121"], [])).toEqual([
      { value: "Niva 2121", source: "history" },
    ]);
  });

  it("ограничивает число подсказок", () => {
    const many = Array.from({ length: 10 }, (_, index) => `BMW M${index}`);
    expect(suggest("bmw", many, many, 3)).toHaveLength(3);
  });
});

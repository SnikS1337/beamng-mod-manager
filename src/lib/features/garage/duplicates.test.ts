import { describe, expect, it } from "vitest";
import { duplicateBadge, duplicateExplain } from "./duplicates";

describe("повторы модов", () => {
  it("копия того же архива", () => {
    const copy = { id: "w", name: "weather fx", exact: true };
    expect(duplicateBadge(copy)).toBe("Копия");
    const explain = duplicateExplain(copy);
    expect(explain.title).toBe("Копия другого мода");
    expect(explain.text).toContain("«weather fx»");
    expect(explain.text).toContain("загрузит оба");
  });

  it("другая версия тех же машин", () => {
    const version = { id: "a", name: "Audi RS3", exact: false };
    expect(duplicateBadge(version)).toBe("Вторая версия");
    expect(duplicateExplain(version).text).toContain("Оставьте одну");
  });

  it("без названия другого мода", () => {
    expect(duplicateExplain({ id: "x", name: "", exact: true }).text).toContain("другой мод");
  });
});

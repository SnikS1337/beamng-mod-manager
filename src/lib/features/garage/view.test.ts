import { describe, expect, it } from "vitest";
import {
  loadViewMode,
  matchesType,
  saveViewMode,
  typeCounts,
  typeLabel,
  typeTag, revealScroll } from "./view";

describe("typeTag", () => {
  it("верит составу архива", () => {
    expect(typeTag({ role: "vehicle", kind: "unknown" })).toBe("vehicle");
    expect(typeTag({ role: "map", kind: "vehicle" })).toBe("map");
    expect(typeTag({ role: "addon", kind: "vehicle" })).toBe("addon");
    expect(typeTag({ role: "addon", kind: "script" })).toBe("script");
    expect(typeTag({ role: "addon", kind: "sound" })).toBe("sound");
  });

  it("пока архив не прочитан — угадывает по файлу", () => {
    expect(typeTag({ role: null, kind: "level" })).toBe("map");
    expect(typeTag({ role: null, kind: "unknown" })).toBe("mod");
    expect(typeLabel(typeTag({ role: null, kind: "unknown" }))).toBe("Мод");
  });
});

describe("фильтр по типу", () => {
  const mods = [
    { role: "vehicle" as const, kind: "vehicle" as const },
    { role: "addon" as const, kind: "script" as const },
    { role: "addon" as const, kind: "unknown" as const },
    { role: "map" as const, kind: "level" as const },
    { role: null, kind: "unknown" as const },
  ];

  it("скрипты и звуки — дополнения", () => {
    expect(typeCounts(mods)).toEqual({ vehicle: 1, map: 1, addon: 2 });
    expect(mods.filter((mod) => matchesType(mod, "addon"))).toHaveLength(2);
    expect(mods.filter((mod) => matchesType(mod, "all"))).toHaveLength(5);
  });
});

describe("вид гаража", () => {
  it("запоминается и не падает без хранилища", () => {
    const store = new Map<string, string>();
    const storage = {
      getItem: (key: string) => store.get(key) ?? null,
      setItem: (key: string, value: string) => void store.set(key, value),
    };
    expect(loadViewMode(storage)).toBe("grid");
    saveViewMode("list", storage);
    expect(loadViewMode(storage)).toBe("list");
    expect(loadViewMode(null)).toBe("grid");
    const broken = {
      getItem: () => {
        throw new Error("закрыто");
      },
    };
    expect(loadViewMode(broken)).toBe("grid");
  });
});

describe("прокрутка к строке списка", () => {
  it("прокручивает, только если строки не видно", () => {
    // Строки по 48: экран 0–480 показывает строки 0–9.
    expect(revealScroll(3, 48, 0, 480)).toBeNull();
    expect(revealScroll(10, 48, 0, 480)).toBe(48);
    expect(revealScroll(2, 48, 200, 480)).toBe(96);
  });
});

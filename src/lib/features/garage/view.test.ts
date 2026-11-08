import { describe, expect, it } from "vitest";
import {
  loadViewMode,
  matchesType,
  saveViewMode,
  typeCounts,
  typeLabel,
  typeTag, revealScroll, nextReveal, freshReveal } from "./view";
import { readFileSync } from "node:fs";

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

describe("показ карточки после стрелок — разовый", () => {
  it("каждая просьба новая, даже к той же карточке", () => {
    const first = nextReveal(null, "a");
    const again = nextReveal(first, "a");
    expect(first).toEqual({ id: "a", seq: 1 });
    expect(again.seq).toBe(2);
  });

  it("выполненную просьбу не повторяем", () => {
    // Раньше показ повторялся при прокрутке и вводе в поиск: прокрутка
    // возвращалась к карточке, фокус уходил на её переключатель.
    const request = nextReveal(null, "a");
    expect(freshReveal(request, 0)).toBe(true);
    expect(freshReveal(request, request.seq)).toBe(false);
    expect(freshReveal(null, 0)).toBe(false);
    expect(freshReveal(nextReveal(request, "b"), request.seq)).toBe(true);
  });

  it("сетка и список выполняют просьбу один раз и не следят за прокруткой", () => {
    for (const file of ["./ModGrid.svelte", "./ModList.svelte"]) {
      const text = readFileSync(new URL(file, import.meta.url), "utf8");
      expect(text, file).toContain("freshReveal(request, revealed)");
      expect(text, file).toContain("revealed = request.seq");
      expect(text, file).toMatch(/untrack\(\(\) => \{\s*const index = items\.findIndex/);
      expect(text, file).not.toContain("revealId");
    }
  });
});

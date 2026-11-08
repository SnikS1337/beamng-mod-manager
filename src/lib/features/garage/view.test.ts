import { describe, expect, it } from "vitest";
import {
  applyLoaded,
  loadViewMode,
  matchesType,
  saveViewMode,
  typeCounts,
  typeLabel,
  typeTag, revealScroll, nextReveal, freshReveal } from "./view";
// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
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
      const text: string = readFileSync(new URL(file, import.meta.url), "utf8");
      expect(text, file).toContain("freshReveal(request, revealed)");
      expect(text, file).toContain("revealed = request.seq");
      expect(text, file).toMatch(/untrack\(\(\) => \{\s*const index = items\.findIndex/);
      expect(text, file).not.toContain("revealId");
    }
  });
});

describe("перечитанная библиотека", () => {
  const recorder = () => {
    const log: string[] = [];
    return {
      log,
      change: () => log.push("change"),
      set: () => log.push("set"),
      run: async (update: () => void) => {
        log.push("run");
        update();
      },
    };
  };

  it("своё изменение — до данных и в том же переходе", async () => {
    const r = recorder();
    await applyLoaded({ isCurrent: () => true, change: r.change, set: r.set, smooth: true, run: r.run });
    expect(r.log).toEqual(["run", "change", "set"]);
  });

  it("без плавности — сразу, без перехода", async () => {
    const r = recorder();
    await applyLoaded({ isCurrent: () => true, change: r.change, set: r.set, smooth: false, run: r.run });
    expect(r.log).toEqual(["change", "set"]);
  });

  it("пока читали, началась новая загрузка — данные не кладём, а выбор снимаем", async () => {
    const r = recorder();
    await applyLoaded({ isCurrent: () => false, change: r.change, set: r.set, smooth: true, run: r.run });
    expect(r.log).toEqual(["change"]);
  });

  it("новая загрузка началась уже во время перехода — то же", async () => {
    const r = recorder();
    let current = true;
    await applyLoaded({
      isCurrent: () => current,
      change: r.change,
      set: r.set,
      smooth: true,
      run: async (update) => {
        current = false;
        update();
      },
    });
    expect(r.log).toEqual(["change"]);
  });

  it("без своего изменения — только данные", async () => {
    const r = recorder();
    await applyLoaded({ isCurrent: () => true, set: r.set, smooth: false, run: r.run });
    expect(r.log).toEqual(["set"]);
  });
});

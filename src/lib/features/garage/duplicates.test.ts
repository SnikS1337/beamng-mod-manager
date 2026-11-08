import { describe, expect, it } from "vitest";
import type { ModCardDto } from "#lib/ipc/library.js";
import {
  copiesBadge,
  duplicateExplain,
  groupConflictNow,
  groupMembers,
  isShown,
  visibleCards,
} from "./duplicates";

function card(id: string, patch: Partial<ModCardDto> = {}): ModCardDto {
  return {
    id,
    name: id,
    fileName: `${id}.zip`,
    version: null,
    sizeBytes: 1,
    enabled: true,
    subFolder: null,
    kind: "vehicle",
    previewUrl: null,
    compat: { modFor: null, game: null, text: null, differs: false },
    update: "none",
    updateVersion: null,
    unchecked: false,
    pending: "none",
    fromGameRepo: false,
    deleted: false,
    copies: 0,
    savedAt: null,
    problem: null,
    duplicateOf: null,
    group: [],
    groupConflict: false,
    hasPage: false,
    role: null,
    ...patch,
  };
}

describe("группы копий", () => {
  // weather_fx в четырёх файлах: в списке одна карточка «Копий: 3».
  const main = card("w", { group: ["w1", "tg", "ml"], groupConflict: true });
  const copies = [
    card("w1", { duplicateOf: { id: "w", name: "weather fx", exact: true } }),
    card("tg", { duplicateOf: { id: "w", name: "weather fx", exact: false } }),
    card("ml", { duplicateOf: { id: "w", name: "weather fx", exact: true } }),
  ];
  const all = [card("rs3"), main, ...copies];

  it("копии прячутся за основной карточкой", () => {
    expect(visibleCards(all).map((item) => item.id)).toEqual(["rs3", "w"]);
    expect(copiesBadge(main)).toBe("4 копии");
    expect(copiesBadge(card("rs3"))).toBeNull();
  });

  it("копию можно открыть из группы", () => {
    const shown = visibleCards(all);
    expect(isShown("tg", shown)).toBe(true);
    expect(isShown("nope", shown)).toBe(false);
    expect(groupMembers(main, all).map((item) => item.id)).toEqual(["w", "w1", "tg", "ml"]);
  });

  it("подписи копий", () => {
    expect(duplicateExplain({ id: "w", name: "weather fx", exact: true }).text).toContain("«weather fx»");
    expect(duplicateExplain({ id: "w", name: "", exact: false }).text).toContain("другой мод");
  });
});

describe("несколько включённых копий — сразу после переключения", () => {
  const all = () => [
    card("w", { group: ["w1", "w2"], enabled: true }),
    card("w1", { enabled: false, duplicateOf: { id: "w", name: "W", exact: true } }),
    card("w2", { enabled: false, duplicateOf: { id: "w", name: "W", exact: false } }),
    card("solo", { enabled: true }),
  ];

  it("включили копию при включённом основном — конфликт", () => {
    const mods = all();
    mods[1]!.enabled = true;
    expect(groupConflictNow(mods, "w1")).toEqual({ mainId: "w", conflict: true });
  });

  it("включили основной при включённой копии — тоже", () => {
    const mods = all();
    mods[0]!.enabled = false;
    mods[2]!.enabled = true;
    expect(groupConflictNow(mods, "w")).toEqual({ mainId: "w", conflict: false });
    mods[0]!.enabled = true;
    expect(groupConflictNow(mods, "w")).toEqual({ mainId: "w", conflict: true });
  });

  it("включена одна — конфликта нет; удалённая копия не считается", () => {
    const mods = all();
    expect(groupConflictNow(mods, "w")).toEqual({ mainId: "w", conflict: false });
    mods[1]!.enabled = true;
    mods[1]!.deleted = true;
    expect(groupConflictNow(mods, "w1")?.conflict).toBe(false);
  });

  it("мод без копий и неизвестный мод — не группа", () => {
    expect(groupConflictNow(all(), "solo")).toBeNull();
    expect(groupConflictNow(all(), "нет")).toBeNull();
  });
});

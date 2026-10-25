import { describe, expect, it } from "vitest";
import type { ModCardDto } from "#lib/ipc/library.js";
import { paletteItems } from "./paletteItems";

function mod(id: string, name: string, patch: Partial<ModCardDto> = {}): ModCardDto {
  return {
    id,
    name,
    fileName: `${id}.zip`,
    version: null,
    sizeBytes: 1,
    enabled: true,
    subFolder: null,
    kind: "vehicle",
    previewUrl: null,
    compat: { differs: false, modFor: null, game: null, text: null },
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
    role: "vehicle",
    ...patch,
  };
}

describe("paletteItems", () => {
  const mods = [
    mod("a", "LADA 2106 (Remaster)"),
    mod("b", "LADA 2102", { enabled: false }),
    mod("c", "LADA копия", { duplicateOf: { id: "a", name: "LADA", exact: true } }),
    mod("d", "LADA удалённая", { deleted: true }),
  ];

  it("без запроса — команды и разделы, без модов", () => {
    const items = paletteItems("", mods);
    expect(items.some((item) => item.group === "Ваши моды")).toBe(false);
    expect(items.some((item) => item.action.kind === "check")).toBe(true);
    expect(items.some((item) => item.action.kind === "go")).toBe(true);
  });

  it("находит свои моды без копий и удалённых и предлагает искать на сайтах", () => {
    const items = paletteItems("лада", mods);
    expect(items.filter((item) => item.group === "Ваши моды")).toHaveLength(0);
    const latin = paletteItems("lada", mods);
    const own = latin.filter((item) => item.group === "Ваши моды");
    expect(own.map((item) => item.label)).toEqual(["LADA 2106 (Remaster)", "LADA 2102"]);
    expect(own[1].hint).toContain("выключен");
    expect(own[0].tag?.text).toBe("Машина");
    expect(latin.find((item) => item.action.kind === "search")?.label).toBe("Найти «lada» на сайтах");
  });

  it("команды ищутся по словам", () => {
    const items = paletteItems("обнов", mods);
    expect(items.some((item) => item.action.kind === "check")).toBe(true);
    expect(items.some((item) => item.label === "Обновления")).toBe(true);
    expect(items.some((item) => item.label === "Настройки")).toBe(false);
  });
});

import { describe, expect, it } from "vitest";
import type { ModCardDto } from "#lib/ipc/library.js";
import {
  EMPTY,
  bulkPlan,
  clickSelect,
  keepVisible,
  selectAll,
  selectOnly,
  single,
  panelOf,
} from "./selection";

const order = ["a", "b", "c", "d", "e"];

describe("выбор щелчками", () => {
  it("обычный щелчок выбирает один, повторный — снимает", () => {
    const one = clickSelect(EMPTY, "b", order);
    expect(one.ids).toEqual(["b"]);
    expect(single(one)).toBe("b");
    expect(clickSelect(one, "b", order)).toEqual(EMPTY);
    expect(clickSelect(one, "d", order).ids).toEqual(["d"]);
  });

  it("Ctrl добавляет и убирает, порядок — как в списке", () => {
    let state = clickSelect(EMPTY, "d", order);
    state = clickSelect(state, "a", order, { ctrl: true });
    expect(state.ids).toEqual(["a", "d"]);
    expect(single(state)).toBeNull();
    state = clickSelect(state, "d", order, { ctrl: true });
    expect(state.ids).toEqual(["a"]);
    state = clickSelect(state, "a", order, { ctrl: true });
    expect(state.ids).toEqual([]);
  });

  it("Shift выбирает диапазон от последнего выбранного в обе стороны", () => {
    const start = clickSelect(EMPTY, "b", order);
    expect(clickSelect(start, "d", order, { shift: true }).ids).toEqual(["b", "c", "d"]);
    expect(clickSelect(start, "a", order, { shift: true }).ids).toEqual(["a", "b"]);
    // Второй Shift считает от той же точки, а не от конца прошлого диапазона.
    const range = clickSelect(start, "e", order, { shift: true });
    expect(clickSelect(range, "c", order, { shift: true }).ids).toEqual(["b", "c"]);
  });

  it("Ctrl+Shift добавляет диапазон к уже выбранному", () => {
    let state = clickSelect(EMPTY, "a", order);
    state = clickSelect(state, "d", order, { ctrl: true });
    state = clickSelect(state, "e", order, { ctrl: true, shift: true });
    expect(state.ids).toEqual(["a", "d", "e"]);
  });

  it("Shift без начала или с пропавшим началом — как обычный щелчок", () => {
    expect(clickSelect(EMPTY, "c", order, { shift: true }).ids).toEqual(["c"]);
    const stale = { ids: ["x"], anchor: "x" };
    expect(clickSelect(stale, "c", order, { shift: true }).ids).toEqual(["c"]);
  });

  it("выбрать все и выбрать один", () => {
    expect(selectAll(order).ids).toEqual(order);
    expect(selectAll([])).toEqual(EMPTY);
    expect(selectOnly("c").ids).toEqual(["c"]);
    expect(selectOnly(null)).toEqual(EMPTY);
  });
});

describe("фильтр меняет видимые", () => {
  it("скрытые выпадают из выбора, начало диапазона переезжает", () => {
    const state = { ids: ["a", "c", "e"], anchor: "a" };
    const after = keepVisible(state, ["c", "d", "e"]);
    expect(after.ids).toEqual(["c", "e"]);
    expect(after.anchor).toBe("c");
    expect(keepVisible(after, ["c", "d", "e"])).toBe(after);
    expect(keepVisible(state, [])).toEqual({ ids: [], anchor: null });
  });
});

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

describe("действия над выбранными", () => {
  const cards = [
    card("on"),
    card("off", { enabled: false }),
    card("repo", { fromGameRepo: true }),
    card("gone", { deleted: true, enabled: false }),
    card("other"),
  ];

  it("делит выбранные по тому, что с ними можно сделать", () => {
    const plan = bulkPlan(cards, ["on", "off", "repo", "gone"]);
    expect(plan.enable.map((item) => item.id)).toEqual(["off"]);
    expect(plan.disable.map((item) => item.id)).toEqual(["on", "repo"]);
    expect(plan.remove.map((item) => item.id)).toEqual(["on", "off"]);
  });

  it("невыбранные не трогает", () => {
    expect(bulkPlan(cards, [])).toEqual({ enable: [], disable: [], remove: [] });
    expect(bulkPlan(cards, ["nope"]).remove).toEqual([]);
  });
});

describe("panelOf", () => {
  it("панель справа по числу выбранных", () => {
    expect(panelOf(EMPTY)).toBe("none");
    expect(panelOf({ ids: ["a"], anchor: "a" })).toBe("one");
    expect(panelOf({ ids: ["a", "b"], anchor: "b" })).toBe("many");
  });

  it("смена мода в открытой панели — та же панель, без перестройки сетки", () => {
    expect(panelOf({ ids: ["a"], anchor: "a" })).toBe(panelOf({ ids: ["b"], anchor: "b" }));
  });
});

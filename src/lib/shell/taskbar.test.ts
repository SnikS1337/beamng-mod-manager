import { describe, expect, it } from "vitest";
import { sameTaskbar, taskbarState } from "./taskbar";

describe("кнопка на панели задач", () => {
  it("ничего не идёт — полосы нет", () => {
    expect(taskbarState(null, false)).toEqual({ status: "none" });
  });

  it("загрузка с размером — проценты, без размера — бегущая полоса", () => {
    expect(taskbarState({ percent: 42, stalled: false }, false)).toEqual({ status: "normal", progress: 42 });
    expect(taskbarState({ percent: null, stalled: false }, false)).toEqual({ status: "indeterminate" });
  });

  it("загрузка встала — пауза (жёлтая кнопка)", () => {
    expect(taskbarState({ percent: 42, stalled: true }, false)).toEqual({ status: "paused", progress: 42 });
    expect(taskbarState({ percent: null, stalled: true }, false)).toEqual({ status: "paused" });
  });

  it("проверка без загрузок — бегущая полоса; загрузка важнее проверки", () => {
    expect(taskbarState(null, true)).toEqual({ status: "indeterminate" });
    expect(taskbarState({ percent: 10, stalled: false }, true)).toEqual({ status: "normal", progress: 10 });
  });

  it("одинаковое состояние повторно не шлём", () => {
    expect(sameTaskbar(null, { status: "none" })).toBe(false);
    expect(sameTaskbar({ status: "normal", progress: 5 }, { status: "normal", progress: 5 })).toBe(true);
    expect(sameTaskbar({ status: "normal", progress: 5 }, { status: "normal", progress: 6 })).toBe(false);
  });
});

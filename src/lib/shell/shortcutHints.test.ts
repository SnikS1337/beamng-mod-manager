import { afterEach, describe, expect, it, vi } from "vitest";
import { CtrlHold, HOLD_MS, hintsFor } from "./shortcutHints.svelte";

const key = (name: string, extra: Partial<KeyboardEvent> = {}) =>
  ({ key: name, repeat: false, shiftKey: false, altKey: false, metaKey: false, ...extra }) as KeyboardEvent;

describe("подсказка горячих клавиш по Ctrl", () => {
  afterEach(() => vi.useRealTimers());

  it("держат Ctrl — через полсекунды видно, отпустили — пропало", () => {
    vi.useFakeTimers();
    const hold = new CtrlHold();
    hold.keydown(key("Control"));
    vi.advanceTimersByTime(HOLD_MS - 1);
    expect(hold.shown).toBe(false);
    vi.advanceTimersByTime(1);
    expect(hold.shown).toBe(true);
    hold.keyup(key("Control"));
    expect(hold.shown).toBe(false);
  });

  it("повтор keydown при удержании таймер не сдвигает", () => {
    vi.useFakeTimers();
    const hold = new CtrlHold();
    hold.keydown(key("Control"));
    vi.advanceTimersByTime(300);
    hold.keydown(key("Control", { repeat: true }));
    vi.advanceTimersByTime(HOLD_MS - 300);
    expect(hold.shown).toBe(true);
  });

  it("Ctrl+F и прочие сочетания подсказку не показывают", () => {
    vi.useFakeTimers();
    const hold = new CtrlHold();
    hold.keydown(key("Control"));
    hold.keydown(key("f", { ctrlKey: true }));
    vi.advanceTimersByTime(HOLD_MS * 2);
    expect(hold.shown).toBe(false);
  });

  it("Ctrl+Shift, Ctrl+Alt — не подсказка", () => {
    vi.useFakeTimers();
    const hold = new CtrlHold();
    hold.keydown(key("Control", { shiftKey: true }));
    hold.keydown(key("Control", { altKey: true }));
    vi.advanceTimersByTime(HOLD_MS * 2);
    expect(hold.shown).toBe(false);
  });

  it("клик или уход из окна прячет и отменяет", () => {
    vi.useFakeTimers();
    const hold = new CtrlHold();
    hold.keydown(key("Control"));
    hold.cancel();
    vi.advanceTimersByTime(HOLD_MS * 2);
    expect(hold.shown).toBe(false);
    hold.keydown(key("Control"));
    vi.advanceTimersByTime(HOLD_MS);
    expect(hold.shown).toBe(true);
    hold.cancel();
    expect(hold.shown).toBe(false);
  });
});

describe("какие сочетания показать", () => {
  it("гараж: поиск, выбор, отмена — только когда есть что отменить", () => {
    expect(hintsFor("/", false).map((hint) => hint.keys)).toEqual(["Ctrl F", "Ctrl A", "Ctrl + клик", "Ctrl K"]);
    expect(hintsFor("/", true).map((hint) => hint.keys)).toContain("Ctrl Z");
  });

  it("другие разделы — что там есть", () => {
    expect(hintsFor("/search", true).map((hint) => hint.keys)).toEqual(["Ctrl F", "Ctrl K"]);
    expect(hintsFor("/settings", true).map((hint) => hint.keys)).toEqual(["Ctrl K"]);
  });
});

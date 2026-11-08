import { describe, expect, it } from "vitest";
import { isShortcut, tabByKey } from "./keys";

const press = (patch: Partial<KeyboardEvent>) =>
  ({ ctrlKey: false, metaKey: false, code: "", key: "", ...patch }) as KeyboardEvent;

describe("isShortcut", () => {
  it("работает в русской раскладке", () => {
    expect(isShortcut(press({ ctrlKey: true, code: "KeyK", key: "л" }), "k")).toBe(true);
    expect(isShortcut(press({ ctrlKey: true, code: "KeyF", key: "а" }), "f")).toBe(true);
  });

  it("Ctrl+Shift+буква — не наша команда", () => {
    expect(
      isShortcut(press({ ctrlKey: true, shiftKey: true, code: "KeyZ", key: "Z" }), "z"),
    ).toBe(false);
  });

  it("и в английской, и без Ctrl не срабатывает", () => {
    expect(isShortcut(press({ ctrlKey: true, code: "KeyK", key: "k" }), "k")).toBe(true);
    expect(isShortcut(press({ code: "KeyK", key: "k" }), "k")).toBe(false);
    expect(isShortcut(press({ ctrlKey: true, code: "KeyL", key: "l" }), "k")).toBe(false);
  });
});

describe("вкладки по стрелкам", () => {
  const order = ["available", "hidden", "pages"] as const;

  it("стрелки ходят по кругу, Home/End — к краям", () => {
    expect(tabByKey(order, "available", "ArrowRight")).toBe("hidden");
    expect(tabByKey(order, "pages", "ArrowRight")).toBe("available");
    expect(tabByKey(order, "available", "ArrowLeft")).toBe("pages");
    expect(tabByKey(order, "hidden", "Home")).toBe("available");
    expect(tabByKey(order, "hidden", "End")).toBe("pages");
  });

  it("другие клавиши и пустой список — ничего", () => {
    expect(tabByKey(order, "hidden", "Enter")).toBeNull();
    expect(tabByKey(order, "hidden", "ArrowDown")).toBeNull();
    expect(tabByKey([], "x", "ArrowRight")).toBeNull();
  });
});

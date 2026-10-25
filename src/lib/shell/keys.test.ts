import { describe, expect, it } from "vitest";
import { isShortcut } from "./keys";

const press = (patch: Partial<KeyboardEvent>) =>
  ({ ctrlKey: false, metaKey: false, code: "", key: "", ...patch }) as KeyboardEvent;

describe("isShortcut", () => {
  it("работает в русской раскладке", () => {
    expect(isShortcut(press({ ctrlKey: true, code: "KeyK", key: "л" }), "k")).toBe(true);
    expect(isShortcut(press({ ctrlKey: true, code: "KeyF", key: "а" }), "f")).toBe(true);
  });

  it("и в английской, и без Ctrl не срабатывает", () => {
    expect(isShortcut(press({ ctrlKey: true, code: "KeyK", key: "k" }), "k")).toBe(true);
    expect(isShortcut(press({ code: "KeyK", key: "k" }), "k")).toBe(false);
    expect(isShortcut(press({ ctrlKey: true, code: "KeyL", key: "l" }), "k")).toBe(false);
  });
});

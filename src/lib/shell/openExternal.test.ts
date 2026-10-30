import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn(async () => {
    throw new Error("плагина нет");
  }),
}));
const shown: string[] = [];
vi.mock("./toasts.svelte", () => ({ toasts: { show: (text: string) => shown.push(text) } }));

const { openExternal } = await import("./openExternal");

describe("запасное открытие ссылки", () => {
  afterEach(() => {
    shown.length = 0;
    delete (globalThis as { window?: unknown }).window;
  });

  it("открытое окно — успех без ложного тоста", async () => {
    // С "noopener" window.open возвращал null и при успехе — программа писала
    // «Не удалось открыть ссылку», хотя страница открылась.
    const opened = { opener: {} as unknown };
    (globalThis as { window?: unknown }).window = { open: vi.fn(() => opened) };
    expect(await openExternal("https://example.org/mod")).toBe(true);
    expect(opened.opener).toBeNull();
    expect(shown).toEqual([]);
  });

  it("если окно не открылось — честно говорит об этом", async () => {
    (globalThis as { window?: unknown }).window = { open: vi.fn(() => null) };
    expect(await openExternal("https://example.org/mod")).toBe(false);
    expect(shown).toEqual(["Не удалось открыть ссылку в браузере"]);
  });
});

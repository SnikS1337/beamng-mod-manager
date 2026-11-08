import { afterEach, describe, expect, it, vi } from "vitest";
import {
  PREVIEW_MEMORY_LIMIT,
  clearPreviewMemory,
  forgetPreview,
  knownPreview,
  rememberPreview,
  resetPreviewMemory,
} from "./previewCache";

const local = (key: string) => `http://bmm.localhost/search-preview/${key}.jpg`;

describe("картинки выдачи без повторной загрузки", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
    resetPreviewMemory();
  });

  it("скачанная картинка известна сразу — и после перезапуска", () => {
    vi.useFakeTimers();
    const saved: Record<string, string> = {};
    vi.stubGlobal("localStorage", {
      getItem: (key: string) => saved[key] ?? null,
      setItem: (key: string, value: string) => (saved[key] = value),
    });
    rememberPreview("https://stmods.org/a.jpg", local("a"));
    expect(knownPreview("https://stmods.org/a.jpg")).toBe(local("a"));
    vi.advanceTimersByTime(1000);
    resetPreviewMemory();
    expect(knownPreview("https://stmods.org/a.jpg"), "после перезапуска").toBe(local("a"));
  });

  it("чужие адреса не запоминаются и не подставляются из хранилища", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => JSON.stringify({ x: "javascript:alert(1)", y: "https://evil.example/a.jpg", z: local("z") }),
      setItem: () => {},
    });
    expect(knownPreview("x")).toBeUndefined();
    expect(knownPreview("y")).toBeUndefined();
    expect(knownPreview("z")).toBe(local("z"));
    rememberPreview("w", "https://evil.example/w.jpg");
    expect(knownPreview("w")).toBeUndefined();
  });

  it("пропавший файл забывается — картинку запросят снова", () => {
    rememberPreview("a", local("a"));
    rememberPreview("b", local("a"));
    forgetPreview(local("a"));
    expect(knownPreview("a")).toBeUndefined();
    expect(knownPreview("b")).toBeUndefined();
  });

  it("помнит не больше предела, старые уходят первыми", () => {
    for (let index = 0; index <= PREVIEW_MEMORY_LIMIT; index += 1) rememberPreview(`r${index}`, local(`k${index}`));
    expect(knownPreview("r0")).toBeUndefined();
    expect(knownPreview(`r${PREVIEW_MEMORY_LIMIT}`)).toBe(local(`k${PREVIEW_MEMORY_LIMIT}`));
  });

  it("без хранилища (или с испорченным) работает в памяти", () => {
    vi.stubGlobal("localStorage", { getItem: () => "{сломано", setItem: () => { throw new Error("full"); } });
    rememberPreview("a", local("a"));
    expect(knownPreview("a")).toBe(local("a"));
  });

  it("после очистки картинок ничего не помнит — ни в памяти, ни в хранилище", () => {
    const removed: string[] = [];
    vi.stubGlobal("localStorage", { getItem: () => null, setItem: () => {}, removeItem: (key: string) => removed.push(key) });
    rememberPreview("a", local("a"));
    clearPreviewMemory();
    expect(knownPreview("a")).toBeUndefined();
    expect(removed).toEqual(["bmm.searchPreviews"]);
  });
});

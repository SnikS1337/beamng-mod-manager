// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it, vi } from "vitest";

vi.mock("#lib/ipc/settings.js", () => ({ settingsLoad: vi.fn(), settingsSave: vi.fn() }));

describe("окно под страницей — цвета темы", () => {
  it("берёт только #rrggbb", async () => {
    const { windowColor } = await import("./appearance.svelte");
    expect(windowColor(" #0E0F11 ")).toBe("#0e0f11");
    expect(windowColor("#fff")).toBeNull();
    expect(windowColor("rgb(1 2 3)")).toBeNull();
    expect(windowColor("")).toBeNull();
  });

  it("фон обеих тем — в таком виде, а окно при запуске — тёмное, как тема по умолчанию", () => {
    // Было: после «Свернуть» — «Развернуть» пару секунд виден чёрный прямоугольник.
    const css = readFileSync("src/app.css", "utf8");
    const backgrounds = [...css.matchAll(/^\s+--background: (#[0-9a-f]{6});/gm)].map((match) => match[1]);
    expect(backgrounds.length).toBeGreaterThanOrEqual(2);
    const config = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));
    const dark = css.match(/\.dark \{\s*--background: (#[0-9a-f]{6});/)?.[1];
    expect(config.app.windows[0].backgroundColor).toBe(dark);
  });
});

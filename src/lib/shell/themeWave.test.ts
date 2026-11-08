// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { WAVE_FEATHER, waveGeometry } from "./themeWave";

const size = { width: 1200, height: 800 };

describe("волна новой темы", () => {
  it("идёт от нажатия и докрывает самый дальний угол", () => {
    const wave = waveGeometry({ x: 100, y: 100 }, size);
    expect([wave.x, wave.y]).toEqual([100, 100]);
    expect(wave.radius).toBe(Math.ceil(Math.hypot(1100, 700) + WAVE_FEATHER));
  });

  it("с клавиатуры (координат нет) — из центра окна", () => {
    expect(waveGeometry(null, size)).toMatchObject({ x: 600, y: 400 });
    expect(waveGeometry({ x: 0, y: 0 }, size)).toMatchObject({ x: 600, y: 400 });
  });

  it("в переходе темы двигается всё окно целиком, а не отдельные слои", () => {
    // Было: у страницы и меню свои слои — они просто проявлялись, а кругом
    // расходилась только рамка окна.
    const css = readFileSync("src/app.css", "utf8").replace(/\r\n/g, "\n");
    expect(css).toMatch(/html\.flow-theme \*,?[^{]*\{\s*view-transition-name: none !important;/);
    expect(css).toMatch(/@property --theme-wave/);
  });
});

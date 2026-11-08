// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const read = (path: string): string => readFileSync(new URL(path, import.meta.url), "utf8").replace(/\r\n/g, "\n");

describe("живые галочки", () => {
  it("галочка в «Быстро» стоит готовой, а не пустой", () => {
    const check = read("./DrawCheck.svelte");
    // Общее правило «Быстро» проматывает анимацию до конца: без `both` штрих
    // остался бы с исходным смещением — пустая галочка.
    expect(check).toMatch(/animation: check-draw [^;]* both;/);
    expect(check).toMatch(/animation: check-ring [^;]* both;/);
    expect(check).toMatch(/stroke-dasharray: 1;/);
    expect(check).toContain('pathLength="1"');
    // `.ring` — утилита Tailwind (обводка): с таким классом вокруг галочки
    // появлялась квадратная рамка.
    expect(check).not.toMatch(/class:ring\b|class="[^"]*\bring\b/);
  });

  it("отметка «Установлен» в «Быстро» видна, пока жива", () => {
    const css = read("../../app.css");
    // Конец анимации — прозрачный: в «Быстро» её снимаем целиком, иначе отметка
    // сразу исчезла бы.
    expect(css).toMatch(/\.no-motion \.installed-life \{\s*animation-name: none;/);
    expect(css).toMatch(/\.installed-life \{\s*animation: installed-life 1800ms both;/);
  });

  it("время отметки совпадает с таймером", async () => {
    const { INSTALLED_MS } = await import("../features/garage/conflictFlash.svelte");
    const css = read("../../app.css");
    expect(css).toContain(`animation: installed-life ${INSTALLED_MS}ms both;`);
  });
});

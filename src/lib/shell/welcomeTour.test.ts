// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/** Правило CSS по селектору из блока <style> компонента. */
function rule(text: string, selector: string): string {
  const start = text.indexOf(`${selector} {`);
  return start < 0 ? "" : text.slice(start, text.indexOf("}", start));
}

describe("приветствие при крупном масштабе", () => {
  it("окно не выше экрана и прокручивается — «Начать» всегда достижима", () => {
    // При 200 % окно 1000×680 — это 500×340: кнопка оказывалась за нижним краем.
    const text: string = readFileSync(new URL("./WelcomeTour.svelte", import.meta.url), "utf8");
    const tour = rule(text, ".tour");
    expect(tour).toMatch(/max-height:\s*calc\(100dvh - 48px\)/);
    expect(tour).toMatch(/overflow-y:\s*auto/);
    expect(rule(text, ".backdrop")).toMatch(/overflow-y:\s*auto/);
  });
});

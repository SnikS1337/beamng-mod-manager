// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readdirSync, readFileSync, statSync } from "node:fs";
// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Свои классы, совпадающие с утилитами Tailwind, которые что-то рисуют. Tailwind 4
 * находит такое слово в исходниках и добавляет свою утилиту: `.ring` давала рамку
 * вокруг галочки «Установлен» и кольца «Отменить».
 */
const RESERVED = ["ring", "shadow", "outline", "border", "blur", "filter", "transform", "transition", "invert", "grayscale", "sepia", "rounded"];

function svelteFiles(dir: string): string[] {
  const out: string[] = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) out.push(...svelteFiles(path));
    else if (name.endsWith(".svelte")) out.push(path);
  }
  return out;
}

/** Классы элемента: из `class="…"` (без выражений) и `class:имя`. */
export function classTokens(source: string): string[] {
  const tokens: string[] = [];
  for (const match of source.matchAll(/\sclass="([^"]*)"/g)) {
    const text = match[1]!.replace(/\{[^}]*\}/g, " ");
    tokens.push(...text.split(/\s+/).filter(Boolean));
  }
  for (const match of source.matchAll(/\sclass:([\w-]+)/g)) tokens.push(match[1]!);
  return tokens;
}

describe("классы не совпадают с утилитами Tailwind", () => {
  it("разбор классов", () => {
    expect(classTokens(`<a class="chip filter-toggle {x}" class:with-ring={on} class:ring>`)).toEqual([
      "chip",
      "filter-toggle",
      "with-ring",
      "ring",
    ]);
  });

  it("ни в одном компоненте", () => {
    const wrong: string[] = [];
    // Тесты запускаются из корня проекта (как и проверка толщины иконок).
    for (const file of svelteFiles("src")) {
      const text = readFileSync(file, "utf8");
      for (const token of classTokens(text)) if (RESERVED.includes(token)) wrong.push(`${file}: ${token}`);
    }
    expect(wrong).toEqual([]);
  });
});

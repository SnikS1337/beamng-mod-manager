// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readdirSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * На Windows регистр в именах файлов не различается: `shortcutHints.svelte` (модуль
 * `shortcutHints.svelte.ts`) и `ShortcutHints.svelte` (компонент) — для Vite один
 * файл, и компонент импортировал сам себя. На Linux это не видно, поэтому проверяем
 * явно: импортируемые имена в одной папке не совпадают без учёта регистра.
 */
function importNames(file: string): string[] {
  // Модуль `x.svelte.ts` импортируют как `x.svelte`, `x.ts` — как `x`.
  if (file.endsWith(".svelte.ts")) return [file.slice(0, -3)];
  if (file.endsWith(".ts")) return [file.slice(0, -3), file];
  return [file];
}

function walk(dir: string, clashes: string[]): void {
  const seen = new Map<string, string>();
  for (const name of readdirSync(dir) as string[]) {
    const path = `${dir}/${name}`;
    if (statSync(path).isDirectory()) {
      walk(path, clashes);
      continue;
    }
    for (const imported of importNames(name)) {
      const key = imported.toLowerCase();
      const other = seen.get(key);
      if (other && other !== name) clashes.push(`${dir}: ${other} ↔ ${name}`);
      else seen.set(key, name);
    }
  }
}

describe("имена файлов на Windows", () => {
  it("в одной папке нет имён, которые совпадают без учёта регистра", () => {
    const clashes: string[] = [];
    walk("src", clashes);
    expect(clashes).toEqual([]);
  });
});

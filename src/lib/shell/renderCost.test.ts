// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";

function sources(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir) as string[]) {
    const path = `${dir}/${name}`;
    if (statSync(path).isDirectory()) sources(path, out);
    else if (/\.(svelte|css)$/.test(name)) out.push(path);
  }
  return out;
}

/**
 * Дорогое для перерисовки на всё окно. На слабой видеокарте (и в виртуальной машине)
 * после «Развернуть» фон с этим рисовался секундами — окно было чёрным.
 */
const COSTLY = [/mix-blend-mode/, /feTurbulence/, /backdrop-filter/];

describe("дешёвая перерисовка окна", () => {
  it("без смешивания слоёв, SVG-шума и размытия под элементами", () => {
    const wrong: string[] = [];
    for (const file of sources("src")) {
      const text = readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
      for (const pattern of COSTLY) if (pattern.test(text)) wrong.push(`${file}: ${pattern}`);
    }
    expect(wrong).toEqual([]);
  });
});

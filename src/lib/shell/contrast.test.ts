// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/** Токены светлой темы из app.css (первый блок `:root`). */
function lightTokens(): Record<string, string> {
  const css: string = readFileSync(new URL("../../app.css", import.meta.url), "utf8");
  const start = css.indexOf("/* Светлая тема.");
  const block = css.slice(start, css.indexOf("}", start));
  const tokens: Record<string, string> = {};
  for (const match of block.matchAll(/(--[\w-]+):\s*(#[0-9a-fA-F]{6});/g)) {
    tokens[match[1]!] = match[2]!;
  }
  return tokens;
}

function luminance(hex: string): number {
  const channel = (offset: number) => {
    const value = parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (light + 0.05) / (dark + 0.05);
}

describe("светлая тема", () => {
  const tokens = lightTokens();
  const backgrounds = ["--background", "--surface", "--surface-2", "--surface-3"];
  const texts = [
    "--foreground",
    "--muted-foreground",
    "--faint-foreground",
    "--accent-brand-text",
    "--ok",
    "--warning",
    "--danger",
    "--kind-vehicle",
    "--kind-map",
    "--kind-addon",
    "--kind-script",
  ];

  it.each(texts)("%s читается на всех фонах (AA, 4.5:1)", (text) => {
    for (const background of backgrounds) {
      expect(tokens[text], text).toBeDefined();
      expect(tokens[background], background).toBeDefined();
      expect(contrast(tokens[text]!, tokens[background]!)).toBeGreaterThanOrEqual(4.5);
    }
  });
});

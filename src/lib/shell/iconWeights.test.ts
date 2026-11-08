// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * Одно правило толщины иконок Phosphor (docs/DECISIONS.md):
 * - заглушки вместо картинки (машина, коробка, пазл…) — thin, любого размера;
 * - мелкие значки в метках и галочки (до 13 px) и крестик «закрыть» — bold;
 * - обычные (14–24 px: кнопки, меню, поля, навигация) — regular, активное — fill;
 * - крупнее — thin.
 * Исключения перечислены явно и с причиной.
 */
const GLYPHS = new Set(["Car", "Package", "PuzzlePiece", "MapTrifold", "SpeakerHigh"]);
/** Заглушки вместо картинки, где тип выбирается по моду (`kindIcon`). */
const PLACEHOLDERS: { file: string; tag: string }[] = [
  { file: "ModList.svelte", tag: "<KindIcon size={16}" },
];
const EXCEPTIONS: { file: string; tag: string; why: string }[] = [
  { file: "Titlebar.svelte", tag: "", why: "кнопки окна — как у Windows, тонкие" },
  { file: "search/+page.svelte", tag: "<MagnifyingGlass size={26}", why: "эмблема в кружке стартового экрана" },
  { file: "ModCard.svelte", tag: "<Icon size={40}", why: "заглушка типа мода" },
  { file: "ModCard.svelte", tag: "<Icon size={34}", why: "заглушка дополнения" },
];

function files(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir) as string[]) {
    const path = `${dir}/${name}`;
    if (statSync(path).isDirectory()) {
      if (!path.includes("components/ui")) files(path, out);
    } else if (name.endsWith(".svelte")) out.push(path);
  }
  return out;
}

function expected(name: string, size: number): string[] {
  if (GLYPHS.has(name)) return ["thin"];
  if (name === "X") return ["bold"];
  if (size <= 13) return ["bold"];
  if (size <= 24) return ["regular", "fill"];
  return ["thin"];
}

describe("толщина иконок", () => {
  it("по одному правилу", () => {
    const wrong: string[] = [];
    for (const path of files("src")) {
      const text: string = readFileSync(path, "utf8");
      for (const match of text.matchAll(/<([A-Z][A-Za-z]*) size=\{(\d+)\}([^>]*)\/>/g)) {
        const [tag, name, size, rest] = match as unknown as [string, string, string, string];
        if (!/phosphor-svelte/.test(text) && !/Icon/.test(name)) continue;
        // Свои компоненты (DrawCheck и т. п.) — не Phosphor, у них нет толщины.
        if (new RegExp(`import ${name} from "[^"]*\\.svelte"`).test(text)) continue;
        if (EXCEPTIONS.some((item) => path.endsWith(item.file) && tag.startsWith(item.tag))) continue;
        if (PLACEHOLDERS.some((item) => path.endsWith(item.file) && tag.startsWith(item.tag))) {
          if (!/weight="thin"/.test(rest)) wrong.push(`${path}: ${tag}`);
          continue;
        }
        const weight = rest.match(/weight="([a-z]+)"/)?.[1];
        // Выражение (активное состояние) — только regular/fill.
        if (/weight=\{/.test(rest)) {
          const values = [...rest.matchAll(/[?:]\s*"([a-z]+)"/g)].map((item) => item[1]);
          if (!values.every((value) => value === "regular" || value === "fill")) wrong.push(`${path}: ${tag}`);
          continue;
        }
        if (!expected(name, Number(size)).includes(weight ?? "regular")) wrong.push(`${path}: ${tag}`);
      }
    }
    expect(wrong).toEqual([]);
  });
});

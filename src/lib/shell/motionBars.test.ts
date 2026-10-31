// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * В режиме «Быстро» любая анимация обрывается на последнем кадре. Бегущая полоса
 * («идёт работа, сколько — неизвестно») на последнем кадре уезжает за край и не видна.
 * У каждой такой полосы должен быть свой вид без движения.
 */
const FILES = [
  "../../routes/(app)/updates/+page.svelte",
  "../../routes/download/+page.svelte",
  "../features/incoming/IncomingHost.svelte",
];

describe("бегущие полосы без анимаций", () => {
  for (const file of FILES) {
    it(file, () => {
      const text: string = readFileSync(new URL(file, import.meta.url), "utf8");
      // Полоса бежит: бесконечная анимация, ключевые кадры уводят её вбок.
      const runs = /animation:\s*([\w-]+)[^;]*infinite/.exec(text);
      expect(runs, "в файле есть бегущая полоса").not.toBeNull();
      const name = runs![1]!;
      expect(text).toMatch(new RegExp(`@keyframes ${name}[^}]*translateX`));
      const still = /:global\(\.no-motion\)[^{]*\{[^}]*animation:\s*none[^}]*transform:\s*none/;
      expect(text, "есть неподвижный вид для режима «Быстро»").toMatch(still);
    });
  }
});

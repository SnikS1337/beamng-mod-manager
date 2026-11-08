// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * Бегущие полосы («идёт работа, сколько — неизвестно») — одни на всю программу:
 * общие ключевые кадры и шаг (`--run` в app.css). В режиме «Быстро» любая анимация
 * обрывается на последнем кадре — полоса уехала бы за край, поэтому у каждой полосы
 * свой вид без движения.
 */
const FILES = [
  "../../routes/(app)/updates/+page.svelte",
  "../../routes/download/+page.svelte",
  "../features/garage/ModCard.svelte",
];

const read = (file: string): string => readFileSync(new URL(file, import.meta.url), "utf8");

describe("бегущие полосы", () => {
  it("общие кадры уводят полосу вбок, шаг один", () => {
    const css = read("../../app.css");
    expect(css).toMatch(/--run: bar-run 1\.3s var\(--ease-in-out\) infinite;/);
    expect(css).toMatch(/@keyframes bar-run \{\s*from \{\s*translate: -100% 0;/);
  });

  for (const file of FILES) {
    it(file, () => {
      const text = read(file);
      expect(text, "полоса — общая").toMatch(/animation: var\(--run\);/);
      expect(text, "своих кадров у полосы нет").not.toMatch(/@keyframes [\w-]*run\b/);
      const still = /:global\(\.no-motion\)[^{]*\{[^}]*animation:\s*none/;
      expect(text, "есть неподвижный вид для режима «Быстро»").toMatch(still);
    });
  }

  it("отблеск в окне установки без анимаций стоит", () => {
    const text = read("../features/incoming/IncomingHost.svelte");
    expect(text).toMatch(/@keyframes sheen[^}]*translateX/);
    expect(text).toMatch(/:global\(\.no-motion\)[^{]*\{[^}]*animation:\s*none/);
  });
});

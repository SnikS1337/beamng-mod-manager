// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { toggleLabel } from "./a11y";

describe("подписи переключателей", () => {
  it("называют мод — иначе десятки «Включить мод» не различить", () => {
    expect(toggleLabel(true, "BMW E34")).toBe("Выключить мод «BMW E34»");
    expect(toggleLabel(false, "Poland")).toBe("Включить мод «Poland»");
  });

  for (const file of ["./ModCard.svelte", "./ModList.svelte"]) {
    it(`${file} берёт подпись с названием`, () => {
      const text: string = readFileSync(new URL(file, import.meta.url), "utf8");
      expect(text).toContain("toggleLabel(mod.enabled, mod.name)");
      expect(text).not.toContain('"Выключить мод" : "Включить мод"');
    });
  }
});

describe("форма «Указать страницу»", () => {
  it("подпись и ошибка связаны с полем", () => {
    const text: string = readFileSync(new URL("./Inspector.svelte", import.meta.url), "utf8");
    expect(text).toContain('<label for="bind-url">');
    expect(text).toContain('id="bind-url"');
    expect(text).toContain('aria-describedby={bindError ? "bind-error" : undefined}');
    expect(text).toContain('aria-invalid={bindError ? "true" : undefined}');
    expect(text).toContain('id="bind-error"');
  });
});

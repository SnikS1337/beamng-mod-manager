// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { StaleGuard } from "./staleGuard";

describe("проверка архива и смена мода", () => {
  it("ответ по архиву прошлого мода больше не текущий", () => {
    const guard = new StaleGuard();
    const first = guard.begin();
    expect(guard.isCurrent(first)).toBe(true);
    guard.invalidate();
    expect(guard.isCurrent(first)).toBe(false);
    const second = guard.begin();
    expect(guard.isCurrent(second)).toBe(true);
    expect(guard.isCurrent(first)).toBe(false);
  });

  it("новая проверка вытесняет старую", () => {
    const guard = new StaleGuard();
    const first = guard.begin();
    const second = guard.begin();
    expect(guard.isCurrent(first)).toBe(false);
    expect(guard.isCurrent(second)).toBe(true);
  });
});

describe("окно скачивания", () => {
  it("смена мода отбрасывает идущую проверку архива", () => {
    const page: string = readFileSync(
      new URL("../../../routes/download/+page.svelte", import.meta.url),
      "utf8",
    );
    const handler = page.slice(page.indexOf("listenDownloadContext((value) =>"));
    const body = handler.slice(0, handler.indexOf("context = value"));
    expect(body).toContain("checks.invalidate()");
    expect(page).not.toMatch(/prepareToken/);
  });
});

describe("окно скачивания для экранного диктора", () => {
  it("иконки скрыты от диктора, адрес — в своей области", () => {
    const page: string = readFileSync(
      new URL("../../../routes/download/+page.svelte", import.meta.url),
      "utf8",
    );
    const icons = page.match(/<[A-Z][A-Za-z]* size=\{[^>]*\/>/g) ?? [];
    expect(icons.length).toBeGreaterThan(0);
    for (const icon of icons) expect(icon, icon).toContain('aria-hidden="true"');
    expect(page).toMatch(/class="urlbar" role="region" aria-label="Адрес страницы"/);
  });
});

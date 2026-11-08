// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("контекстное меню мода", () => {
  it("пока открыто, горячие клавиши гаража не работают", () => {
    // Было: Delete при открытом меню удалял выбранный мод, Ctrl+A менял выбор под меню.
    const menu: string = readFileSync(new URL("./ModMenu.svelte", import.meta.url), "utf8");
    expect(menu).toContain("<ContextMenu.Root onOpenChange={openChange}>");
    expect(menu).toContain("overlay = enterOverlay()");
    expect(menu).toContain("onDestroy(() => openChange(false))");
    const page: string = readFileSync(new URL("./GaragePage.svelte", import.meta.url), "utf8");
    expect(page).toMatch(/if \(modalOpen\(\) \|\| anyOverlay\(\) \|\| event\.defaultPrevented\) return;/);
  });
});

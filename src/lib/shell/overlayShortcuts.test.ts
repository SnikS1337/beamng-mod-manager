import { describe, expect, it } from "vitest";
// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";

const source = (path: string) => readFileSync(new URL(path, import.meta.url), "utf8") as string;

describe("горячие клавиши под оверлеями", () => {
  it("подсказки скрыты под модалкой", () => {
    expect(source("./ShortcutHints.svelte")).toContain("hold.shown && !anyOverlay()");
  });
  it("Ctrl+F не перехватывается под модалкой", () => {
    expect(source("../../routes/(app)/search/+page.svelte")).toContain(
      "if (event.defaultPrevented || anyOverlay()) return;",
    );
  });
  it("одна галочка обслуживается Delete и Space", () => {
    const page = source("../features/garage/GaragePage.svelte");
    expect(page).toContain("const actionable = selected ?? checked");
    expect(page).toContain("void (selected ? deleteSelected() : deleteCard(actionable))");
    expect(page).toContain('event.key === " " && actionable');
  });
  it("версия не анимируется при первичном монтировании", () => {
    expect(source("../features/garage/ModCard.svelte")).toContain(
      "mounted ? rollIn(node) : { duration: 0 }",
    );
    const list = source("../features/garage/ModList.svelte");
    expect(list).toContain("shownVersions.get(id)");
    expect(list).toContain("shouldRollVersion(previous, text, animated)");
  });
});
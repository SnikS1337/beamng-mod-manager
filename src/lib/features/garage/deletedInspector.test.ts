import { describe, expect, it } from "vitest";
// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";

describe("подсказка в панели удалённого мода", () => {
  const source = readFileSync(new URL("./DeletedInspector.svelte", import.meta.url), "utf8") as string;

  it("не обещает включение: выключенный мод возвращается выключенным", () => {
    // Возврат сохраняет прежнюю выключенность (versions.rs, unremove/restore_backup),
    // а панель раньше обещала «снова включится» всем без разбора.
    expect(source).not.toContain("снова включится");
    expect(source).toContain("останется выключенным");
  });
});
// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { beforeClose, runCloseHooks } from "./closeGuard";

describe("перед закрытием главного окна", () => {
  it("дела выполняются по очереди, сбой одного не мешает остальным", async () => {
    const done: string[] = [];
    const stopFirst = beforeClose(async () => {
      done.push("first");
    });
    const stopBroken = beforeClose(() => {
      throw new Error("сбой");
    });
    const stopLast = beforeClose(() => {
      done.push("last");
    });
    await runCloseHooks();
    expect(done).toEqual(["first", "last"]);
    stopFirst();
    stopBroken();
    stopLast();
    await runCloseHooks();
    expect(done).toEqual(["first", "last"]);
  });

  it("обработчик закрытия у окна один", () => {
    // Два onCloseRequested закрывали окно каждый сам: вопрос «Прервать загрузку?»
    // не мог его удержать.
    const files = [
      "../../routes/(app)/settings/+page.svelte",
      "../features/download/DownloadsHost.svelte",
    ].map((path) => readFileSync(new URL(path, import.meta.url), "utf8") as string);
    const count = files.reduce(
      (sum, text) => sum + (text.match(/onCloseRequested\(/g) ?? []).length,
      0,
    );
    expect(count).toBe(1);
    expect(files[0]).toContain("beforeClose(");
    expect(files[1]).toContain("runCloseHooks()");
  });
});

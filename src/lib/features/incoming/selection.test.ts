import { describe, expect, it } from "vitest";
import type { IncomingItemDto } from "#lib/ipc/incoming.js";
import {
  installSucceeded,
  installable,
  mayRemoveSource,
  preselected,
  suspiciousFiles,
  suspiciousText,
  undoAll,
  wholeArchiveInstalled,
} from "./selection";
// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";

function item(patch: Partial<IncomingItemDto> = {}): IncomingItemDto {
  return {
    zipPath: "C:/x.zip",
    fileName: "x.zip",
    size: 1,
    title: "X",
    kind: "new",
    replaces: null,
    replacesEnabled: true,
    replacesSubFolder: null,
    locked: false,
    repacked: false,
    blocked: false,
    warnings: false,
    findings: [],
    similarTo: null,
    ...patch,
  };
}

const script = (entry: string) => ({
  severity: "warning" as const,
  code: "executable",
  message: "Сомнительные файлы",
  entry,
});

describe("выбор модов к установке", () => {
  it("обычный мод отмечен сразу", () => {
    expect(preselected(item())).toBe(true);
  });

  it("мод со скриптами Windows можно поставить, но галочку ставит пользователь", () => {
    const maserati = item({
      warnings: true,
      findings: [script("tools/Start_Spotify_Remote.bat"), script("tools/remote.ps1")],
    });
    expect(installable(maserati)).toBe(true);
    expect(preselected(maserati)).toBe(false);
    expect(suspiciousFiles(maserati)).toEqual(["tools/Start_Spotify_Remote.bat", "tools/remote.ps1"]);
  });

  it("заблокированный и уже установленный не ставятся", () => {
    expect(installable(item({ blocked: true }))).toBe(false);
    expect(installable(item({ kind: "alreadyInstalled" }))).toBe(false);
  });

  it("длинный список файлов сокращается", () => {
    expect(suspiciousText(["a.bat", "b.bat", "c.bat", "d.bat", "e.bat"])).toBe("a.bat, b.bat, c.bat и ещё 2");
    expect(suspiciousText(["a.bat"])).toBe("a.bat");
  });
});

describe("удаление исходного архива", () => {
  it("только если поставлено всё, что в нём есть", () => {
    const a = item({ fileName: "a.zip" });
    const b = item({ fileName: "B.zip" });
    const done = new Set(["a.zip", "b.zip"]);
    expect(wholeArchiveInstalled([a, b], done)).toBe(true);
    expect(wholeArchiveInstalled([a, b], new Set(["a.zip"]))).toBe(false);
    expect(wholeArchiveInstalled([a, item({ kind: "alreadyInstalled" })], done)).toBe(true);
  });

  it("не удаляет архив, где что-то не разобрано или заблокировано", () => {
    // «Ещё 30 модов не разобраны» и битые вложенные архивы пропали бы вместе с исходником.
    const a = item({ fileName: "a.zip" });
    const overflow = item({ fileName: "pack.zip", blocked: true });
    expect(wholeArchiveInstalled([a, overflow], new Set(["a.zip", "pack.zip"]))).toBe(false);
  });
});


describe("удаление исходника после установки", () => {
  it("в архиве остался непоставленный мод — исходник бережём", () => {
    const a = item({ fileName: "a.zip" });
    const b = item({ fileName: "b.zip" });
    expect(mayRemoveSource([a, b], new Set(["a.zip"]), 0)).toBe(false);
  });

  it("часть модов не поставилась — исходник бережём", () => {
    const a = item({ fileName: "a.zip" });
    expect(mayRemoveSource([a], new Set<string>(), 1)).toBe(false);
  });

  it("строка «ещё N модов не разобраны» не даёт удалить исходник", () => {
    const a = item({ fileName: "a.zip" });
    const overflow = item({ fileName: "pack.zip", blocked: true });
    expect(mayRemoveSource([a, overflow], new Set(["a.zip"]), 0)).toBe(false);
  });

  it("поставлено всё, что в архиве есть, — исходник можно удалить", () => {
    const a = item({ fileName: "a.zip" });
    const b = item({ fileName: "B.zip" });
    const already = item({ fileName: "c.zip", kind: "alreadyInstalled" });
    expect(mayRemoveSource([a, b, already], new Set(["a.zip", "b.zip"]), 0)).toBe(true);
  });

  it("решение про исходник не зависит от галочек — только от поставленного", () => {
    // Доотметка пункта во время установки раньше делала архив «полностью разобранным».
    for (const file of ["./IncomingHost.svelte", "../../../routes/download/+page.svelte"]) {
      const source = readFileSync(new URL(file, import.meta.url), "utf8") as string;
      expect(source, file).toContain("mayRemoveSource(");
      expect(source, file).not.toContain("leftInArchive");
    }
  });
});

describe("итог установки", () => {
  it("всё поставилось — окно можно закрыть и рабочую папку убрать", () => {
    expect(installSucceeded(2, 0)).toBe(true);
  });

  it("хоть одна ошибка — окно и распакованное остаются", () => {
    expect(installSucceeded(1, 1)).toBe(false);
    expect(installSucceeded(0, 2)).toBe(false);
  });

  it("не поставилось ничего — окно остаётся, чтобы попробовать снова", () => {
    expect(installSucceeded(0, 0)).toBe(false);
  });

  it("галочки не меняются, пока идёт установка", () => {
    const source = readFileSync(new URL("./IncomingHost.svelte", import.meta.url), "utf8") as string;
    expect(source).toContain("disabled={!installable(item) || installing}");
  });
});

describe("отмена установки", () => {
  it("снимает все моды с конца, даже если один не снялся, и говорит о первой ошибке", async () => {
    const done: string[] = [];
    const problem = await undoAll(
      ["a", "b", "c"],
      async (ticket) => {
        if (ticket === "b" || ticket === "a") throw new Error(`занят ${ticket}`);
        done.push(ticket);
      },
      (reason) => (reason as Error).message,
    );
    expect(done).toEqual(["c"]);
    expect(problem).toBe("занят b");
  });

  it("всё снялось — ошибки нет", async () => {
    const done: number[] = [];
    expect(await undoAll([1, 2], async (ticket) => void done.push(ticket), String)).toBeNull();
    expect(done).toEqual([2, 1]);
  });
});

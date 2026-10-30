import { describe, expect, it } from "vitest";
import type { IncomingItemDto } from "#lib/ipc/incoming.js";
import {
  installable,
  preselected,
  suspiciousFiles,
  suspiciousText,
  wholeArchiveInstalled,
} from "./selection";

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


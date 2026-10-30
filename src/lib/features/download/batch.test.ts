import { describe, expect, it } from "vitest";
import type { DownloadRequest } from "#lib/ipc/download.js";
import type { IncomingArchiveDto, IncomingItemDto } from "#lib/ipc/incoming.js";
import { batchLine, collect, nextPosition, pickedItems, positionOf } from "./batch";

function request(name: string): DownloadRequest {
  return {
    modName: name,
    installedVersion: "1.0",
    latestVersion: "2.0",
    pageUrl: `https://stmods.org/${name}/`,
    previewUrl: null,
    role: "vehicle",
  };
}

function item(fileName: string, extra: Partial<IncomingItemDto> = {}): IncomingItemDto {
  return {
    zipPath: `C:/work/${fileName}`,
    fileName,
    title: fileName,
    kind: "update",
    replaces: fileName,
    size: 100,
    blocked: false,
    warnings: false,
    locked: false,
    findings: [],
    similar: [],
    ...extra,
  } as IncomingItemDto;
}

function archive(items: IncomingItemDto[], problem: string | null = null): IncomingArchiveDto {
  return { source: "C:/Downloads/a.zip", workDir: "C:/work", problem, items };
}

describe("обновление пачкой", () => {
  const batch = [request("a"), request("b"), request("c")];

  it("знает, какой мод открыт", () => {
    expect(positionOf(batch, batch[1])).toBe(1);
    expect(positionOf(batch, null)).toBe(-1);
  });

  it("идёт к следующему нескачанному моду", () => {
    expect(nextPosition(batch, 0, [])).toBe(1);
    expect(nextPosition(batch, 0, [collect(batch[1], archive([item("b.zip")]))])).toBe(2);
    expect(nextPosition(batch, 2, [])).toBeNull();
  });

  it("ставит отмеченное из всех архивов, кроме непригодных", () => {
    const good = collect(batch[0], archive([item("a.zip"), item("bad.zip", { blocked: true })]));
    const broken = collect(batch[1], archive([item("b.zip")], "Архив повреждён"));
    const off = collect(batch[2], archive([item("c.zip")]));
    off.selected = [false];
    expect(pickedItems([good, broken, off]).map((entry) => entry.fileName)).toEqual(["a.zip"]);
  });

  it("пишет, где мы в очереди", () => {
    expect(batchLine(1, 5, 0)).toBe("Мод 2 из 5");
    expect(batchLine(1, 5, 1)).toBe("Мод 2 из 5 · скачано 1");
  });
});

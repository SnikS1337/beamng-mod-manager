import { describe, expect, it } from "vitest";
import type { DownloadRequest } from "#lib/ipc/download.js";
import type { IncomingArchiveDto, IncomingItemDto } from "#lib/ipc/incoming.js";
import {
  batchLine,
  collect,
  installRequests,
  nextPosition,
  pickedItems,
  positionOf,
  sameAsInstalled,
} from "./batch";

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

describe("версия страницы при установке", () => {
  it("одно обновление в архиве получает версию страницы", () => {
    // «Манекен» со STMods: файл «Dummy v.0.20.2.zip», страница — 1.2.
    const requests = installRequests([
      { request: { ...request("dummy"), latestVersion: "1.2" }, archive: archive([item("Dummy v.0.20.2.zip", { replaces: "dummy.zip" })]), selected: [true] },
    ]);
    expect(requests).toEqual([
      {
        zipPath: "C:/work/Dummy v.0.20.2.zip",
        fileName: "Dummy v.0.20.2.zip",
        replaces: "dummy.zip",
        allowWarnings: true,
        version: "1.2",
      },
    ]);
  });

  it("в наборе из нескольких обновлений версию не угадываем", () => {
    const requests = installRequests([
      { request: request("pack"), archive: archive([item("a.zip"), item("b.zip")]), selected: [true, true] },
    ]);
    expect(requests.every((entry) => !("version" in entry))).toBe(true);
  });

  it("новый мод из архива версию страницы не получает, порядок — как у pickedItems", () => {
    const entries = [
      collect(request("a"), archive([item("a.zip"), item("new.zip", { kind: "new", replaces: null })])),
      collect(request("b"), archive([item("b.zip")], "битый")),
      collect({ ...request("c"), latestVersion: " " }, archive([item("c.zip")])),
    ];
    const requests = installRequests(entries);
    expect(requests.map((entry) => entry.fileName)).toEqual(
      pickedItems(entries).map((entry) => entry.fileName),
    );
    expect(requests.find((entry) => entry.fileName === "a.zip")?.version).toBe("2.0");
    expect(requests.find((entry) => entry.fileName === "new.zip")).not.toHaveProperty("version");
    expect(requests.find((entry) => entry.fileName === "c.zip")).not.toHaveProperty("version");
  });

  it("без страницы (архив брошен на окно) — без версии", () => {
    const requests = installRequests([{ request: null, archive: archive([item("x.zip")]), selected: [true] }]);
    expect(requests[0]).not.toHaveProperty("version");
  });
});

describe("скачался тот же файл", () => {
  it("всё в архиве уже стоит — это та же версия", () => {
    // Smoke Dust FX: ModsGaming подписал 0.14.1 как «1.14.1», скачивали дважды.
    expect(
      sameAsInstalled(archive([item("smokeDustFX_mihaits.zip", { kind: "alreadyInstalled" })])),
    ).toBe(true);
  });

  it("есть что ставить, пусто или архив битый — не «та же версия»", () => {
    expect(sameAsInstalled(archive([item("a.zip"), item("b.zip", { kind: "alreadyInstalled" })]))).toBe(false);
    expect(sameAsInstalled(archive([]))).toBe(false);
    expect(sameAsInstalled(archive([item("a.zip", { kind: "alreadyInstalled" })], "битый"))).toBe(false);
  });
});

import { describe, expect, it, vi } from "vitest";
import type { UpdateFinding, UpdatesSnapshot } from "#lib/ipc/updates.js";
import {
  BROWSER_HINT,
  findingOf,
  openUpdate,
  requestOf,
  updateFromGarage,
  updateRoute,
  type UpdateDeps,
} from "./startUpdate";

function finding(patch: Partial<UpdateFinding> = {}): UpdateFinding {
  return {
    modId: "car",
    modName: "Car",
    source: "modland",
    status: "likely",
    installedVersion: "1.0",
    latestVersion: "1.3",
    url: "https://modland.test/car",
    dismissed: false,
    previewUrl: null,
    role: "vehicle",
    installedSize: null,
    latestSize: null,
    noticedAt: null,
    authorVersion: null,
    fileVerdict: null,
    fileName: null,
    fileVersion: null,
    fileGone: false,
    siteDown: null,
    alternatives: [],
    ...patch,
  };
}

function snapshot(findings: UpdateFinding[], hidden: UpdateFinding[] = []): UpdatesSnapshot {
  return { findings, hidden, suggestions: [] } as unknown as UpdatesSnapshot;
}

function deps(patch: Partial<UpdateDeps> = {}) {
  const calls: string[] = [];
  const value: UpdateDeps = {
    openWindow: vi.fn(async () => void calls.push("window")),
    openBrowser: vi.fn(async (url: string) => (calls.push(`browser ${url}`), true)),
    expectDownload: vi.fn(async () => void calls.push("expect")),
    toast: vi.fn((text: string) => void calls.push(`toast ${text}`)),
    ...patch,
  };
  return { value, calls };
}

const down = { note: "Не открывается", reach: "reset" as const };

describe("«Обновить» одинаково везде", () => {
  it("запрос окна загрузки — из находки", () => {
    expect(requestOf(finding({ previewUrl: "p", role: null }))).toEqual({
      modName: "Car",
      installedVersion: "1.0",
      latestVersion: "1.3",
      pageUrl: "https://modland.test/car",
      previewUrl: "p",
      role: null,
      modId: "car",
    });
  });

  it("окно загрузки открылось — браузер не нужен", async () => {
    const { value, calls } = deps();
    await openUpdate(finding(), value);
    expect(calls).toEqual(["window"]);
  });

  it("окна нет — страница в браузере, «Загрузки» и подсказка", async () => {
    const { value, calls } = deps({
      openWindow: async () => {
        throw new Error("нет окна");
      },
    });
    await openUpdate(finding(), value);
    expect(calls).toEqual(["browser https://modland.test/car", "expect", `toast ${BROWSER_HINT}`]);
  });

  it("и браузер не открылся — за «Загрузками» не следим и не обещаем", async () => {
    const { value, calls } = deps({
      openWindow: async () => {
        throw new Error("нет окна");
      },
      openBrowser: async () => false,
    });
    await openUpdate(finding(), value);
    expect(calls).toEqual([]);
  });
});

describe("«Обновить» из гаража", () => {
  it("сайт открывается — как в «Обновлениях»: окно загрузки", async () => {
    const { value, calls } = deps();
    const result = await updateFromGarage("car", async () => snapshot([finding()]), value);
    expect(result).toEqual({ kind: "opened" });
    expect(calls).toEqual(["window"]);
  });

  it("сайт не открывается — ведём туда, где спросят про запасной сайт", async () => {
    const { value, calls } = deps();
    const result = await updateFromGarage("car", async () => snapshot([finding({ siteDown: down })]), value);
    expect(result).toEqual({ kind: "ask", route: "/updates?update=car" });
    expect(calls).toEqual([]);
  });

  it("скрытое обновление открываем как есть, даже если сайт не открывается", async () => {
    const { value, calls } = deps();
    const result = await updateFromGarage(
      "car",
      async () => snapshot([], [finding({ siteDown: down, dismissed: true })]),
      value,
    );
    expect(result).toEqual({ kind: "opened" });
    expect(calls).toEqual(["window"]);
  });

  it("обновления уже нет — так и говорим", async () => {
    const { value, calls } = deps();
    expect(await updateFromGarage("car", async () => snapshot([]), value)).toEqual({ kind: "gone" });
    expect(calls).toEqual([]);
  });

  it("ошибка чтения находок не глотается — гараж покажет её", async () => {
    const { value } = deps();
    await expect(
      updateFromGarage(
        "car",
        async () => {
          throw new Error("нет связи");
        },
        value,
      ),
    ).rejects.toThrow("нет связи");
  });

  it("находка — сначала видимая, потом скрытая; адрес экранирует id", () => {
    const visible = finding({ modId: "a" });
    const hidden = finding({ modId: "b" });
    expect(findingOf(snapshot([visible], [hidden]), "a")).toBe(visible);
    expect(findingOf(snapshot([visible], [hidden]), "b")).toBe(hidden);
    expect(findingOf(snapshot([visible], [hidden]), "c")).toBeNull();
    expect(updateRoute("mod id/1&x")).toBe("/updates?update=mod%20id%2F1%26x");
  });
});

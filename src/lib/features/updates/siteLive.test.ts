// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  STAGE_SITE_DONE,
  STAGE_SITE_FAILED,
  isSiteFinish,
  progressLine,
  siteLiveShare,
  siteLiveText,
  trackSite,
} from "./siteLive";

const step = (source: string, stage: string, done = 0, total = 0) => ({ source, stage, done, total });

describe("ход проверки по сайтам", () => {
  it("каждый сайт — своя строка: идёт, проверен, не проверен", () => {
    let known = trackSite({}, step("modland", "лента", 1, 3));
    known = trackSite(known, step("stmods", "карточки", 2, 10));
    expect(siteLiveText(known.modland)).toEqual({ tone: "busy", text: "лента 1 из 3" });
    expect(siteLiveText(known.stmods)).toEqual({ tone: "busy", text: "моды 2 из 10" });
    known = trackSite(known, step("modland", STAGE_SITE_DONE, 1, 1));
    known = trackSite(known, step("stmods", STAGE_SITE_FAILED, 1, 1));
    expect(siteLiveText(known.modland).tone).toBe("ok");
    expect(siteLiveText(known.stmods).tone).toBe("bad");
    expect(siteLiveText(undefined)).toEqual({ tone: "wait", text: "ждёт очереди" });
  });

  it("общее «готово» и поиск страниц сайтами не считаются", () => {
    const known = {};
    expect(trackSite(known, step("", "готово", 1, 1))).toBe(known);
    expect(trackSite(known, step("страницы", "страницы", 2, 5))).toBe(known);
  });

  it("закончивший сайт поздним ходом обратно не «идёт»", () => {
    let known = trackSite({}, step("x", STAGE_SITE_DONE));
    known = trackSite(known, step("x", "карточки", 3, 4));
    expect(known.x?.state).toBe("done");
  });

  it("полоска сайта: доля, неизвестно сколько, конец", () => {
    expect(siteLiveShare(undefined)).toBe(0);
    expect(siteLiveShare({ state: "running", stage: "лента", done: 1, total: 4 })).toBe(0.25);
    expect(siteLiveShare({ state: "running", stage: "лента", done: 9, total: 4 })).toBe(1);
    expect(siteLiveShare({ state: "running", stage: "лента", done: 0, total: 0 })).toBeNull();
    expect(siteLiveShare({ state: "failed", stage: "", done: 0, total: 0 })).toBe(1);
  });

  it("событие «сайт закончил» узнаётся", () => {
    expect(isSiteFinish(step("x", STAGE_SITE_DONE))).toBe(true);
    expect(isSiteFinish(step("x", STAGE_SITE_FAILED))).toBe(true);
    expect(isSiteFinish(step("x", "лента"))).toBe(false);
    expect(isSiteFinish(null)).toBe(false);
  });

  it("названия этапов совпадают с бэкендом", () => {
    const runner: string = readFileSync("crates/bmm-checks/src/runner.rs", "utf8");
    expect(runner).toContain(`pub const STAGE_SITE_DONE: &str = "${STAGE_SITE_DONE}";`);
    expect(runner).toContain(`pub const STAGE_SITE_FAILED: &str = "${STAGE_SITE_FAILED}";`);
  });
});

describe("общая строка хода", () => {
  const name = (id: string) => ({ modland: "Modland" })[id] ?? id;

  it("этапы сайта, поиск страниц и пустой ход", () => {
    expect(progressLine(null, name)).toBe("Проверяем сайты…");
    expect(progressLine(step("modland", "лента", 1, 3), name)).toBe("Modland · лента 1 из 3");
    expect(progressLine(step("modland", "карточки", 0, 0), name)).toBe("Modland · карточки");
    expect(progressLine(step("страницы", "страницы", 2, 5), name)).toBe("Ищем страницы: 2 из 5");
    expect(progressLine(step("страницы", "страницы", 0, 0), name)).toBe("Ищем страницы…");
    expect(progressLine(step("", "готово", 1, 1), name)).toBe("проверка · готово 1 из 1");
  });

  it("сайт закончил — «проверен», а не «готово 1 из 1»", () => {
    expect(progressLine(step("modland", STAGE_SITE_DONE, 1, 1), name)).toBe("Modland · проверен");
    expect(progressLine(step("modland", STAGE_SITE_FAILED, 1, 1), name)).toBe("Modland · не проверен");
  });
});

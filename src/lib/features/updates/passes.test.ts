import { describe, expect, it } from "vitest";
import type { UpdateSite, UpdateSource } from "#lib/ipc/updates.js";
import {
  coveredBy,
  followupOf,
  freshSources,
  markPassed,
  nextFromQueue,
  noPasses,
  queuePass,
  runStarted,
  sitesAfterPass,
  wasPassed,
} from "./passes";

function source(id: string, status: UpdateSource["status"], note: string | null = null): UpdateSource {
  return {
    id,
    displayName: id,
    status,
    cardsFetched: 0,
    feedPagesFetched: 0,
    postponed: 0,
    missing: 0,
    unreadable: 0,
    layoutChanged: false,
    note,
    reach: null,
  };
}

function site(id: string, status: UpdateSite["status"], tracked = 3): UpdateSite {
  return { id, displayName: id, status, tracked, lastChecked: null, note: null } as UpdateSite;
}

const asks = "сайт просит пройти проверку «вы человек?»";

describe("пройденные проверки сайтов", () => {
  it("проверка, начатая до прохождения, не возвращает «просит проверку»", () => {
    // Человек нажал «Проверить сейчас», ModHub попросил проверку, человек её прошёл,
    // а проверка закончилась со старым ответом про ModHub.
    const started = runStarted(noPasses());
    const passes = markPassed(started.passes, "modhub");
    const live = freshSources([source("modhub", "needsHumanCheck", asks), source("modland", "ok")], passes, started.epoch);
    expect(live[0]).toMatchObject({ id: "modhub", status: "ok", note: null });
    expect(live[1].status).toBe("ok");
  });

  it("проверка, начатая после прохождения, может сказать, что сайт просит снова", () => {
    const passed = markPassed(noPasses(), "modhub");
    const started = runStarted(passed);
    const live = freshSources([source("modhub", "needsHumanCheck", asks)], started.passes, started.epoch);
    expect(live[0]).toMatchObject({ status: "needsHumanCheck", note: asks });
  });

  it("фоновая проверка считается начатой до прохождения", () => {
    const passes = markPassed(noPasses(), "modhub");
    expect(freshSources([source("modhub", "needsHumanCheck")], passes, 0)[0].status).toBe("ok");
  });

  it("прохождение одного сайта не трогает другой", () => {
    const started = runStarted(noPasses());
    const passes = markPassed(started.passes, "modhub");
    const live = freshSources([source("modland", "needsHumanCheck", asks)], passes, started.epoch);
    expect(live[0]).toMatchObject({ status: "needsHumanCheck", note: asks });
  });

  it("недоступный сайт остаётся недоступным и после прохождения", () => {
    const started = runStarted(noPasses());
    const passes = markPassed(started.passes, "modhub");
    const live = freshSources([source("modhub", "unavailable", "таймаут")], passes, started.epoch);
    expect(live[0]).toMatchObject({ status: "unavailable", note: "таймаут" });
  });

  it("повторное прохождение сдвигает время: старая проверка снова устарела", () => {
    let passes = markPassed(noPasses(), "modhub");
    const started = runStarted(passes);
    passes = markPassed(started.passes, "modhub");
    expect(freshSources([source("modhub", "needsHumanCheck")], passes, started.epoch)[0].status).toBe("ok");
  });

  it("сайт без прохождения — ответы как есть", () => {
    const passes = noPasses();
    expect(wasPassed(passes, "modhub")).toBe(false);
    const list = [source("modhub", "needsHumanCheck")];
    expect(freshSources(list, passes, 0)).toEqual(list);
    expect(wasPassed(markPassed(passes, "modhub"), "modhub")).toBe(true);
  });

  it("не меняет переданные списки", () => {
    const started = runStarted(noPasses());
    const passes = markPassed(started.passes, "modhub");
    const list = [source("modhub", "needsHumanCheck", asks)];
    freshSources(list, passes, started.epoch);
    expect(list[0].status).toBe("needsHumanCheck");
  });
});

describe("панель сайтов после прохождения", () => {
  it("«просит проверку» сразу сменяется на «пускает»", () => {
    const sites = [site("modhub", "human_check"), site("modland", "human_check")];
    const after = sitesAfterPass(sites, "modhub");
    expect(after.map((item) => item.status)).toEqual(["ok", "human_check"]);
    expect(sites[0].status).toBe("human_check");
  });

  it("выключенный и недоступный сайт так и остаются", () => {
    const after = sitesAfterPass([site("modhub", "off"), site("modland", "unavailable")], "modhub");
    expect(after.map((item) => item.status)).toEqual(["off", "unavailable"]);
    expect(sitesAfterPass([site("modland", "unavailable")], "modland")[0].status).toBe("unavailable");
  });
});

describe("что делать после прохождения", () => {
  const tracked: Record<string, number> = { modhub: 3, modland: 0, stmods: 5 };
  const trackedOf = (site: string) => tracked[site] ?? 0;

  it("сайт без модов — ищем на нём страницы, с модами — проверяем снова", () => {
    expect(followupOf(0)).toBe("findPages");
    expect(followupOf(2)).toBe("check");
  });

  it("очередь без повторов, в порядке прохождения", () => {
    let queue = queuePass([], "modhub");
    queue = queuePass(queue, "modland");
    queue = queuePass(queue, "modhub");
    expect(queue).toEqual(["modhub", "modland"]);
  });

  it("после проверки — проверка, поиск страниц остаётся на потом", () => {
    const first = nextFromQueue(["modland", "modhub", "stmods"], trackedOf);
    expect(first).toEqual({ run: "check", rest: ["modland"] });
    expect(nextFromQueue(first.rest, trackedOf)).toEqual({ run: "findPages", rest: [] });
    expect(nextFromQueue([], trackedOf)).toEqual({ run: null, rest: [] });
  });

  it("новая проверка снимает из очереди то, что покроет сама", () => {
    // Иначе после следующей проверки по кнопке запускалась ещё одна, лишняя.
    const queue = ["modhub", "modland"];
    expect(coveredBy(queue, "check", trackedOf)).toEqual(["modland"]);
    expect(coveredBy(queue, "findPages", trackedOf)).toEqual(["modhub"]);
    expect(coveredBy(queue, "retry", trackedOf)).toEqual(queue);
  });

  it("сайт, за которым начали следить, уже проверяется, а не ищется", () => {
    // Прошли сайт без модов, а пока шла проверка, на нём нашли страницу.
    const later = (site: string) => (site === "modland" ? 1 : trackedOf(site));
    expect(nextFromQueue(["modland"], later)).toEqual({ run: "check", rest: [] });
  });
});

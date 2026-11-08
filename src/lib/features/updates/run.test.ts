import { describe, expect, it } from "vitest";
import type { UpdatesEvent } from "#lib/ipc/updates.js";
import { updatesRun } from "./run.svelte";

function event(requestId: number, phase: UpdatesEvent["phase"]): UpdatesEvent {
  return {
    requestId,
    phase,
    progress: phase === "progress" ? { source: "stmods", stage: "лента", done: 1, total: 3 } : null,
    findings: [],
    hidden: [],
    suggestions: [],
    sources: [],
    newCount: 0,
    upToDate: 0,
    requests: 0,
    cancelled: false,
    stoppedByTime: false,
    error: null,
    followed: 0,
  } as unknown as UpdatesEvent;
}

describe("идущая проверка", () => {
  it("переживает уход из раздела и отдаёт итог при возврате", () => {
    const id = updatesRun.begin();
    updatesRun.onEvent(event(id, "progress"));
    expect(updatesRun.checking).toBe(true);
    expect(updatesRun.progress?.done).toBe(1);
    // Раздел закрыт — итог всё равно запоминается.
    updatesRun.onEvent(event(id, "done"));
    expect(updatesRun.checking).toBe(false);
    expect(updatesRun.pending?.phase).toBe("done");
  });

  it("не путает фоновую проверку со своей", () => {
    const id = updatesRun.begin();
    updatesRun.onEvent(event(0, "progress"));
    expect(updatesRun.autoProgress).not.toBeNull();
    expect(updatesRun.checking).toBe(true);
    updatesRun.onEvent(event(0, "auto"));
    expect(updatesRun.autoProgress).toBeNull();
    updatesRun.onEvent(event(id + 5, "done"));
    expect(updatesRun.checking).toBe(true);
  });

  it("пока раздел открыт, итог не откладывает: его показывает сам раздел", () => {
    const id = updatesRun.begin();
    updatesRun.attached = true;
    try {
      updatesRun.onEvent(event(id, "done"));
      // Иначе при следующем входе тот же итог и тост показались бы ещё раз.
      expect(updatesRun.pending).toBeNull();
    } finally {
      updatesRun.attached = false;
    }
  });

  it("итог, пришедший до подписки раздела, раздел получает при подключении", () => {
    const id = updatesRun.begin();
    // Раздел ещё не подписался: событие ловит общий слушатель.
    updatesRun.onEvent(event(id, "done"));
    expect(updatesRun.checking).toBe(false);
    const handed = updatesRun.attach();
    try {
      expect(handed?.requestId).toBe(id);
      expect(updatesRun.pending).toBeNull();
    } finally {
      updatesRun.attached = false;
    }
  });

  it("итог, который раздел уже показал сам, второй раз не отдаётся", () => {
    const id = updatesRun.begin();
    updatesRun.onEvent(event(id, "done"));
    // Подписка раздела успела раньше и показала этот же итог.
    updatesRun.handledDone = id;
    try {
      expect(updatesRun.attach()).toBeNull();
    } finally {
      updatesRun.attached = false;
    }
  });
});

describe("«сайт закончил» и общая строка хода", () => {
  it("строку хода не перетирает — другие сайты ещё идут", () => {
    const id = updatesRun.begin();
    updatesRun.onEvent(event(id, "progress"));
    updatesRun.onEvent({
      ...event(id, "progress"),
      progress: { source: "modland", stage: "сайт проверен", done: 1, total: 1 },
    });
    expect(updatesRun.progress).toEqual({ source: "stmods", stage: "лента", done: 1, total: 3 });
    updatesRun.checking = false;
  });

  it("у фоновой проверки тоже, но первое событие всё же включает её ход", () => {
    updatesRun.onEvent({
      ...event(0, "progress"),
      progress: { source: "modland", stage: "сайт проверен", done: 1, total: 1 },
    });
    expect(updatesRun.autoProgress?.stage).toBe("сайт проверен");
    updatesRun.onEvent(event(0, "progress"));
    updatesRun.onEvent({
      ...event(0, "progress"),
      progress: { source: "modland", stage: "сайт не открылся", done: 1, total: 1 },
    });
    expect(updatesRun.autoProgress?.stage).toBe("лента");
    updatesRun.onEvent(event(0, "auto"));
    expect(updatesRun.autoProgress).toBeNull();
  });
});

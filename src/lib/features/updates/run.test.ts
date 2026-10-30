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
});

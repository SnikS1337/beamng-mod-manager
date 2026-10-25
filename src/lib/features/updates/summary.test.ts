import { describe, expect, it } from "vitest";
import type { UpdateSource } from "#lib/ipc/updates.js";
import { checkSummary } from "./summary";

function source(patch: Partial<UpdateSource>): UpdateSource {
    return {
        id: "stmods",
        displayName: "STMods",
        status: "ok",
        cardsFetched: 0,
        feedPagesFetched: 0,
        postponed: 0,
        missing: 0,
        unreadable: 0,
        layoutChanged: false,
        note: null,
        ...patch,
    };
}

const base = {
    cancelled: false,
    newCount: 0,
    requests: 3,
    stoppedByTime: false,
    sources: [],
};

describe("checkSummary", () => {
    it("молчит после остановки", () => {
        expect(checkSummary({ ...base, cancelled: true }).text).toBeNull();
    });

    it("сообщает, что новых версий нет, и сколько отложено", () => {
        expect(checkSummary(base)).toEqual({
            text: "Новых версий нет",
            offerForce: false,
        });
        expect(
            checkSummary({
                ...base,
                sources: [source({ postponed: 4 }), source({ postponed: 1 })],
            }).text,
        ).toBe("Новых версий нет. Отложено, проверяли недавно: 5");
    });

    it("предлагает проверить заново, если сайты не трогали", () => {
        const summary = checkSummary({
            ...base,
            requests: 0,
            sources: [source({ postponed: 7 })],
        });
        expect(summary.offerForce).toBe(true);
        expect(summary.text).toContain("проверено недавно");
    });

    it("считает найденное", () => {
        expect(checkSummary({ ...base, newCount: 2 }).text).toBe(
            "Нашли обновления: 2",
        );
    });

    it("называет сайты, которые не ответили, а не «всё проверено»", () => {
        const down = source({ status: "unavailable", displayName: "Modland" });
        const summary = checkSummary({ ...base, requests: 0, sources: [down] });
        expect(summary.offerForce).toBe(false);
        expect(summary.text).toBe("Новых версий нет. Не ответили: Modland");
        const human = source({
            status: "needsHumanCheck",
            displayName: "Modland",
        });
        expect(checkSummary({ ...base, sources: [human] }).text).toContain(
            "Modland: нужно пройти проверку",
        );
    });

    it("говорит, что проверили не всё, когда кончилось время", () => {
        expect(
            checkSummary({
                ...base,
                stoppedByTime: true,
                sources: [source({ postponed: 3 })],
            }).text,
        ).toBe(
            "Новых версий нет. Проверили не всё — остальное в следующий раз",
        );
    });

  it("сообщает, что сайт изменился или страницы пропали", () => {
    const changed = source({ displayName: "STMods", layoutChanged: true, missing: 2 });
    expect(checkSummary({ ...base, sources: [changed] }).text).toBe(
      "Новых версий нет. STMods: сайт изменился, часть модов не прочитали. Страницы пропали с сайта: 2",
    );
  });
});

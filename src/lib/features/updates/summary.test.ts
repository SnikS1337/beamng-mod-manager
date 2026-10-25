import { describe, expect, it } from "vitest";
import type { UpdateSite, UpdateSource } from "#lib/ipc/updates.js";
import { checkSummary, failedSites, updateHeadline } from "./summary";

function site(patch: Partial<UpdateSite>): UpdateSite {
    return {
        id: "stmods",
        displayName: "STMods",
        status: "ok",
        tracked: 3,
        ...patch,
    };
}

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

describe("updateHeadline", () => {
    const now = "2026-10-25T10:00:00Z";

    it("не называет всё актуальным, если сайт не ответил", () => {
        expect(
            updateHeadline(0, [
                site({ displayName: "Modland", status: "unavailable" }),
                site({ displayName: "STMods" }),
            ], now),
        ).toEqual({
            text: "Проверено не всё: Modland",
            tone: "warn",
        });
        expect(
            updateHeadline(0, [
                site({ displayName: "Modland", status: "human_check" }),
            ], now).text,
        ).toBe("Проверено не всё: Modland");
    });

    it("различает ещё не проверяли, все сайты выключены и нечего проверять", () => {
        expect(updateHeadline(0, [], null)).toEqual({
            text: "Ещё не проверяли",
            tone: "muted",
        });
        expect(
            updateHeadline(0, [site({ status: "off", tracked: 0 })], now).text,
        ).toBe("Все сайты выключены");
        expect(
            updateHeadline(0, [site({ tracked: 0 })], now).text,
        ).toBe("Нечего проверять");
    });

    it("показывает число ожидающих решения и не прячет проблемный сайт", () => {
        expect(
            updateHeadline(2, [
                site({ displayName: "STMods" }),
                site({ displayName: "Modland", status: "unavailable" }),
            ], now),
        ).toEqual({
            text: "2 мода ждут решения · проверено не всё",
            tone: "warn",
        });
        expect(failedSites([
            site({ displayName: "STMods" }),
            site({ displayName: "Modland", status: "unavailable" }),
        ]).map((item) => item.displayName)).toEqual(["Modland"]);
    });

    it("всё актуально только после полной проверки", () => {
        expect(
            updateHeadline(0, [site({ displayName: "STMods" })], now),
        ).toEqual({ text: "Всё актуально", tone: "ok" });
    });
});

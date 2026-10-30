import { describe, expect, it } from "vitest";
import type { UpdateSite, UpdateSource } from "#lib/ipc/updates.js";
import { checkSummary, failedSites, updateHeadline } from "./summary";

function site(patch: Partial<UpdateSite>): UpdateSite {
    return {
        id: "stmods",
        displayName: "STMods",
        status: "ok",
        tracked: 3,
        lastChecked: null,
        note: null,
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
        ).toBe("Новых версий нет. 5 модов проверены недавно — их очередь в следующий раз");
        expect(
            checkSummary({ ...base, sources: [source({ postponed: 1 })] }).text,
        ).toBe("Новых версий нет. 1 мод проверен недавно — их очередь в следующий раз");
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
            "Нашли обновления: 2 мода",
        );
        expect(checkSummary({ ...base, newCount: 5 }).text).toBe(
            "Нашли обновления: 5 модов",
        );
    });

    it("называет сайты, которые не ответили, а не «всё проверено»", () => {
        const down = source({ status: "unavailable", displayName: "Modland" });
        const summary = checkSummary({ ...base, requests: 0, sources: [down] });
        expect(summary.offerForce).toBe(false);
        expect(summary.text).toBe("Не ответили: Modland");
        const human = source({
            status: "needsHumanCheck",
            displayName: "Modland",
        });
        expect(checkSummary({ ...base, sources: [human] }).text).toContain(
            "Modland: нужно пройти проверку",
        );
    });

    it("разделяет проверку и отказ сайта", () => {
        const human = source({
            status: "needsHumanCheck",
            displayName: "Modland",
        });
        const down = source({ status: "unavailable", displayName: "STMods" });
        const summary = checkSummary({ ...base, sources: [human, down] });
        expect(summary.text).toBe(
            "Новых версий нет. Modland: нужно пройти проверку на сайте. Не ответили: STMods",
        );
    });

    it("не обещает проверку, когда все сайты выключены", () => {
        expect(
            checkSummary({ ...base, requests: 0, sources: [] }),
        ).toEqual({
            text: "Нет включённых сайтов — проверять нечего",
            offerForce: false,
        });
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
      "Новых версий нет. STMods: сайт изменился, часть модов не прочитали. Пропавшие страницы: 2",
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
            text: "2 мода с обновлением · проверено не всё",
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
        expect(
            updateHeadline(0, [site({ displayName: "STMods" })], now, {
                mods: 35,
                tracked: 35,
                withoutPage: 0,
                declined: 0,
            }),
        ).toEqual({ text: "Всё актуально", tone: "ok" });
    });

    it("не называет актуальным то, что не проверяли: моды без страницы", () => {
        expect(
            updateHeadline(0, [site({ displayName: "STMods" })], now, {
                mods: 35,
                tracked: 26,
                withoutPage: 9,
                declined: 0,
            }),
        ).toEqual({
            text: "Актуальны 26 из 35: у 9 модов нет страницы — их не проверяли",
            tone: "warn",
        });
    });
});

describe("итог проверки и моды без страницы", () => {
    it("называет, сколько модов не проверяли", () => {
        expect(
            checkSummary({
                ...base,
                coverage: { mods: 1000, tracked: 940, withoutPage: 60, declined: 0 },
            }).text,
        ).toBe("Новых версий нет. У 60 модов нет страницы — их не проверяли");
        expect(
            checkSummary({
                ...base,
                coverage: { mods: 10, tracked: 9, withoutPage: 1, declined: 0 },
            }).text,
        ).toBe("Новых версий нет. У 1 мода нет страницы — их не проверяли");
    });
});

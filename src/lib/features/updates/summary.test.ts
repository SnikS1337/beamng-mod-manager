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

    it("говорит, что нового нет, и сколько ждут обновления по-прежнему", () => {
        // Было: «Новых версий нет» рядом с «Доступные 4».
        expect(checkSummary(base)).toEqual({ text: "Новых обновлений нет" });
        const waiting = Array.from({ length: 4 }, () => ({}) as never);
        expect(checkSummary({ ...base, findings: waiting }).text).toBe(
            "Новых обновлений нет, ждут обновления, как и раньше: 4",
        );
    });

    it("не пишет «проверены недавно — завтра»: кнопка проверяет все моды", () => {
        const text = checkSummary({
            ...base,
            sources: [source({ postponed: 4 }), source({ postponed: 1 })],
        }).text;
        expect(text).not.toContain("недавно");
        expect(text).toBe("Новых обновлений нет. Не успели: 5 модов — проверим в следующий раз");
    });

    it("считает найденное и всё, что ждёт", () => {
        expect(checkSummary({ ...base, newCount: 2 }).text).toBe("Нашли обновления: 2 мода");
        const all = Array.from({ length: 6 }, () => ({}) as never);
        expect(checkSummary({ ...base, newCount: 5, findings: all }).text).toBe(
            "Нашли обновления: 5 модов, всего ждут обновления: 6",
        );
    });

    it("называет сайты, которые не ответили, а не «всё проверено»", () => {
        const down = source({ status: "unavailable", displayName: "Modland" });
        expect(checkSummary({ ...base, requests: 0, sources: [down] }).text).toBe(
            "Не ответили: Modland",
        );
        const human = source({ status: "needsHumanCheck", displayName: "Modland" });
        expect(checkSummary({ ...base, sources: [human] }).text).toContain(
            "Modland: нужно пройти проверку",
        );
    });

    it("разделяет проверку и отказ сайта", () => {
        const human = source({ status: "needsHumanCheck", displayName: "Modland" });
        const down = source({ status: "unavailable", displayName: "STMods" });
        expect(checkSummary({ ...base, sources: [human, down] }).text).toBe(
            "Новых обновлений нет. Modland: нужно пройти проверку на сайте. Не ответили: STMods",
        );
    });

    it("не обещает проверку, когда все сайты выключены", () => {
        expect(checkSummary({ ...base, requests: 0, sources: [] })).toEqual({
            text: "Нет включённых сайтов — проверять нечего",
        });
    });

    it("не молчит, если сайты есть, а запросов не было", () => {
        expect(checkSummary({ ...base, requests: 0, sources: [source({})] }).text).toBe(
            "Сайты не ответили — проверить не удалось",
        );
    });

    it("говорит, что проверили не всё, когда кончилось время", () => {
        expect(
            checkSummary({ ...base, stoppedByTime: true, sources: [source({ postponed: 3 })] }).text,
        ).toBe("Новых обновлений нет. Проверили не всё — остальное в следующий раз");
    });

    it("сообщает, что сайт изменился или страницы пропали", () => {
        const changed = source({ displayName: "STMods", layoutChanged: true, missing: 2 });
        expect(checkSummary({ ...base, sources: [changed] }).text).toBe(
            "Новых обновлений нет. STMods: сайт изменился, часть модов не прочитали. Пропавшие страницы: 2",
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

    it("не считает моды, от проверки которых отказались", () => {
        expect(
            updateHeadline(0, [site({ displayName: "STMods" })], now, {
                mods: 35,
                tracked: 26,
                withoutPage: 7,
                declined: 2,
            }).text,
        ).toBe("Актуальны 26 из 33: у 7 модов нет страницы — их не проверяли");
    });
});

describe("итог проверки и моды без страницы", () => {
    it("называет, сколько модов не проверяли", () => {
        expect(
            checkSummary({
                ...base,
                coverage: { mods: 1000, tracked: 940, withoutPage: 60, declined: 0 },
            }).text,
        ).toBe("Новых обновлений нет. У 60 модов нет страницы — их не проверяли");
        expect(
            checkSummary({
                ...base,
                coverage: { mods: 10, tracked: 9, withoutPage: 1, declined: 0 },
            }).text,
        ).toBe("Новых обновлений нет. У 1 мода нет страницы — его не проверяли");
        expect(
            checkSummary({
                ...base,
                coverage: { mods: 30, tracked: 9, withoutPage: 21, declined: 0 },
            }).text,
        ).toBe("Новых обновлений нет. У 21 мода нет страницы — их не проверяли");
    });
});

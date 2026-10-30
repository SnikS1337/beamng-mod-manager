import type { UpdatesEvent, UpdateSite } from "#lib/ipc/updates.js";
import type { CoverageDto } from "#lib/ipc/generated.js";
import { plural } from "#lib/features/garage/format.js";

/** Итог нажатия «Проверить сейчас»: одна строка для тоста и нужна ли «Всё равно проверить». */
export interface CheckSummary {
    text: string | null;
    offerForce: boolean;
}

/** Что показать под заголовком «Обновления». */
export interface UpdateHeadline {
    text: string;
    tone: "muted" | "ok" | "warn";
}

/** Сайты, которые не дали полного ответа в последней проверке. */
export function failedSites(sites: UpdateSite[] | undefined): UpdateSite[] {
    return (sites ?? []).filter(
        (site) => site.status === "human_check" || site.status === "unavailable",
    );
}

/**
 * «Всё актуально» можно говорить, только если проверка действительно прошла по всем
 * включённым сайтам. Иначе честнее назвать сайты, которые не ответили.
 */
/** «у 9 модов нет страницы — их не проверяли»; пусто, если таких нет. */
export function pagelessText(coverage: CoverageDto | null | undefined): string | null {
    const count = coverage?.withoutPage ?? 0;
    if (count <= 0) return null;
    // «у 1 мода … — его»; при 21 по смыслу их много: «у 21 мода … — их».
    return `у ${count} ${plural(count, "мода", "модов", "модов")} нет страницы — ${count === 1 ? "его" : "их"} не проверяли`;
}

export function updateHeadline(
    availableCount: number,
    sites: UpdateSite[] | undefined,
    lastChecked: string | null | undefined,
    coverage?: CoverageDto | null,
): UpdateHeadline {
    if (availableCount > 0) {
        const failed = failedSites(sites);
        return {
            text: `${availableCount} ${plural(availableCount, "мод", "мода", "модов")} с обновлением${failed.length > 0 ? " · проверено не всё" : ""}`,
            tone: failed.length > 0 ? "warn" : "ok",
        };
    }
    if (!lastChecked) return { text: "Ещё не проверяли", tone: "muted" };
    const active = (sites ?? []).filter((site) => site.status !== "off");
    if (active.length === 0) return { text: "Все сайты выключены", tone: "muted" };
    const failed = failedSites(sites);
    if (failed.length > 0) {
        return {
            text: `Проверено не всё: ${failed.map((site) => site.displayName).join(", ")}`,
            tone: "warn",
        };
    }
    if (active.every((site) => site.tracked === 0)) {
        return { text: "Нечего проверять", tone: "muted" };
    }
    // «Всё актуально» — только про моды со страницей: остальные программа не видит.
    const pageless = pagelessText(coverage);
    if (pageless && coverage) {
        return {
            // Моды, от проверки которых отказались, в счёт не идут: это решение,
            // а не пробел — иначе «26 из 35» при 9 без страницы и 2 отказах не сходится.
            text: `Актуальны ${coverage.tracked} из ${coverage.mods - coverage.declined}: ${pageless}`,
            tone: "warn",
        };
    }
    return { text: "Всё актуально", tone: "ok" };
}

type Done = Pick<
    UpdatesEvent,
    "cancelled" | "newCount" | "requests" | "stoppedByTime" | "sources"
> &
    Partial<Pick<UpdatesEvent, "coverage">>;

export function checkSummary(event: Done): CheckSummary {
    // Остановили сами — говорить нечего.
    if (event.cancelled) return { text: null, offerForce: false };
    if (event.sources.length === 0 && event.requests === 0) {
        return {
            text: "Нет включённых сайтов — проверять нечего",
            offerForce: false,
        };
    }

    const human = event.sources.filter((source) => source.status === "needsHumanCheck");
    const down = event.sources.filter((source) => source.status === "unavailable");
    const failed = [...human, ...down];
    const postponed = event.sources.reduce(
        (sum, source) => sum + source.postponed,
        0,
    );
    const parts: string[] = [];

    if (event.newCount > 0) {
        parts.push(
            `Нашли обновления: ${event.newCount} ${plural(event.newCount, "мод", "мода", "модов")}`,
        );
    } else if (event.requests === 0 && failed.length === 0) {
        return {
            text: "Всё проверено недавно. Нажмите ещё раз, чтобы проверить заново",
            offerForce: true,
        };
    } else if (event.requests > 0) {
        parts.push("Новых версий нет");
    }

    if (human.length > 0) {
        const names = human.map((source) => source.displayName).join(", ");
        parts.push(`${names}: нужно пройти проверку на сайте`);
    }
    if (down.length > 0) {
        parts.push(`Не ответили: ${down.map((source) => source.displayName).join(", ")}`);
    }
    const changed = event.sources.filter((source) => source.status === "ok" && source.layoutChanged);
    if (changed.length > 0) {
        parts.push(
            `${changed.map((source) => source.displayName).join(", ")}: сайт изменился, часть модов не прочитали`,
        );
    }
    const missing = event.sources.reduce((sum, source) => sum + source.missing, 0);
    if (missing > 0) parts.push(`Пропавшие страницы: ${missing}`);
    if (event.stoppedByTime) {
        parts.push("Проверили не всё — остальное в следующий раз");
    } else if (postponed > 0) {
        // Это не «не успели»: страницы этих модов проверялись недавно, открывать их
        // снова незачем.
        parts.push(
            `${postponed} ${plural(postponed, "мод проверен", "мода проверены", "модов проверены")} недавно — их очередь в следующий раз`,
        );
    }
    const pageless = pagelessText(event.coverage);
    if (pageless) parts.push(pageless.charAt(0).toUpperCase() + pageless.slice(1));
    return { text: parts.join(". "), offerForce: false };
}

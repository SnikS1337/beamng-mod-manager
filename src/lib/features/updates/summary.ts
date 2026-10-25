import type { UpdatesEvent, UpdateSite } from "#lib/ipc/updates.js";
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
export function updateHeadline(
    availableCount: number,
    sites: UpdateSite[] | undefined,
    lastChecked: string | null | undefined,
): UpdateHeadline {
    if (availableCount > 0) {
        const failed = failedSites(sites);
        return {
            text: `${availableCount} ${plural(availableCount, "мод ждёт", "мода ждут", "модов ждут")} решения${failed.length > 0 ? " · проверено не всё" : ""}`,
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
    return { text: "Всё актуально", tone: "ok" };
}

type Done = Pick<
    UpdatesEvent,
    "cancelled" | "newCount" | "requests" | "stoppedByTime" | "sources"
>;

export function checkSummary(event: Done): CheckSummary {
    // Остановили сами — говорить нечего.
    if (event.cancelled) return { text: null, offerForce: false };

    const failed = event.sources.filter((source) => source.status !== "ok");
    const postponed = event.sources.reduce(
        (sum, source) => sum + source.postponed,
        0,
    );
    const parts: string[] = [];

    if (event.newCount > 0) {
        parts.push(`Нашли обновления: ${event.newCount}`);
    } else if (event.requests === 0 && failed.length === 0) {
        return {
            text: "Всё проверено недавно. Нажмите ещё раз, чтобы проверить заново",
            offerForce: true,
        };
    } else {
        parts.push("Новых версий нет");
    }

    if (failed.length > 0) {
        const names = failed.map((source) => source.displayName).join(", ");
        parts.push(
            failed.some((source) => source.status === "needsHumanCheck")
                ? `${names}: нужно пройти проверку на сайте`
                : `Не ответили: ${names}`,
        );
    }
    const changed = event.sources.filter((source) => source.status === "ok" && source.layoutChanged);
  if (changed.length > 0) {
    parts.push(
      `${changed.map((source) => source.displayName).join(", ")}: сайт изменился, часть модов не прочитали`,
    );
  }
  const missing = event.sources.reduce((sum, source) => sum + source.missing, 0);
  if (missing > 0) parts.push(`Страницы пропали с сайта: ${missing}`);
  if (event.stoppedByTime) {
        parts.push("Проверили не всё — остальное в следующий раз");
    } else if (postponed > 0) {
        parts.push(`Отложено, проверяли недавно: ${postponed}`);
    }
    return { text: parts.join(". "), offerForce: false };
}

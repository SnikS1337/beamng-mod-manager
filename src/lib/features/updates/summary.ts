import type { UpdatesEvent } from "#lib/ipc/updates.js";

/** Итог нажатия «Проверить сейчас»: одна строка для тоста и нужна ли «Всё равно проверить». */
export interface CheckSummary {
    text: string | null;
    offerForce: boolean;
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

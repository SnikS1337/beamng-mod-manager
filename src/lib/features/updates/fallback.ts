import type { UpdateAlternative, UpdateFinding } from "#lib/ipc/updates.js";

/**
 * «Обновить», когда сайт обновления не открывается (блокировка, сайт лежит).
 *
 * Программа не переключает сайт молча: спрашивает. Если то же обновление есть на
 * сайте, который открывается, — «Скачать оттуда?», иначе — «Поискать на других
 * сайтах?». Отказаться можно всегда: «Всё равно открыть» ведёт на прежнюю страницу
 * (вдруг человек включил VPN).
 */
export type UpdatePlan =
  | { kind: "open" }
  | { kind: "askAlternative"; alternative: UpdateAlternative }
  | { kind: "askSearch" }
  | { kind: "notFound" };

export function planUpdate(finding: UpdateFinding): UpdatePlan {
  if (!finding.siteDown) return { kind: "open" };
  const alternative = finding.alternatives[0];
  return alternative ? { kind: "askAlternative", alternative } : { kind: "askSearch" };
}

/** После поиска на других сайтах: нашлось — спрашиваем про него, нет — так и говорим. */
export function planAfterSearch(finding: UpdateFinding | undefined): UpdatePlan {
  if (!finding) return { kind: "open" };
  if (!finding.siteDown) return { kind: "open" };
  const alternative = finding.alternatives[0];
  return alternative ? { kind: "askAlternative", alternative } : { kind: "notFound" };
}

/** Та же находка, но со страницей на другом сайте — для окна загрузки. */
export function onAlternative(finding: UpdateFinding, alternative: UpdateAlternative): UpdateFinding {
  return {
    ...finding,
    source: alternative.source,
    url: alternative.url,
    latestVersion: alternative.latestVersion ?? finding.latestVersion,
    siteDown: null,
    alternatives: [],
  };
}

export interface AskText {
  title: string;
  note: string;
  offer: string;
}

export function askText(
  finding: UpdateFinding,
  plan: UpdatePlan,
  siteName: (id: string) => string,
): AskText {
  const title = `${siteName(finding.source)} не открывается`;
  const note = finding.siteDown?.note ?? "";
  switch (plan.kind) {
    case "askAlternative": {
      const version = plan.alternative.latestVersion ? ` — версия ${plan.alternative.latestVersion}` : "";
      return { title, note, offer: `Это обновление есть на ${siteName(plan.alternative.source)}${version}. Скачать оттуда?` };
    }
    case "askSearch":
      return { title, note, offer: "На других сайтах этот мод ещё не искали. Поискать?" };
    case "notFound":
      return { title, note, offer: "На других сайтах это обновление не нашлось." };
    case "open":
      return { title, note, offer: "" };
  }
}

/** «Обновить все»: какие моды можно взять с другого сайта, а какие есть только на недоступном. */
export interface BatchPlan {
  /** Мод → запасной сайт. */
  swap: { finding: UpdateFinding; alternative: UpdateAlternative }[];
  /** Сайт не открывается, а другого нет. */
  stuck: UpdateFinding[];
}

export function planBatch(findings: UpdateFinding[]): BatchPlan {
  const swap: BatchPlan["swap"] = [];
  const stuck: UpdateFinding[] = [];
  for (const finding of findings) {
    if (!finding.siteDown) continue;
    const alternative = finding.alternatives[0];
    if (alternative) swap.push({ finding, alternative });
    else stuck.push(finding);
  }
  return { swap, stuck };
}

/** Спрашивать перед «Обновить все» — только если есть что предложить. */
export function batchNeedsAsk(plan: BatchPlan): boolean {
  return plan.swap.length > 0;
}

/** Очередь «Обновить все»: с запасных сайтов (если человек согласился) или как есть. */
export function applyBatch(findings: UpdateFinding[], plan: BatchPlan, useAlternatives: boolean): UpdateFinding[] {
  if (!useAlternatives) return findings;
  const swapped = new Map(plan.swap.map((item) => [item.finding.modId, item.alternative]));
  return findings.map((finding) => {
    const alternative = swapped.get(finding.modId);
    return alternative ? onAlternative(finding, alternative) : finding;
  });
}

export function batchAskText(plan: BatchPlan, plural: (count: number, one: string, few: string, many: string) => string): string {
  const count = plan.swap.length;
  const mods = plural(count, "мод", "мода", "модов");
  let text = `${count} ${mods} — на сайтах, которые не открываются, но есть на других. Качать оттуда?`;
  if (plan.stuck.length > 0) {
    const stuck = plan.stuck.length;
    text += ` Ещё ${stuck} ${plural(stuck, "мод", "мода", "модов")} есть только на недоступных сайтах — их страницы откроются как есть.`;
  }
  return text;
}

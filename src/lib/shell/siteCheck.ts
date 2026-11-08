/**
 * Кнопка «Пройти проверку» сайта. У каждого сайта своё окно проверки: пока Modland
 * долго грузится, можно пройти ModHub. Недоступна только кнопка сайта, чьё окно уже
 * открыто, и «Проходим проверку…» пишет только она.
 */
export function passButton(
  passing: readonly string[],
  site: string,
  name: string,
): { text: string; disabled: boolean } {
  const busy = passing.includes(site);
  return {
    text: busy ? "Проходим проверку…" : `Пройти проверку ${name}`,
    disabled: busy,
  };
}

/** Сайт начали проходить: добавить в список открытых окон (без повторов). */
export function startPassing(passing: readonly string[], site: string): string[] {
  return passing.includes(site) ? [...passing] : [...passing, site];
}

/** Окно сайта закрылось (пустил или нет) — его кнопка снова доступна. */
export function stopPassing(passing: readonly string[], site: string): string[] {
  return passing.filter((item) => item !== site);
}

/**
 * Кнопку «Пройти проверку» показываем, только когда сайт её правда просит. Раньше она
 * висела и у сайта «ни одного мода»: такой сайт пускал без проверки, окно открывалось
 * и сразу закрывалось, а кнопка возвращалась — выглядело как вечная проверка.
 * Сайт без модов, который просит проверку, поиск страниц теперь сам отмечает.
 */
export function needsPassButton(site: { tone: string }, checkable: boolean): boolean {
  return checkable && site.tone === "warn";
}

/**
 * Что повторить, когда сайт пустил: поиск по тексту или раздел («Машины», «Карты»).
 * Раньше повторялся только поиск по тексту — в разделе после проверки ничего не
 * происходило, и сайт так и оставался пустым до ручного обновления.
 */
export function rerunAfterPass(submitted: string, browsing: string | null): "search" | "browse" | null {
  if (submitted) return "search";
  if (browsing) return "browse";
  return null;
}

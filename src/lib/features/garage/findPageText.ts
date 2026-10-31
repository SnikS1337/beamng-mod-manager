/** Что сказать после «Найти на сайтах» у одного мода. */
export function findPageText(result: {
  suggested: boolean;
  followed: boolean;
  name: string;
  /** Сайты, которые не ответили: на них не искали. */
  skippedSites: string[];
}): string {
  if (result.suggested) return "Есть похожая страница — подтвердите её в «Обновлениях»";
  if (result.followed) return `Нашли страницу «${result.name}»`;
  if (result.skippedSites.length === 0) return "Страницу найти не удалось";
  // Раньше «не удалось» звучало как «искали везде», хотя Modland и ModHub просили
  // проверку и их не спрашивали.
  const sites = result.skippedSites.join(" и ");
  const verb = result.skippedSites.length === 1 ? "не ответил" : "не ответили";
  return `Страницу не нашли. ${sites} ${verb} — там не искали: пройдите проверку в «Обновлениях» и нажмите ещё раз`;
}

import type { DuplicateOf } from "$lib/ipc/library";

/** Короткая метка на карточке. */
export function duplicateBadge(duplicate: DuplicateOf): string {
  return duplicate.exact ? "Копия" : "Вторая версия";
}

/** Что сказать в панели мода и что сделать. */
export function duplicateExplain(duplicate: DuplicateOf): { title: string; text: string } {
  const other = duplicate.name ? `«${duplicate.name}»` : "другой мод";
  return duplicate.exact
    ? {
        title: "Копия другого мода",
        text: `Тот же архив, что ${other}. Игра загрузит оба — выключите или удалите лишний.`,
      }
    : {
        title: "Ещё одна версия этого мода",
        text: `Те же машины или карты, что в ${other}. Игра загрузит обе версии, и чьи файлы победят — неизвестно. Оставьте одну.`,
      };
}

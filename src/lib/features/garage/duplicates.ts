import type { DuplicateOf, ModCardDto } from "#lib/ipc/library.js";
import { plural } from "./format";

/** Карточки для списка: копии прячутся за карточкой основного мода группы. */
export function visibleCards(cards: ModCardDto[]): ModCardDto[] {
  return cards.filter((card) => !card.duplicateOf);
}

/** Мод виден в списке сам или как копия в группе видимого. */
export function isShown(id: string, shown: ModCardDto[]): boolean {
  return shown.some((card) => card.id === id || card.group.includes(id));
}

/** Основной и его копии — для панели группы. Основной первым. */
export function groupMembers(main: ModCardDto, all: ModCardDto[]): ModCardDto[] {
  const byId = new Map(all.map((card) => [card.id, card]));
  const members = main.group.map((id) => byId.get(id)).filter((card): card is ModCardDto => !!card);
  return [main, ...members];
}

/** Метка в углу карточки основного: сколько всего файлов в группе, считая его самого. */
export function copiesBadge(main: ModCardDto): string | null {
  if (main.group.length === 0) return null;
  const total = main.group.length + 1;
  return `${total} ${plural(total, "копия", "копии", "копий")}`;
}

/** Короткая метка у копии. */
export function duplicateBadge(duplicate: DuplicateOf): string {
  return duplicate.exact ? "Копия" : "Другая версия";
}

/** Что сказать в панели копии. */
export function duplicateExplain(duplicate: DuplicateOf): { title: string; text: string } {
  const other = duplicate.name ? `«${duplicate.name}»` : "другой мод";
  return duplicate.exact
    ? {
        title: "Копия другого мода",
        text: `Тот же архив, что ${other}. В списке он показан одной карточкой.`,
      }
    : {
        title: "Другая версия этого мода",
        text: `Те же машины, карты или скрипт, что в ${other}. В списке они показаны одной карточкой.`,
      };
}

/** Подпись карточки дополнения: не машина и не карта — картинку не показываем. */
export function addonHint(card: ModCardDto): string {
  if (card.kind === "script") return "Скрипт или приложение";
  if (card.kind === "sound") return "Звук";
  return "Дополнение";
}

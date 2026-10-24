import type { SearchCard } from "#lib/ipc/search.js";

/** Метка типа на карточке поиска: «Машина», «Карта», «Дополнение» или ничего. */
export function searchKindLabel(card: Pick<SearchCard, "kind">): string | null {
  switch (card.kind) {
    case "vehicle":
      return "Машина";
    case "map":
      return "Карта";
    case "addon":
      return "Дополнение";
    default:
      return null;
  }
}

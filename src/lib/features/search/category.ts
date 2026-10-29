import type { SearchKindFilter } from "./filters.js";

export type BrowseKind = Exclude<SearchKindFilter, "all">;

const WORDS: Record<BrowseKind, readonly string[]> = {
  vehicle: [
    "машина",
    "машины",
    "машину",
    "машине",
    "машиной",
    "автомобиль",
    "автомобили",
    "автомобилей",
    "car",
    "cars",
    "vehicle",
    "vehicles",
  ],
  map: ["карта", "карты", "карту", "карте", "картой", "карт", "map", "maps"],
  addon: [
    "дополнение",
    "дополнения",
    "дополнений",
    "addon",
    "addons",
    "аддон",
    "аддоны",
  ],
};

/**
 * Запрос целиком про раздел: «карта», «машины», «addons».
 * «карта зима» — уже текстовый поиск, а не раздел.
 */
export function categoryIntent(raw: string): BrowseKind | null {
  const text = raw
    .trim()
    .toLowerCase()
    .replace(/[«»"'.,!?()]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!text || text.includes(" ")) return null;
  for (const [kind, words] of Object.entries(WORDS) as [
    BrowseKind,
    readonly string[],
  ][]) {
    if (words.includes(text)) return kind;
  }
  return null;
}

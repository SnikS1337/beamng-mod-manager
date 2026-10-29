import type { SearchCard } from "#lib/ipc/search.js";

export type SearchKindFilter = "all" | "vehicle" | "map" | "addon";
export type SearchPriceFilter = "all" | "free" | "paid";
export type SearchSortOrder = "relevance" | "newest" | "largest" | "site";

export interface SearchFilter {
  kind: SearchKindFilter;
  price: SearchPriceFilter;
  installedOnly: boolean;
  source: string;
  sort: SearchSortOrder;
  author?: string | null;
  gameVersion?: string | null;
  onlyCompatible?: boolean;
}

export function hasFree(card: SearchCard): boolean {
  return card.price === "free" || card.price === "mixed";
}

export function cardHasPaid(card: SearchCard): boolean {
  return card.price === "paid" || card.price === "mixed";
}

export function newestAt(card: SearchCard): number {
  let newest = 0;
  for (const listing of card.listings) {
    const at = Date.parse(listing.updatedAt ?? listing.publishedAt ?? "");
    if (Number.isFinite(at) && at > newest) newest = at;
  }
  return newest;
}

export function largestSize(card: SearchCard): number {
  return Math.max(0, ...card.listings.map((listing) => listing.sizeBytes ?? 0));
}

/// Старшая пара чисел версии: «0.39.x» и «0.39.3.0» совместимы, «0.40» — нет.
export function versionPair(value: string): [number, number] | null {
  const match = value.match(/(\d+)\.(\d+)/);
  if (!match) return null;
  return [Number(match[1]), Number(match[2])];
}

export function gameMatches(listingVersion: string | null, gameVersion: string): boolean {
  // Сайт не указал версию игры — не скрываем мод: пусть решает человек.
  if (!listingVersion) return true;
  const listing = versionPair(listingVersion);
  const game = versionPair(gameVersion);
  if (!listing || !game) return true;
  return listing[0] === game[0] && listing[1] === game[1];
}

export function filterAndSort(cards: SearchCard[], filter: SearchFilter): SearchCard[] {
  const author = filter.author?.trim().toLowerCase() ?? "";
  const result = cards.filter((card) => {
    if (filter.kind !== "all" && card.kind !== filter.kind) return false;
    if (filter.price === "free" && !hasFree(card)) return false;
    if (filter.price === "paid" && !cardHasPaid(card)) return false;
    if (filter.installedOnly && !card.installed) return false;
    if (
      author &&
      !card.listings.some((listing) => listing.author?.toLowerCase().includes(author))
    ) {
      return false;
    }
    if (
      filter.source !== "all" &&
      !card.listings.some((listing) => listing.source === filter.source)
    ) {
      return false;
    }
    if (
      filter.onlyCompatible &&
      filter.gameVersion &&
      !card.listings.some((listing) => gameMatches(listing.gameVersion, filter.gameVersion!))
    ) {
      return false;
    }
    return true;
  });
  if (filter.sort === "newest") {
    result.sort((left, right) => newestAt(right) - newestAt(left));
  } else if (filter.sort === "largest") {
    result.sort((left, right) => largestSize(right) - largestSize(left));
  }
  return result;
}

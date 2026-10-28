import type { SearchCard } from "#lib/ipc/search.js";

export type SearchKindFilter = "all" | "vehicle" | "map" | "addon";
export type SearchPriceFilter = "all" | "free" | "paid";
export type SearchSortOrder = "relevance" | "newest" | "largest";

export interface SearchFilter {
  kind: SearchKindFilter;
  price: SearchPriceFilter;
  installedOnly: boolean;
  source: string;
  sort: SearchSortOrder;
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

export function filterAndSort(cards: SearchCard[], filter: SearchFilter): SearchCard[] {
  const result = cards.filter((card) => {
    if (filter.kind !== "all" && card.kind !== filter.kind) return false;
    if (filter.price === "free" && !hasFree(card)) return false;
    if (filter.price === "paid" && !cardHasPaid(card)) return false;
    if (filter.installedOnly && !card.installed) return false;
    if (
      filter.source !== "all" &&
      !card.listings.some((listing) => listing.source === filter.source)
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

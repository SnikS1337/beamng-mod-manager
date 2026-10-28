import { describe, expect, it } from "vitest";
import type { SearchCard, SearchListing } from "#lib/ipc/search.js";
import { filterAndSort, gameMatches, versionPair } from "./filters";

function listing(patch: Partial<SearchListing> = {}): SearchListing {
  return {
    source: "stmods",
    sourceId: "1",
    url: "https://example.org/1",
    title: "Mod",
    name: "Mod",
    modVersion: null,
    gameVersion: null,
    author: null,
    category: null,
    sizeBytes: null,
    previewUrl: null,
    price: "unknown",
    offers: [],
    publishedAt: null,
    updatedAt: null,
    externalId: null,
    installed: false,
    ...patch,
  };
}

function card(patch: Partial<SearchCard> = {}): SearchCard {
  return {
    id: "card",
    title: "Mod",
    price: "unknown",
    possibleSameAs: [],
    relevance: 1,
    installed: false,
    kind: "vehicle",
    listings: [listing()],
    ...patch,
  };
}

const base = {
  kind: "all" as const,
  price: "all" as const,
  installedOnly: false,
  source: "all",
  sort: "relevance" as const,
};

describe("фильтры поиска", () => {
  it("бесплатные включают карточки со смешанной ценой", () => {
    const cards = [
      card({ id: "free", price: "free" }),
      card({ id: "mixed", price: "mixed" }),
      card({ id: "paid", price: "paid" }),
      card({ id: "unknown", price: "unknown" }),
    ];
    const result = filterAndSort(cards, { ...base, price: "free" });
    expect(result.map((item) => item.id)).toEqual(["free", "mixed"]);
  });

  it("платные включают смешанную цену", () => {
    const result = filterAndSort(
      [card({ id: "free", price: "free" }), card({ id: "mixed", price: "mixed" })],
      { ...base, price: "paid" },
    );
    expect(result.map((item) => item.id)).toEqual(["mixed"]);
  });

  it("фильтрует по сайту, установленным и типу", () => {
    const cards = [
      card({
        id: "a",
        installed: true,
        kind: "map",
        listings: [listing({ source: "modland" })],
      }),
      card({ id: "b", kind: "vehicle", listings: [listing({ source: "stmods" })] }),
    ];
    expect(filterAndSort(cards, { ...base, source: "modland" }).map((item) => item.id)).toEqual([
      "a",
    ]);
    expect(
      filterAndSort(cards, { ...base, installedOnly: true }).map((item) => item.id),
    ).toEqual(["a"]);
    expect(filterAndSort(cards, { ...base, kind: "vehicle" }).map((item) => item.id)).toEqual([
      "b",
    ]);
  });

  it("фильтрует по автору и совместимости с игрой", () => {
    const cards = [
      card({
        id: "compatible",
        listings: [listing({ author: "Flanje", gameVersion: "0.39.x" })],
      }),
      card({
        id: "other",
        listings: [listing({ author: "Someone", gameVersion: "0.40.x" })],
      }),
    ];
    expect(
      filterAndSort(cards, { ...base, author: "fla" }).map((item) => item.id),
    ).toEqual(["compatible"]);
    expect(
      filterAndSort(cards, {
        ...base,
        onlyCompatible: true,
        gameVersion: "0.39.3.0",
      }).map((item) => item.id),
    ).toEqual(["compatible"]);
  });

  it("сравнивает версии игры по старшей паре чисел", () => {
    expect(versionPair("0.39.x")).toEqual([0, 39]);
    expect(gameMatches("0.39.x", "0.39.3.0")).toBe(true);
    expect(gameMatches("0.40.x", "0.39.3.0")).toBe(false);
    expect(gameMatches(null, "0.39.3.0")).toBe(true);
  });

  it("сортирует новые и крупные", () => {
    const cards = [
      card({
        id: "old",
        listings: [listing({ updatedAt: "2026-01-01T00:00:00Z", sizeBytes: 10 })],
      }),
      card({
        id: "new",
        listings: [listing({ updatedAt: "2026-05-01T00:00:00Z", sizeBytes: 20 })],
      }),
    ];
    expect(filterAndSort(cards, { ...base, sort: "newest" }).map((item) => item.id)).toEqual([
      "new",
      "old",
    ]);
    expect(filterAndSort(cards, { ...base, sort: "largest" }).map((item) => item.id)).toEqual([
      "new",
      "old",
    ]);
  });
});

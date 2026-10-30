import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  SearchCard,
  SearchEvent,
  SearchListing,
  SearchSource,
} from "./generated.js";
import type { SearchPriceSummary } from "./unions.js";

export type {
  SearchCard,
  SearchEvent,
  SearchListing,
  SearchOffer,
  SearchSource,
} from "./generated.js";
export type {
  SearchOfferKind,
  SearchPhase,
  SearchPrice,
  SearchPriceSummary,
  SearchSourceStatus,
} from "./unions.js";

type MockListener = (event: SearchEvent) => void;
const mockListeners = new Set<MockListener>();
let mockCurrentCards: SearchCard[] = [];
let mockCurrentSources: SearchSource[] = mockSourceReports();

export async function listenSearch(handler: MockListener): Promise<UnlistenFn> {
  if (!isTauri()) {
    mockListeners.add(handler);
    return () => mockListeners.delete(handler);
  }
  return listen<SearchEvent>("search://event", (event) => handler(event.payload));
}

export async function searchStart(query: string, requestId: number): Promise<void> {
  if (isTauri()) {
    await invoke("search_start", { query, requestId });
    return;
  }
  mockSearch(query, requestId);
}

export async function searchBrowse(
  kind: "vehicle" | "map" | "addon",
  fromPage: number,
  requestId: number,
): Promise<SearchEvent> {
  if (isTauri()) {
    return invoke<SearchEvent>("search_browse", { kind, fromPage, requestId });
  }
  const cards = mockCards("карта").map((card) => ({ ...card, kind }));
  const sources = mockDoneSources(cards);
  mockCurrentCards = cards;
  mockCurrentSources = sources;
  return { requestId, phase: "done", cards, sources, error: null };
}

export async function searchRetrySource(
  query: string,
  sourceId: string,
  requestId: number,
): Promise<SearchEvent> {
  if (isTauri()) {
    return invoke<SearchEvent>("search_retry_source", { query, sourceId, requestId });
  }
  const cards = mockCards(query);
  const sources = mockDoneSources(cards);
  mockCurrentCards = cards;
  mockCurrentSources = sources;
  return { requestId, phase: "done", cards, sources, error: null };
}

export async function searchCancel(requestId?: number): Promise<void> {
  if (isTauri()) {
    await invoke("search_cancel", { requestId: requestId ?? null });
  }
}

export async function searchPreview(url: string): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("search_preview", { url });
}

export async function searchPagePreview(
  source: string,
  page: string,
): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("search_page_preview", { source, page });
}

/** «Пройти проверку» сайта из поиска: окно открывает выдачу этого сайта. */
export async function searchOpenCheck(source: string, query: string): Promise<boolean> {
  if (!isTauri()) {
    await new Promise((resolve) => setTimeout(resolve, 900));
    if (source === "modland") mockModlandNeedsCheck = false;
    return true;
  }
  return invoke<boolean>("search_open_check", { source, query });
}

export async function searchDecide(
  query: string,
  a: { source: string; sourceId: string },
  b: { source: string; sourceId: string },
  same: boolean,
): Promise<SearchEvent> {
  if (isTauri()) {
    return invoke<SearchEvent>("search_decide", {
      query,
      aSource: a.source,
      aId: a.sourceId,
      bSource: b.source,
      bId: b.sourceId,
      same,
    });
  }
  return mockDecide(query, a, b, same);
}

const TITLES = [
  "BMW M2 G87",
  "Toyota Sienna 2011-2017",
  "Gavril D-Series Offroad",
  "Ibishu Covet Rally",
  "ETK 800-Series Tuned",
  "Карта «Приозерск»",
  "Temporal Anti Aliasing",
  "Dodge Charger SRT",
  "Audi RS6 C7",
  "Ford Mustang GT",
  "Lada Niva 4x4",
  "Bruckell Bastion Police",
] as const;

const SOURCE_NAMES: Record<string, string> = {
  modland: "Modland",
  stmods: "STMods",
  modsgaming: "ModsGaming",
  modhub: "ModHub",
  beamng: "BeamNG (официальный)",
};

let mockModlandNeedsCheck = true;

function mockSourceReports(): SearchSource[] {
  return [
    { id: "modland", displayName: "Modland", status: "ok", count: 0, note: null, pending: true },
    { id: "stmods", displayName: "STMods", status: "ok", count: 0, note: null, pending: true },
    {
      id: "modsgaming",
      displayName: "ModsGaming",
      status: "ok",
      count: 0,
      note: null,
      pending: true,
    },
  ];
}

function mockListing(index: number, source: string, title: string): SearchListing {
  const paid = index % 5 === 0 && source === "modland";
  return {
    source,
    sourceId: `${source}-${index}`,
    url: `https://example.org/${source}/${index}`,
    title,
    name: title,
    modVersion: `1.${index % 7}`,
    gameVersion: "0.39.x",
    author: index % 2 === 0 ? "InfnityModding" : "matissator",
    category: index % 3 === 0 ? "Cars" : "Mods",
    sizeBytes: (30 + index * 17) * 1024 * 1024,
    previewUrl: null,
    price: paid ? "paid" : source === "beamng" ? "free" : "unknown",
    offers: paid
      ? [
          {
            kind: "paidLink",
            host: "patreon.com",
            url: "https://www.patreon.com/author",
            fileId: null,
            sizeBytes: null,
          },
        ]
      : [
          {
            kind: "externalHost",
            host: "modsfire",
            url: "https://modsfire.com/demo",
            fileId: "demo",
            sizeBytes: null,
          },
        ],
    publishedAt: null,
    updatedAt: new Date(Date.now() - index * 86_400_000).toISOString(),
    externalId: source === "beamng" ? "MOKU3EJNG" : null,
    installed: index % 6 === 0,
  };
}

function mockCards(query: string): SearchCard[] {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const titles = TITLES.filter((title) =>
    words.some((word) => title.toLowerCase().includes(word)),
  );
  const chosen = (titles.length > 0 ? titles : TITLES).slice(0, 8);
  return chosen.map((title, index) => {
    const sources = index % 3 === 0 ? ["modland", "stmods"] : ["stmods"];
    const listings = sources.map((source, offset) =>
      mockListing(index * 2 + offset, source, title),
    );
    return {
      id: listings.map((item) => `${item.source}:${item.sourceId}`).sort().join("+"),
      title,
      price: priceSummaryFor(listings),
      possibleSameAs: index === 1 ? [mockCardId(2)] : [],
      relevance: 1 - index * 0.05,
      installed: listings.some((item) => item.installed),
      kind: index % 4 === 3 ? "addon" : index % 5 === 4 ? null : "vehicle",
      listings,
    };
  });
}

function priceSummaryFor(listings: SearchListing[]): SearchPriceSummary {
  const paid = listings.some((item) => item.price === "paid");
  const free = listings.some((item) => item.price === "free");
  if (paid && free) return "mixed";
  if (paid) return "paid";
  if (free) return "free";
  return "unknown";
}

function mockCardId(index: number): string {
  return `stmods:stmods-${index * 2}`;
}

function emitMock(event: SearchEvent): void {
  for (const listener of mockListeners) listener(event);
}

function mockDoneSources(cards: SearchCard[]): SearchSource[] {
  return mockSourceReports().map((source, index) => ({
    ...source,
    pending: false,
    count:
      source.id === "beamng"
        ? 0
        : cards.filter((card) =>
            card.listings.some((listing) => listing.source === source.id),
          ).length,
    status:
      index === 0 && mockModlandNeedsCheck
        ? ("needsHumanCheck" as const)
        : source.status,
    note:
      index === 0 && mockModlandNeedsCheck
        ? "Сайт просит подтвердить, что вы человек."
        : source.note,
  }));
}

function mockSearch(query: string, requestId: number): void {
  const sources = mockSourceReports();
  const cards = mockCards(query);
  window.setTimeout(() => {
    emitMock({ requestId, phase: "partial", cards, sources, error: null });
  }, 260);
  window.setTimeout(() => {
    const reports = mockDoneSources(cards);
    mockCurrentCards = cards;
    mockCurrentSources = reports;
    emitMock({ requestId, phase: "done", cards, sources: reports, error: null });
  }, 720);
}

function mockDecide(
  query: string,
  a: { source: string; sourceId: string },
  b: { source: string; sourceId: string },
  same: boolean,
): SearchEvent {
  let cards = mockCurrentCards.length > 0 ? mockCurrentCards : mockCards(query);
  const cardA = cards.find((card) =>
    card.listings.some(
      (listing) => listing.source === a.source && listing.sourceId === a.sourceId,
    ),
  );
  const cardB = cards.find((card) =>
    card.listings.some(
      (listing) => listing.source === b.source && listing.sourceId === b.sourceId,
    ),
  );
  if (!cardA || !cardB) {
    return { requestId: 0, phase: "regroup", cards, sources: mockCurrentSources, error: null };
  }
  if (same) {
    const listings = [...cardA.listings];
    for (const listing of cardB.listings) {
      if (!listings.some((item) => item.source === listing.source)) {
        listings.push(listing);
      }
    }
    const merged: SearchCard = {
      ...cardA,
      id: listings.map((item) => `${item.source}:${item.sourceId}`).sort().join("+"),
      listings,
      price: priceSummaryFor(listings),
      installed: listings.some((item) => item.installed),
      possibleSameAs: cardA.possibleSameAs.filter((id) => id !== cardB.id),
    };
    cards = cards
      .filter((card) => card.id !== cardB.id)
      .map((card) => card.id === cardA.id ? merged : card);
  } else {
    cards = cards.map((card) => {
      if (card.id === cardA.id) {
        return { ...card, possibleSameAs: card.possibleSameAs.filter((id) => id !== cardB.id) };
      }
      if (card.id === cardB.id) {
        return { ...card, possibleSameAs: card.possibleSameAs.filter((id) => id !== cardA.id) };
      }
      return card;
    });
  }
  mockCurrentCards = cards;
  return { requestId: 0, phase: "regroup", cards, sources: mockCurrentSources, error: null };
}

export function sourceName(id: string): string {
  return SOURCE_NAMES[id] ?? id;
}

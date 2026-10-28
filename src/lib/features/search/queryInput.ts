import type { SearchKindFilter } from "./filters";

export interface ParsedSearch {
  /** Текст, который уходит сайтам: без операторов. */
  text: string;
  author: string | null;
  kind: SearchKindFilter | null;
  source: string | null;
}

const KIND_ALIASES: Record<string, SearchKindFilter> = {
  машина: "vehicle",
  машины: "vehicle",
  авто: "vehicle",
  car: "vehicle",
  cars: "vehicle",
  vehicle: "vehicle",
  карта: "map",
  карты: "map",
  map: "map",
  maps: "map",
  level: "map",
  дополнение: "addon",
  дополнения: "addon",
  addon: "addon",
  script: "addon",
};

const SOURCE_ALIASES: Record<string, string> = {
  modland: "modland",
  stmods: "stmods",
  modsgaming: "modsgaming",
  "modsgaming.us": "modsgaming",
};

/// `автор:Flanje`, `тип:карта`, `сайт:stmods`; остальное — обычный запрос.
export function parseSearchInput(raw: string): ParsedSearch {
  const words = raw.trim().split(/\s+/).filter(Boolean);
  const rest: string[] = [];
  let author: string | null = null;
  let kind: SearchKindFilter | null = null;
  let source: string | null = null;
  for (const word of words) {
    const split = word.indexOf(":");
    if (split <= 0) {
      rest.push(word);
      continue;
    }
    const key = word.slice(0, split).toLowerCase();
    const value = word.slice(split + 1).trim();
    if (!value) continue;
    if (key === "автор" || key === "author") {
      author = value;
    } else if (key === "тип" || key === "type") {
      kind = KIND_ALIASES[value.toLowerCase()] ?? null;
      if (!kind) rest.push(word);
    } else if (key === "сайт" || key === "site" || key === "source") {
      source = SOURCE_ALIASES[value.toLowerCase()] ?? null;
      if (!source) rest.push(word);
    } else {
      rest.push(word);
    }
  }
  return { text: rest.join(" "), author, kind, source };
}

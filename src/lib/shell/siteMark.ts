/**
 * Значок сайта: монограмма в своём цвете — в поиске, обновлениях, списках сайтов.
 * Не логотипы сайтов, а свои метки: узнаётся с одного взгляда и не выдаёт себя за
 * сайт. Белые буквы на цвете — контраст не ниже 4.5:1 (тест).
 */
export interface SiteMark {
  letters: string;
  color: string;
}

const MARKS: Record<string, SiteMark> = {
  modland: { letters: "ML", color: "#2f6aa8" },
  stmods: { letters: "ST", color: "#a8461f" },
  modsgaming: { letters: "MG", color: "#6a4cb0" },
  modhub: { letters: "MH", color: "#1c7a50" },
  beamng: { letters: "BN", color: "#4b5563" },
};

const UNKNOWN_COLOR = "#5b6472";

export function siteMark(id: string): SiteMark {
  const known = MARKS[id];
  if (known) return known;
  // Новый сайт без своей метки — две первые буквы, нейтральный цвет.
  const letters = id.replace(/[^\p{L}\p{N}]/gu, "").slice(0, 2).toUpperCase() || "?";
  return { letters, color: UNKNOWN_COLOR };
}

export const KNOWN_SITES = Object.keys(MARKS);

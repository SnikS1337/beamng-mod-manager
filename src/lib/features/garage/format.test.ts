import { describe, expect, it } from "vitest";
import {
  formatAgo,
  formatSize,
  formatWhen,
  headerNote,
  kindLabel,
  matchesQuery,
  plural,
  words,
} from "./format";

describe("formatSize", () => {
  it("пусто и ноль — прочерк", () => {
    expect(formatSize(0)).toBe("—");
    expect(formatSize(-5)).toBe("—");
  });
  it("килобайты, мегабайты, гигабайты", () => {
    expect(formatSize(495_418)).toBe("484 КБ");
    expect(formatSize(37_092_684)).toBe("35 МБ");
    expect(formatSize(1.6 * 1024 ** 3)).toBe("1,6 ГБ");
  });
});

describe("plural", () => {
  const mods = (count: number) => plural(count, "мод", "мода", "модов");
  it("русские формы", () => {
    expect(mods(1)).toBe("мод");
    expect(mods(3)).toBe("мода");
    expect(mods(5)).toBe("модов");
    expect(mods(11)).toBe("модов");
    expect(mods(14)).toBe("модов");
    expect(mods(21)).toBe("мод");
    expect(mods(112)).toBe("модов");
    expect(mods(122)).toBe("мода");
  });
});

describe("headerNote", () => {
  it("без модов, с модами, с обновлениями", () => {
    expect(headerNote(0, 0)).toBe("модов пока нет");
    expect(headerNote(8, 0)).toBe("8 модов");
    expect(headerNote(8, 1)).toBe("8 модов · 1 ждёт обновления");
    expect(headerNote(8, 3)).toBe("8 модов · 3 ждут обновления");
    expect(headerNote(5000, 295)).toBe("5 000 модов · 295 ждут обновления");
    expect(headerNote(21, 0)).toBe("21 мод");
  });
});

describe("kindLabel", () => {
  it("знакомые и незнакомые типы", () => {
    expect(kindLabel("vehicle")).toBe("Машина");
    expect(kindLabel("level")).toBe("Карта");
    expect(kindLabel("что-то")).toBe("Мод");
  });
});

describe("matchesQuery", () => {
  const audi = { name: "Audi Sport GMBH RS3 Y8", fileName: "Audi_RS3_8Y_2022.zip" };
  it("ищет по началу слов в названии и имени файла", () => {
    expect(matchesQuery(audi, "rs3")).toBe(true);
    expect(matchesQuery(audi, "audi 2022")).toBe(true);
    expect(matchesQuery(audi, "  ")).toBe(true);
    expect(matchesQuery(audi, "bmw")).toBe(false);
    expect(matchesQuery(audi, "udi")).toBe(false);
  });
  it("ё и е, регистр", () => {
    const map = { name: "Ёлки", fileName: "elki.zip" };
    expect(matchesQuery(map, "ЕЛ")).toBe(true);
    expect(words("Ёж_Ёлка")).toEqual(["еж", "елка"]);
  });
});

describe("formatAgo", () => {
  const now = Date.parse("2026-10-23T12:00:00Z");
  const ago = (minutes: number) => formatAgo(new Date(now - minutes * 60000).toISOString(), now);
  it("от минут до месяцев", () => {
    expect(formatAgo(null, now)).toBe("");
    expect(formatAgo("не дата", now)).toBe("");
    expect(ago(0)).toBe("только что");
    expect(ago(18)).toBe("18 минут назад");
    expect(ago(60 * 3)).toBe("3 часа назад");
    expect(ago(60 * 24 * 2)).toBe("2 дня назад");
    expect(ago(60 * 24 * 21)).toBe("3 недели назад");
    expect(ago(60 * 24 * 90)).toBe("3 месяца назад");
  });
  it("время из будущего не уходит в минус", () => {
    expect(ago(-30)).toBe("только что");
  });
});

describe("formatWhen", () => {
  const now = new Date(2026, 9, 23, 15, 0);
  it("сегодня, вчера и дата", () => {
    expect(formatWhen(new Date(2026, 9, 23, 3, 47).toISOString(), now)).toBe("сегодня в 3:47");
    expect(formatWhen(new Date(2026, 9, 22, 21, 10).toISOString(), now)).toBe("вчера в 21:10");
    expect(formatWhen(new Date(2026, 9, 12, 9, 5).toISOString(), now)).toBe("12 октября в 9:05");
    expect(formatWhen(undefined, now)).toBe("");
  });
});

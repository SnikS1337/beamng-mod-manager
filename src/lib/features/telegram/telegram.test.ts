import { describe, expect, it } from "vitest";
import { isTelegramPost, planUpdateAll, splitForBatch, telegramLeftText } from "./telegram";

describe("посты Telegram-канала", () => {
  it("узнаёт пост канала", () => {
    expect(isTelegramPost("https://t.me/m0dsbeamng/18360")).toBe(true);
    expect(isTelegramPost("https://t.me/s/m0dsbeamng/18360")).toBe(true);
    expect(isTelegramPost("https://T.ME/M0dsBeamNG/5?single")).toBe(true);
  });

  it("не путает с чужим и с не постом", () => {
    expect(isTelegramPost("https://t.me/m0dsbeamng")).toBe(false);
    expect(isTelegramPost("https://t.me/s/m0dsbeamng?q=audi")).toBe(false);
    expect(isTelegramPost("https://t.me/other/1")).toBe(false);
    expect(isTelegramPost("https://t.me/m0dsbeamng/0")).toBe(false);
    expect(isTelegramPost("https://t.me/m0dsbeamng/12a")).toBe(false);
    expect(isTelegramPost("https://evil.example/m0dsbeamng/1")).toBe(false);
    expect(isTelegramPost("https://t.me.evil.example/m0dsbeamng/1")).toBe(false);
    expect(isTelegramPost("tg://resolve?domain=m0dsbeamng&post=1")).toBe(false);
    expect(isTelegramPost("не адрес")).toBe(false);
  });

  it("«Обновить все» отделяет моды из Telegram", () => {
    const split = splitForBatch([
      { pageUrl: "https://www.modland.net/x" },
      { pageUrl: "https://t.me/m0dsbeamng/1" },
      { pageUrl: "https://stmods.org/y" },
    ]);
    expect(split.window.map((item) => item.pageUrl)).toEqual(["https://www.modland.net/x", "https://stmods.org/y"]);
    expect(split.telegram).toHaveLength(1);
    expect(splitForBatch([])).toEqual({ window: [], telegram: [] });
  });

  it("считает моды по-русски", () => {
    expect(telegramLeftText(1)).toContain("1 мод ");
    expect(telegramLeftText(3)).toContain("3 мода");
    expect(telegramLeftText(5)).toContain("5 модов");
    expect(telegramLeftText(11)).toContain("11 модов");
    expect(telegramLeftText(21)).toContain("21 мод ");
    expect(telegramLeftText(12)).toContain("12 модов");
  });

  it("«Обновить все»: окно — по сайтам, Telegram — по одному", () => {
    const site = (n: number) => ({ pageUrl: `https://stmods.org/${n}` });
    const tg = (n: number) => ({ pageUrl: `https://t.me/m0dsbeamng/${n}` });
    expect(planUpdateAll([])).toEqual({ single: null, batch: [], telegramLeft: 0 });
    expect(planUpdateAll([site(1), site(2), tg(3)])).toEqual({ single: null, batch: [site(1), site(2)], telegramLeft: 1 });
    expect(planUpdateAll([site(1), tg(2), tg(3)])).toEqual({ single: site(1), batch: [], telegramLeft: 2 });
    expect(planUpdateAll([tg(2), tg(3)])).toEqual({ single: tg(2), batch: [], telegramLeft: 1 });
    expect(planUpdateAll([tg(2)])).toEqual({ single: tg(2), batch: [], telegramLeft: 0 });
  });
});

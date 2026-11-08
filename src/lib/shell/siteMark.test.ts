import { describe, expect, it } from "vitest";
import { KNOWN_SITES, siteMark } from "./siteMark";

function luminance(hex: string): number {
  const channel = (offset: number) => {
    const value = parseInt(hex.slice(offset, offset + 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

const contrastWithWhite = (hex: string) => 1.05 / (luminance(hex) + 0.05);

describe("значки сайтов", () => {
  it("у каждого сайта свои буквы и свой цвет", () => {
    const marks = KNOWN_SITES.map((id) => siteMark(id));
    expect(new Set(marks.map((mark) => mark.letters)).size).toBe(marks.length);
    expect(new Set(marks.map((mark) => mark.color)).size).toBe(marks.length);
    expect(siteMark("modland")).toEqual({ letters: "ML", color: "#2f6aa8" });
  });

  it("белые буквы читаются на любом цвете", () => {
    for (const id of [...KNOWN_SITES, "новый-сайт"]) {
      expect(contrastWithWhite(siteMark(id).color), id).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("новый сайт — две буквы и нейтральный цвет; пустое имя не ломает", () => {
    expect(siteMark("nexusmods").letters).toBe("NE");
    expect(siteMark("ru-mods").letters).toBe("RU");
    expect(siteMark("").letters).toBe("?");
    expect(siteMark("--").letters).toBe("?");
    expect(siteMark("x").letters).toBe("X");
  });
});

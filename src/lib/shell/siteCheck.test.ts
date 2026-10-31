import { describe, expect, it } from "vitest";
import { needsPassButton, passButton } from "./siteCheck";

describe("кнопка «Пройти проверку»", () => {
  it("«Проходим проверку…» — только у нажатого сайта", () => {
    expect(passButton("modland", "modland", "Modland")).toEqual({
      text: "Проходим проверку…",
      disabled: true,
    });
    expect(passButton("modland", "modhub", "ModHub")).toEqual({
      text: "Пройти проверку ModHub",
      disabled: true,
    });
    expect(passButton(null, "modhub", "ModHub")).toEqual({
      text: "Пройти проверку ModHub",
      disabled: false,
    });
  });
});

describe("needsPassButton", () => {
  it("сайт без модов, который пускает, кнопку не показывает", () => {
    // Было: «ни одного мода» всегда с кнопкой — окно открывалось и сразу закрывалось.
    expect(needsPassButton({ tone: "off" }, true)).toBe(false);
    expect(needsPassButton({ tone: "ok" }, true)).toBe(false);
  });
  it("кнопка — когда сайт просит проверку и её можно пройти", () => {
    expect(needsPassButton({ tone: "warn" }, true)).toBe(true);
    expect(needsPassButton({ tone: "warn" }, false)).toBe(false);
  });
});

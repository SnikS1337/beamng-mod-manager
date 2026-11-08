import { describe, expect, it } from "vitest";
import { needsPassButton, passButton, rerunAfterPass, startPassing, stopPassing } from "./siteCheck";

describe("кнопка «Пройти проверку»", () => {
  it("«Проходим проверку…» — только у нажатого сайта", () => {
    expect(passButton(["modland"], "modland", "Modland")).toEqual({
      text: "Проходим проверку…",
      disabled: true,
    });
    expect(passButton([], "modhub", "ModHub")).toEqual({
      text: "Пройти проверку ModHub",
      disabled: false,
    });
  });

  it("пока проходят один сайт, другой тоже можно пройти", () => {
    // Было: окно проверки одно на программу — пока первый сайт долго грузился,
    // кнопка второго была недоступна.
    expect(passButton(["modland"], "modhub", "ModHub").disabled).toBe(false);
    const both = startPassing(startPassing([], "modland"), "modhub");
    expect(both).toEqual(["modland", "modhub"]);
    expect(passButton(both, "modhub", "ModHub").text).toBe("Проходим проверку…");
  });

  it("повторное нажатие не дублирует, закрытое окно освобождает только свой сайт", () => {
    const passing = startPassing(["modland"], "modland");
    expect(passing).toEqual(["modland"]);
    const after = stopPassing(["modland", "modhub"], "modland");
    expect(after).toEqual(["modhub"]);
    expect(passButton(after, "modland", "Modland").disabled).toBe(false);
    expect(stopPassing([], "modhub")).toEqual([]);
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

describe("rerunAfterPass", () => {
  it("в разделе после проверки раздел грузится заново", () => {
    expect(rerunAfterPass("", "vehicle")).toBe("browse");
  });
  it("поиск по тексту повторяется поиском", () => {
    expect(rerunAfterPass("bmw", null)).toBe("search");
    expect(rerunAfterPass("bmw", "vehicle")).toBe("search");
  });
  it("ни поиска, ни раздела — повторять нечего", () => {
    expect(rerunAfterPass("", null)).toBeNull();
  });
});

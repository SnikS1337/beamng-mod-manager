import { describe, expect, it } from "vitest";
import { needsPassButton, passButton, rerunAfterPass, startPassing, stopPassing } from "./siteCheck";

describe("кнопка «Пройти проверку»", () => {
  it("пока окно сайта открыто — кнопка показывает его, а не висит", () => {
    // Было: «Проходим проверку…», недоступная, пока окно пряталось за другим окном.
    expect(passButton(["modland"], "modland", "Modland")).toEqual({
      text: "Показать окно проверки Modland",
      open: true,
    });
    expect(passButton([], "modhub", "ModHub")).toEqual({ text: "Пройти проверку ModHub", open: false });
  });

  it("пока проходят один сайт, другой тоже можно пройти", () => {
    expect(passButton(["modland"], "modhub", "ModHub").open).toBe(false);
    const both = startPassing(startPassing([], "modland"), "modhub");
    expect(both).toEqual(["modland", "modhub"]);
    expect(passButton(both, "modhub", "ModHub").open).toBe(true);
  });

  it("повторное нажатие не дублирует, закрытое окно освобождает только свой сайт", () => {
    const passing = startPassing(["modland"], "modland");
    expect(passing).toEqual(["modland"]);
    const after = stopPassing(["modland", "modhub"], "modland");
    expect(after).toEqual(["modhub"]);
    expect(passButton(after, "modland", "Modland").open).toBe(false);
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

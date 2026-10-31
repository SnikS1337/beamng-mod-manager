import { describe, expect, it } from "vitest";
import { passButton } from "./siteCheck";

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

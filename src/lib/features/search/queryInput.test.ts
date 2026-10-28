import { describe, expect, it } from "vitest";
import { parseSearchInput } from "./queryInput";

describe("операторы поиска", () => {
  it("вынимает автора, тип и сайт из запроса", () => {
    expect(
      parseSearchInput("lada нива автор:Flanje тип:карта сайт:stmods"),
    ).toEqual({
      text: "lada нива",
      author: "Flanje",
      kind: "map",
      source: "stmods",
    });
  });

  it("поддерживает английские и полные имена", () => {
    expect(
      parseSearchInput("bmw author:Someone type:vehicle source:modsgaming.us"),
    ).toEqual({
      text: "bmw",
      author: "Someone",
      kind: "vehicle",
      source: "modsgaming",
    });
  });

  it("оставляет неизвестные операторы обычным текстом", () => {
    expect(parseSearchInput("audi цена:дорого")).toEqual({
      text: "audi цена:дорого",
      author: null,
      kind: null,
      source: null,
    });
  });

  it("не теряет пустые операторы", () => {
    expect(parseSearchInput("автор:")).toEqual({
      text: "",
      author: null,
      kind: null,
      source: null,
    });
  });
});

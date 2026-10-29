import { describe, expect, it } from "vitest";
import { categoryIntent } from "./category";

describe("categoryIntent", () => {
  it("узнаёт разделы по одному слову", () => {
    expect(categoryIntent("Карта")).toBe("map");
    expect(categoryIntent("КАРТЫ")).toBe("map");
    expect(categoryIntent("map")).toBe("map");
    expect(categoryIntent("машины")).toBe("vehicle");
    expect(categoryIntent("car")).toBe("vehicle");
    expect(categoryIntent("дополнения")).toBe("addon");
    expect(categoryIntent("addons")).toBe("addon");
  });

  it("не путает название с разделом", () => {
    expect(categoryIntent("карта зима")).toBeNull();
    expect(categoryIntent("карточка")).toBeNull();
    expect(categoryIntent("машинное масло")).toBeNull();
    expect(categoryIntent("Gambler Hills")).toBeNull();
    expect(categoryIntent("")).toBeNull();
  });

  it("снимает кавычки и знаки", () => {
    expect(categoryIntent("«Карта»!")).toBe("map");
    expect(categoryIntent("  машины? ")).toBe("vehicle");
  });
});

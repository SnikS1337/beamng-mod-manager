import { describe, expect, it } from "vitest";

import { libraryView, staleError } from "./libraryView";

describe("libraryView", () => {
  it("показывает скелет до первой загрузки", () => {
    expect(libraryView({ loading: true, error: null, hasSnapshot: false })).toBe("skeleton");
  });

  it("показывает полную ошибку, если показывать нечего", () => {
    expect(libraryView({ loading: false, error: "нет связи", hasSnapshot: false })).toBe("error");
    expect(staleError({ loading: false, error: "нет связи", hasSnapshot: false })).toBe(false);
  });

  it("оставляет библиотеку при сбое фоновой перезагрузки", () => {
    const input = { loading: false, error: "нет связи", hasSnapshot: true };
    expect(libraryView(input)).toBe("content");
    expect(staleError(input)).toBe(true);
  });

  it("силовая перезагрузка не прячет уже загруженное", () => {
    expect(libraryView({ loading: true, error: null, hasSnapshot: true })).toBe("content");
  });

  it("успешная загрузка без ошибки — обычный список", () => {
    const input = { loading: false, error: null, hasSnapshot: true };
    expect(libraryView(input)).toBe("content");
    expect(staleError(input)).toBe(false);
  });
});

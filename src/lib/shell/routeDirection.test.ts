import { describe, expect, it } from "vitest";
import { routeDirection } from "./routeDirection";

describe("направление смены раздела", () => {
  it("по порядку меню: вниз — снизу, вверх — сверху", () => {
    expect(routeDirection("/", "/updates")).toBe(1);
    expect(routeDirection("/search", "/settings")).toBe(1);
    expect(routeDirection("/settings", "/")).toBe(-1);
    expect(routeDirection("/search", "/updates")).toBe(-1);
  });

  it("хвостовая косая черта и вложенные адреса не путают", () => {
    expect(routeDirection("/settings/", "/updates")).toBe(-1);
    expect(routeDirection("/", "/settings/folders")).toBe(1);
  });

  it("неизвестный раздел — снизу", () => {
    expect(routeDirection(null, "/updates")).toBe(1);
    expect(routeDirection("/download", "/")).toBe(1);
  });
});

import { describe, expect, it } from "vitest";
import { lightSpot } from "./backgroundSpot";

describe("lightSpot", () => {
  it("у каждого раздела своё место, разделы не совпадают", () => {
    const spots = ["/", "/updates", "/search", "/settings"].map((path) => JSON.stringify(lightSpot(path)));
    expect(new Set(spots).size).toBe(4);
  });

  it("косая черта в конце не сбивает", () => {
    expect(lightSpot("/updates/")).toEqual(lightSpot("/updates"));
  });

  it("незнакомый раздел — свет по центру, а не пропадает", () => {
    const spot = lightSpot("/dev");
    expect(spot.power).toBeGreaterThan(0.5);
    expect(spot).toEqual({ x: 50, y: 50, power: 0.85 });
  });

  it("в настройках свет тише, чем в гараже", () => {
    expect(lightSpot("/settings").power).toBeLessThan(lightSpot("/").power);
  });
});

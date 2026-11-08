// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { pop, rollIn, spring } from "./transitions";

describe("пружина", () => {
  it("начинается в нуле, кончается ровно в единице", () => {
    expect(spring(0)).toBe(0);
    expect(spring(1)).toBe(1);
    expect(spring(-1)).toBe(0);
    expect(spring(2)).toBe(1);
  });

  it("перелетает едва заметно и к концу успокаивается", () => {
    const values = Array.from({ length: 201 }, (_, index) => spring(index / 200));
    const peak = Math.max(...values);
    expect(peak).toBeGreaterThan(1.005);
    expect(peak).toBeLessThan(1.025);
    expect(Math.abs(values[190]! - 1)).toBeLessThan(0.002);
    // До перелёта только вперёд: окно не «дёргается» назад в начале.
    const top = values.indexOf(peak);
    for (let index = 1; index <= top; index += 1) {
      expect(values[index]!).toBeGreaterThanOrEqual(values[index - 1]!);
    }
  });

  it("совпадает с --ease-spring в CSS", () => {
    const css: string = readFileSync("src/app.css", "utf8");
    const match = css.match(/--ease-spring:\s*linear\(([^)]*)\)/);
    expect(match).not.toBeNull();
    const points = match![1]!.split(",").map(Number);
    points.forEach((point, index) => {
      expect(spring(index / (points.length - 1))).toBeCloseTo(point, 3);
    });
  });
});

describe("появление окон", () => {
  it("прозрачность не перелетает, сдвиг — пружиной", () => {
    const config = pop({} as Element, { y: 10 });
    const middle = config.css!(0.5, 0.5);
    const opacity = Number(middle.match(/opacity:([\d.]+)/)![1]);
    expect(opacity).toBeLessThanOrEqual(1);
    expect(config.css!(1, 0)).toContain("translateY(0px) scale(1)");
  });
});

describe("версия на карточке", () => {
  it("новая въезжает снизу и встаёт ровно на место", () => {
    const config = rollIn({} as Element);
    expect(config.css!(0, 1)).toContain("translateY(70%)");
    expect(config.css!(1, 0)).toContain("translateY(0%)");
    expect(config.css!(1, 0)).toContain("opacity:1");
  });
});

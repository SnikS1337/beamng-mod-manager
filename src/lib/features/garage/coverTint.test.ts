import { describe, expect, it } from "vitest";
import { coverTint, tintFromPixels } from "./coverTint";

function pixels(...colors: [number, number, number, number][]): number[] {
  return colors.flat();
}

const fill = (count: number, color: [number, number, number, number]) =>
  Array.from({ length: count }, () => color);

describe("оттенок панели по обложке", () => {
  it("красная обложка — красноватый оттенок", () => {
    expect(tintFromPixels(pixels(...fill(4, [200, 30, 30, 255])))).toMatch(/^hsl\(0 \d+% \d+%\)$/);
  });

  it("серые, чёрные и белые обложки оттенка не дают", () => {
    expect(tintFromPixels(pixels(...fill(4, [128, 128, 128, 255])))).toBeNull();
    expect(tintFromPixels(pixels(...fill(4, [3, 3, 3, 255])))).toBeNull();
    expect(tintFromPixels(pixels(...fill(4, [252, 252, 252, 255])))).toBeNull();
  });

  it("прозрачное не считается; совсем прозрачная картинка — без оттенка", () => {
    expect(tintFromPixels(pixels(...fill(4, [0, 0, 255, 0])))).toBeNull();
    const blueOnClear = tintFromPixels(pixels([20, 60, 220, 255], ...fill(3, [255, 0, 0, 10])));
    expect(blueOnClear).toMatch(/^hsl\(2\d\d /);
  });

  it("яркая машина на сером асфальте всё же даёт свой цвет", () => {
    const tint = tintFromPixels(pixels(...fill(12, [110, 110, 110, 255]), ...fill(4, [30, 160, 40, 255])));
    expect(tint).not.toBeNull();
    const hue = Number(tint!.match(/^hsl\((\d+)/)![1]);
    expect(hue).toBeGreaterThan(90);
    expect(hue).toBeLessThan(150);
  });

  it("светлота и насыщенность — в читаемых пределах", () => {
    for (const color of [
      [255, 230, 0, 255],
      [40, 0, 90, 255],
      [255, 0, 255, 255],
    ] as [number, number, number, number][]) {
      const [, s, l] = tintFromPixels(pixels(...fill(4, color)))!.match(/(\d+)% (\d+)%/)!.map(Number);
      expect(s).toBeLessThanOrEqual(75);
      expect(l).toBeGreaterThanOrEqual(42);
      expect(l).toBeLessThanOrEqual(60);
    }
  });

  it("пустые и битые данные — без оттенка", () => {
    expect(tintFromPixels([])).toBeNull();
    expect(tintFromPixels([255, 0, 0])).toBeNull();
  });

  it("без адреса или вне окна — сразу нет", async () => {
    expect(await coverTint(null)).toBeNull();
    expect(await coverTint("http://bmm.localhost/preview/a.png")).toBeNull();
  });
});

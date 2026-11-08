import { describe, expect, it } from "vitest";
import { LatestAnswer, backgroundSleeps, lightSpot } from "./backgroundSpot";

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

describe("фон спит, когда его не видно", () => {
  it("скрыто или свёрнуто — спит; просто без фокуса — нет", () => {
    expect(backgroundSleeps({ hidden: false, minimized: false })).toBe(false);
    expect(backgroundSleeps({ hidden: true, minimized: false })).toBe(true);
    expect(backgroundSleeps({ hidden: false, minimized: true })).toBe(true);
    expect(backgroundSleeps({ hidden: true, minimized: true })).toBe(true);
  });

  it("поздний ответ на старый вопрос не перетирает свежий", () => {
    const answers = new LatestAnswer();
    const first = answers.ask();
    const second = answers.ask();
    expect(answers.isLatest(first)).toBe(false);
    expect(answers.isLatest(second)).toBe(true);
  });
});

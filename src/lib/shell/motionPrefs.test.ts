import { describe, expect, it } from "vitest";
import { motionDuration } from "./motionPrefs";

function root(classes: string[] = []) {
  return { classList: { contains: (name: string) => classes.includes(name) } };
}

describe("motionDuration", () => {
  it("обычный режим оставляет длительность", () => {
    expect(motionDuration(240, root())).toBe(240);
  });

  it("«Быстро» обнуляет JS-анимации", () => {
    expect(motionDuration(240, root(["no-motion"]))).toBe(0);
  });
});

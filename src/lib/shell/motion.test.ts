import { describe, expect, it } from "vitest";
import { canFlow, flowName } from "./motion.svelte";

function doc(classes: string[] = [], options: { vt?: boolean; hidden?: boolean } = {}) {
  const list = new Set(classes);
  return {
    documentElement: { classList: { contains: (name: string) => list.has(name) } },
    startViewTransition: options.vt === false ? undefined : () => ({}),
    visibilityState: options.hidden ? "hidden" : "visible",
  } as unknown as Parameters<typeof canFlow>[0];
}

describe("canFlow", () => {
  it("плавно, только если включены анимации и браузер умеет", () => {
    expect(canFlow(doc())).toBe(true);
    expect(canFlow(doc(["no-motion"]))).toBe(false);
    expect(canFlow(doc([], { vt: false }))).toBe(false);
    expect(canFlow(undefined)).toBe(false);
  });

  it("свёрнутое окно не анимируем", () => {
    expect(canFlow(doc([], { hidden: true }))).toBe(false);
  });
});

describe("flowName", () => {
  it("один и тот же мод — одно имя, разные — разные", () => {
    expect(flowName("mods/a.zip")).toBe(flowName("mods/a.zip"));
    expect(flowName("mods/a.zip")).not.toBe(flowName("mods/b.zip"));
  });

  it("годится как CSS-имя для любых путей", () => {
    for (const id of ["C:\\mods\\Лада 2106.zip", "repo/x (1).zip", "", "🚗"]) {
      expect(flowName(id)).toMatch(/^m-[0-9a-z]+$/);
    }
  });

  it("на 5 000 модов совпадений нет", () => {
    const names = new Set(Array.from({ length: 5000 }, (_, i) => flowName(`mods/bmm_mod_${i}.zip`)));
    expect(names.size).toBe(5000);
  });
});

import { describe, expect, it } from "vitest";
import { STATUS_TONES, kindIcon, statusIcon } from "./tagIcons";
import { modStatus } from "./status";

describe("иконки меток", () => {
  it("у каждого типа своя иконка; неизвестный — общая «мод»", () => {
    const tags = ["vehicle", "map", "script", "sound", "addon"];
    const icons = tags.map((tag) => kindIcon(tag));
    expect(new Set(icons).size).toBe(tags.length);
    expect(kindIcon("level")).toBe(kindIcon("map"));
    expect(kindIcon("mod")).toBe(kindIcon(null));
    expect(kindIcon("непонятное")).toBe(kindIcon(undefined));
    expect(icons).not.toContain(kindIcon("mod"));
  });

  it("у каждого состояния — своя, и все состояния из modStatus покрыты", () => {
    const icons = STATUS_TONES.map((tone) => statusIcon(tone));
    expect(new Set(icons).size).toBe(STATUS_TONES.length);
    const base = { deleted: false, problem: null, update: "none", pending: "none", enabled: true } as const;
    const tones = [
      modStatus({ ...base, deleted: true }),
      modStatus({ ...base, problem: "wrapped" }),
      modStatus({ ...base, update: "likely" }),
      modStatus({ ...base, update: "possible" }),
      modStatus({ ...base, pending: "queued" }),
      modStatus({ ...base, enabled: false }),
    ].map((status) => status!.tone);
    for (const tone of tones) expect(statusIcon(tone)).toBeTruthy();
  });
});

import { describe, expect, it } from "vitest";
import { modStatus } from "./status";

const base = {
  deleted: false,
  problem: null,
  update: "none" as const,
  pending: "none" as const,
  enabled: true,
};

describe("modStatus", () => {
  it("включённый мод без новостей — без метки", () => {
    expect(modStatus(base)).toBeNull();
  });

  it("показывает самое важное", () => {
    expect(modStatus({ ...base, enabled: false })?.text).toBe("Выключен");
    expect(modStatus({ ...base, enabled: false, update: "likely" })?.tone).toBe("update");
    expect(modStatus({ ...base, update: "possible", problem: "wrapped" })?.text).toBe(
      "Игра не загрузит",
    );
    expect(modStatus({ ...base, pending: "queued", enabled: false })?.text).toBe("После игры");
    expect(modStatus({ ...base, deleted: true, problem: "loose" })?.tone).toBe("deleted");
  });
});

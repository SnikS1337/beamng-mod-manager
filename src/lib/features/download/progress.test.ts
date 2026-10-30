import { describe, expect, it } from "vitest";
import { megabytes, progressView, speedText, STALL_AFTER_MS } from "./progress";

const MB = 1024 * 1024;

describe("ход загрузки", () => {
  it("показывает проценты, объём и скорость", () => {
    const view = progressView(
      { received: 78.4 * MB, total: 187 * MB, speed: 3.2 * MB },
      1000,
      1500,
    );
    expect(view.percent).toBe(41);
    expect(view.line).toBe("41 % · 78,4 МБ из 187 МБ · 3,2 МБ/с");
    expect(view.stalled).toBe(false);
  });

  it("без размера файла — только сколько скачано", () => {
    const view = progressView({ received: 5 * MB, total: null, speed: 512 * 1024 }, 0, 0);
    expect(view.percent).toBeNull();
    expect(view.line).toBe("5,00 МБ скачано · 512 КБ/с");
  });

  it("пока скорости нет, её не выдумывает", () => {
    const view = progressView({ received: MB, total: 10 * MB, speed: null }, 0, 0);
    expect(view.line).toBe("10 % · 1,00 МБ из 10,0 МБ");
  });

  it("замечает, что загрузка стоит", () => {
    const view = progressView(
      { received: 20 * MB, total: 100 * MB, speed: 2 * MB },
      0,
      STALL_AFTER_MS + 5000,
    );
    expect(view.stalled).toBe(true);
    expect(view.stallText).toBe("Новых данных нет 15 с");
    expect(view.line).toContain("0 КБ/с");
  });

  it("докачанный файл не считается застрявшим", () => {
    const view = progressView({ received: 100, total: 100, speed: 0 }, 0, 60_000);
    expect(view.stalled).toBe(false);
    expect(view.percent).toBe(100);
  });

  it("единицы читаются", () => {
    expect(megabytes(1.5 * 1024 * MB)).toBe("1,50 ГБ");
    expect(megabytes(250 * MB)).toBe("250 МБ");
    expect(speedText(300 * 1024)).toBe("300 КБ/с");
  });
});

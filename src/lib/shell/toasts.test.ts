import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { readingTime, toasts } from "./toasts.svelte";

describe("тосты", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.runAllTimers();
    vi.useRealTimers();
  });

  it("короткий тост исчезает сам", () => {
    toasts.show("Готово");
    expect(toasts.current?.message).toBe("Готово");
    vi.advanceTimersByTime(3000);
    expect(toasts.current).toBeNull();
  });

  it("новый тост ждёт в очереди и не отнимает «Отменить»", () => {
    const commit = vi.fn();
    toasts.show("Мод удалён", { undo: vi.fn(), commit });
    toasts.show("Нашли обновления");
    expect(toasts.current?.message).toBe("Мод удалён");
    expect(commit).not.toHaveBeenCalled();

    vi.advanceTimersByTime(6000);
    expect(commit).toHaveBeenCalledTimes(1);
    expect(toasts.current?.message).toBe("Нашли обновления");
  });

  it("фоновый тост не закрепляет удаление раньше срока", () => {
    const commit = vi.fn();
    toasts.show("Мод удалён", { undo: vi.fn(), commit });
    toasts.show("Нашли обновления");
    vi.advanceTimersByTime(3000);
    expect(commit).not.toHaveBeenCalled();
    expect(toasts.current?.message).toBe("Мод удалён");
  });

  it("«Отменить» показывает следующее сообщение из очереди", async () => {
    const commit = vi.fn();
    const undo = vi.fn();
    toasts.show("Мод удалён", { undo, commit });
    toasts.show("Нашли обновления");
    await toasts.undo();
    expect(undo).toHaveBeenCalledTimes(1);
    expect(commit).not.toHaveBeenCalled();
    expect(toasts.current?.message).toBe("Нашли обновления");
  });

  it("без действия Ctrl+Z нечего отменять", () => {
    toasts.show("Готово");
    expect(toasts.canUndo).toBe(false);
    vi.advanceTimersByTime(10000);
    toasts.show("Мод удалён", { undo: vi.fn() });
    expect(toasts.canUndo).toBe(true);
  });

  it("«Отменить» отменяет и не закрепляет", async () => {
    const commit = vi.fn();
    const undo = vi.fn();
    toasts.show("Мод удалён", { undo, commit });
    await toasts.undo();
    expect(undo).toHaveBeenCalledTimes(1);
    expect(toasts.current).toBeNull();
    vi.advanceTimersByTime(10000);
    expect(commit).not.toHaveBeenCalled();
  });

  it("ошибка отмены показывается тостом", async () => {
    toasts.show("Мод удалён", {
      undo: () => {
        throw new Error("Файл занят игрой");
      },
    });
    await toasts.undo();
    expect(toasts.current?.message).toBe("Файл занят игрой");
  });

  it("действие с отменой висит дольше", () => {
    toasts.show("Мод удалён", { undo: vi.fn() });
    vi.advanceTimersByTime(5000);
    expect(toasts.current).not.toBeNull();
    vi.advanceTimersByTime(1000);
    expect(toasts.current).toBeNull();
  });

  it("длинный текст висит дольше короткого — успеть прочитать", () => {
    // Было: 3 или 5 секунд на всё, итог проверки в 150 знаков пропадал недочитанным.
    const long =
      "Новых версий нет. Modland, ModHub: нужно пройти проверку на сайте. У 9 модов нет страницы — их не проверяли";
    expect(readingTime("Готово")).toBe(3000);
    expect(readingTime(long)).toBeGreaterThan(8000);
    expect(readingTime("x".repeat(1000))).toBe(15000);
    toasts.show(long);
    vi.advanceTimersByTime(5000);
    expect(toasts.current?.message).toBe(long);
    vi.advanceTimersByTime(readingTime(long));
    expect(toasts.current).toBeNull();
  });

  it("под курсором тост не исчезает", () => {
    toasts.show("Готово");
    vi.advanceTimersByTime(2000);
    toasts.pause();
    vi.advanceTimersByTime(20000);
    expect(toasts.current?.message).toBe("Готово");
    toasts.resume();
    vi.advanceTimersByTime(1400);
    expect(toasts.current?.message).toBe("Готово");
    vi.advanceTimersByTime(200);
    expect(toasts.current).toBeNull();
  });

  it("повтор того же сообщения не встаёт в очередь", () => {
    toasts.show("Мод удалён", { undo: vi.fn() });
    toasts.show("Копия удалена");
    toasts.show("Копия удалена");
    toasts.show("Копия удалена");
    vi.advanceTimersByTime(6000);
    expect(toasts.current?.message).toBe("Копия удалена");
    vi.advanceTimersByTime(3000);
    expect(toasts.current).toBeNull();
  });

  it("при длинной очереди сообщения короче, но не меньше 3 секунд", () => {
    const long = "x".repeat(120);
    toasts.show("Мод удалён", { undo: vi.fn() });
    toasts.show(long);
    toasts.show("второе");
    toasts.show("третье");
    vi.advanceTimersByTime(6000);
    expect(toasts.current?.message).toBe(long);
    vi.advanceTimersByTime(Math.round(readingTime(long) * 0.6));
    expect(toasts.current?.message).toBe("второе");
  });
});

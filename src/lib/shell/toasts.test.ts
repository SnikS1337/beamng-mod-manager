import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toasts } from "./toasts.svelte";

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

  it("новый тост закрепляет прежнее действие, чтобы мод не пропал из «Удалённых»", () => {
    const commit = vi.fn();
    toasts.show("Мод удалён", { undo: vi.fn(), commit });
    toasts.show("Другое");
    expect(commit).toHaveBeenCalledTimes(1);
    expect(toasts.current?.message).toBe("Другое");
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
});

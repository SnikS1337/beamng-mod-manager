import { afterEach, describe, expect, it, vi } from "vitest";
import { LAND_TIMEOUT_MS, arrival } from "./arrival.svelte";

describe("мод прилетает в гараж", () => {
  afterEach(() => vi.useRealTimers());

  it("гаража нет на экране — меняем сразу и без имени перехода", async () => {
    let done = false;
    await arrival.fly("car.zip", () => (done = true));
    expect(done).toBe(true);
    expect(arrival.flying).toBeNull();
    expect(arrival.landedName).toBeNull();
  });

  it("летит строка с этим файлом; имя у карточки — только после посадки", async () => {
    const detach = arrival.attach();
    let flight: Promise<void> | null = null;
    flight = arrival.fly("Car.ZIP", () => {
      expect(arrival.isFlying("car.zip")).toBe(true);
      expect(arrival.isLanded("car.zip"), "старая карточка под окном без имени").toBe(false);
      queueMicrotask(() => arrival.landed("car.zip"));
    });
    await flight;
    expect(arrival.isLanded("CAR.zip")).toBe(true);
    expect(arrival.isLanded("other.zip")).toBe(false);
    detach();
  });

  it("гараж не нашёл карточку — переход не ждёт, имени нет", async () => {
    const detach = arrival.attach();
    await arrival.fly("gone.zip", () => queueMicrotask(() => arrival.landed()));
    expect(arrival.landedName).toBeNull();
    detach();
  });

  it("гараж молчит — экран отпускаем по тайм-ауту", async () => {
    vi.useFakeTimers();
    const detach = arrival.attach();
    let finished = false;
    const flight = arrival.fly("slow.zip", () => {}).then(() => (finished = true));
    await vi.advanceTimersByTimeAsync(LAND_TIMEOUT_MS - 1);
    expect(finished).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    await flight;
    expect(finished).toBe(true);
    detach();
  });

  it("гараж закрылся посреди полёта — не ждём тайм-аута", async () => {
    vi.useFakeTimers();
    const detach = arrival.attach();
    let finished = false;
    const flight = arrival.fly("car.zip", () => queueMicrotask(detach)).then(() => (finished = true));
    await vi.advanceTimersByTimeAsync(0);
    await flight;
    expect(finished).toBe(true);
    expect(arrival.canLand).toBe(false);
    detach();
    expect(arrival.canLand, "повторная отписка не уводит счёт в минус").toBe(false);
  });

  it("новый полёт снимает имя прошлой посадки до начала перехода", async () => {
    const detach = arrival.attach();
    await arrival.fly("first.zip", () => queueMicrotask(() => arrival.landed("first.zip")));
    expect(arrival.isLanded("first.zip")).toBe(true);
    await arrival.fly("second.zip", () => {
      expect(arrival.isLanded("first.zip")).toBe(false);
      queueMicrotask(() => arrival.landed("second.zip"));
    });
    expect(arrival.isLanded("second.zip")).toBe(true);
    detach();
  });
});

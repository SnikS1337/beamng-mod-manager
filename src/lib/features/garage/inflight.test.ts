import { describe, expect, it } from "vitest";
import { InFlight, liveCard } from "./inflight";

describe("переключение мода, пока ждём ответа", () => {
  it("второй клик до ответа пропускается, после ответа — нет", () => {
    const flight = new InFlight();
    expect(flight.start("a")).toBe(true);
    expect(flight.start("a")).toBe(false);
    expect(flight.start("b"), "другой мод не ждёт").toBe(true);
    flight.done("a");
    expect(flight.has("a")).toBe(false);
    expect(flight.start("a")).toBe(true);
  });

  it("правка идёт в карточку из текущего списка, а не в прежний объект", () => {
    const old = { id: "a", enabled: true };
    const fresh = { id: "a", enabled: true };
    expect(liveCard([fresh], old)).toBe(fresh);
    expect(liveCard([], old), "мода в списке нет — прежний объект").toBe(old);
    expect(liveCard(undefined, old)).toBe(old);
  });
});

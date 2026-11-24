import { beforeEach, describe, expect, it, vi } from "vitest";

const state = vi.hoisted(() => ({
  listeners: new Map<string, (event: unknown) => void>(),
  listenCalls: 0,
  unlistenCalls: 0,
}));

vi.mock("@tauri-apps/api/core", () => ({ isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (name: string, handler: (event: unknown) => void) => {
    state.listenCalls += 1;
    state.listeners.set(name, handler);
    return () => { state.unlistenCalls += 1; state.listeners.delete(name); };
  },
}));

import { stableListen } from "./stableListen";

describe("стабильные Tauri-подписки", () => {
  beforeEach(() => {
    state.listeners.clear();
    state.listenCalls = 0;
    state.unlistenCalls = 0;
  });

  it("несколько компонентов используют один listener", async () => {
    const first = vi.fn();
    const second = vi.fn();
    const stopFirst = await stableListen("test://event", first);
    const stopSecond = await stableListen("test://event", second);
    expect(state.listenCalls).toBe(1);
    state.listeners.get("test://event")?.({ payload: "ok" });
    expect(first).toHaveBeenCalledTimes(1);
    expect(second).toHaveBeenCalledTimes(1);
    stopFirst();
    stopSecond();
    expect(state.unlistenCalls).toBe(0);
  });
});
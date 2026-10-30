import { describe, expect, it } from "vitest";
import { SaveQueue } from "./saveQueue";

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((done) => (resolve = done));
  return { promise, resolve };
}

describe("очередь сохранений настроек", () => {
  it("правка во время сохранения сохраняется следом, а idle ждёт обе", async () => {
    // Раньше: переключили настройку, сразу вторую и закрыли окно — вторая терялась.
    const gates = [deferred(), deferred()];
    let calls = 0;
    const queue = new SaveQueue(async () => {
      const gate = gates[calls];
      calls += 1;
      await gate?.promise;
    });
    void queue.request();
    void queue.request();
    let idle = false;
    const waiting = queue.idle().then(() => (idle = true));
    gates[0]!.resolve();
    await Promise.resolve();
    await new Promise((done) => setTimeout(done, 0));
    expect(calls).toBe(2);
    expect(idle).toBe(false);
    gates[1]!.resolve();
    await waiting;
    expect(idle).toBe(true);
    expect(queue.busy).toBe(false);
  });

  it("ошибка сохранения не блокирует следующие", async () => {
    let calls = 0;
    const queue = new SaveQueue(async () => {
      calls += 1;
      if (calls === 1) throw new Error("диск");
    });
    await expect(queue.request()).rejects.toThrow("диск");
    await queue.request();
    expect(calls).toBe(2);
  });
});

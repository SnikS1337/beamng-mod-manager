import { describe, expect, it } from "vitest";
import { incomingQueue } from "./queue.svelte";

describe("очередь архивов", () => {
  it("не берёт один и тот же архив дважды и считает ревизии", () => {
    const before = incomingQueue.revision;
    incomingQueue.add(["C:/a.zip", "C:/a.zip", "C:/b.zip"]);
    expect(incomingQueue.count).toBe(2);
    expect(incomingQueue.revision).toBe(before + 1);
    incomingQueue.add(["C:/b.zip"]);
    expect(incomingQueue.count).toBe(2);
    expect(incomingQueue.revision).toBe(before + 1);
    incomingQueue.add(["C:/c.zip"], true);
    expect(incomingQueue.shift()).toEqual({ path: "C:/a.zip", downloaded: false });
    expect(incomingQueue.shift()).toEqual({ path: "C:/b.zip", downloaded: false });
    expect(incomingQueue.shift()).toEqual({ path: "C:/c.zip", downloaded: true });
    expect(incomingQueue.shift()).toBeUndefined();
  });
});

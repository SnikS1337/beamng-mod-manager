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

  it("один файл с разными разделителями и регистром — один раз", () => {
    // Наблюдатель загрузок и перетаскивание могут дать один и тот же путь
    // по-разному: раньше архив попадал в очередь дважды и окно открывалось два раза.
    incomingQueue.add(["C:/Mods/Car.zip", "C:\\Mods\\Car.zip", "c:\\mods\\car.zip"]);
    expect(incomingQueue.count).toBe(1);
    expect(incomingQueue.shift()?.path).toBe("C:/Mods/Car.zip");
    expect(incomingQueue.shift()).toBeUndefined();
  });

  it("разные файлы не схлопываются", () => {
    incomingQueue.add(["C:/Mods/Car.zip", "C:/Mods/Car2.zip"]);
    expect(incomingQueue.count).toBe(2);
    incomingQueue.shift();
    incomingQueue.shift();
  });
});

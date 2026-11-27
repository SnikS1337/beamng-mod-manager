import { describe, expect, it } from "vitest";
import { libraryLoad, modDelete, modPreview, modToggle, modUndelete, modVersions } from "./library";

describe("демо: превью", () => {
  it("кеш ограничен и повторяет недавний запрос", async () => {
    for (let index = 0; index < 300; index += 1) await modPreview(`mod-${index}`, `key-${index}`);
    const recent = modPreview("mod-299", "key-299");
    expect(modPreview("mod-299", "key-299")).toBe(recent);
    await recent;
  });

  it("недавний ключ не вытесняется первым", async () => {
    for (let index = 0; index < 300; index += 1) {
      await modPreview(`flush-${index * 3}`, `flush-key-${index}`);
    }
    const first = modPreview("lru-0", "lru-key-0");
    const second = modPreview("lru-3", "lru-key-1");
    for (let index = 2; index < 256; index += 1) {
      await modPreview(`lru-${index * 3}`, `lru-key-${index}`);
    }
    expect(modPreview("lru-0", "lru-key-0")).toBe(first);
    for (let index = 256; index < 270; index += 1) {
      await modPreview(`lru-${index * 3}`, `lru-key-${index}`);
    }
    expect(modPreview("lru-0", "lru-key-0")).toBe(first);
    expect(modPreview("lru-3", "lru-key-1")).not.toBe(second);
    await Promise.all([first, second]);
  });
});

describe("демо: переключение и отмена", () => {
  it("toggle и Undo меняют одну и ту же карточку", async () => {
    const snapshot = await libraryLoad();
    const card = snapshot.mods.find((mod) => mod.id === "mock-1")!;
    const before = card.enabled;
    const counts = { ...snapshot.counts };
    try {
      expect(await modToggle(card.id, !before)).toEqual({
        outcome: "done",
        enabled: !before,
        message: null,
      });
      expect(card.enabled).toBe(!before);
      expect(snapshot.counts).toEqual({ ...counts, enabled: counts.enabled - 1 });
      expect((await libraryLoad()).mods.find((mod) => mod.id === card.id)?.enabled).toBe(!before);

      await modToggle(card.id, before);
      expect(card.enabled).toBe(before);
      expect(snapshot.counts).toEqual(counts);
    } finally {
      await modToggle(card.id, before);
    }
  });
});

it("демо: список версий содержит текущий файл для Undo", async () => {
  const snapshot = await libraryLoad();
  const card = snapshot.mods.find((mod) => mod.id === "mock-1")!;

  const rows = await modVersions(card.id);

  expect(rows.some((row) => row.fileName === card.fileName)).toBe(true);
});

// Демо-версия должна вести себя как программа: отменённое удаление возвращает
// карточку на её место, а не в конец списка.
describe("демо: удаление и отмена", () => {
  const order = async () => (await libraryLoad()).mods.map((mod) => mod.id);

  it("одна карточка возвращается на своё место", async () => {
    const before = await order();
    const id = before[2]!;
    await modDelete(id);
    expect(await order()).not.toContain(id);
    await modUndelete(id);
    expect(await order()).toEqual(before);
  });

  it("первая карточка — снова первая", async () => {
    const before = await order();
    await modDelete(before[0]!);
    await modUndelete(before[0]!);
    expect(await order()).toEqual(before);
  });

  it("соседние, удалённые пачкой и возвращённые по порядку, встают как были", async () => {
    const before = await order();
    const ids = before.slice(3, 6);
    for (const id of ids) await modDelete(id);
    for (const id of ids) await modUndelete(id);
    expect(await order()).toEqual(before);
  });

  it("возвращённые в обратном порядке — тоже на месте", async () => {
    const before = await order();
    const ids = before.slice(7, 9);
    for (const id of ids) await modDelete(id);
    for (const id of [...ids].reverse()) await modUndelete(id);
    expect(await order()).toEqual(before);
  });
});

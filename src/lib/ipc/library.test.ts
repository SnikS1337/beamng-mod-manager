import { describe, expect, it } from "vitest";
import { libraryLoad, modDelete, modUndelete } from "./library";

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

import { describe, expect, it } from "vitest";
import { canFlow, flowName } from "./motion.svelte";

function doc(classes: string[] = [], options: { vt?: boolean; hidden?: boolean } = {}) {
  const list = new Set(classes);
  return {
    documentElement: { classList: { contains: (name: string) => list.has(name) } },
    startViewTransition: options.vt === false ? undefined : () => ({}),
    visibilityState: options.hidden ? "hidden" : "visible",
  } as unknown as Parameters<typeof canFlow>[0];
}

describe("canFlow", () => {
  it("плавно, только если включены анимации и браузер умеет", () => {
    expect(canFlow(doc())).toBe(true);
    expect(canFlow(doc(["no-motion"]))).toBe(false);
    expect(canFlow(doc([], { vt: false }))).toBe(false);
    expect(canFlow(undefined)).toBe(false);
  });

  it("свёрнутое окно не анимируем", () => {
    expect(canFlow(doc([], { hidden: true }))).toBe(false);
  });
});

describe("flowName", () => {
  it("один и тот же мод — одно имя, разные — разные", () => {
    expect(flowName("mods/a.zip")).toBe(flowName("mods/a.zip"));
    expect(flowName("mods/a.zip")).not.toBe(flowName("mods/b.zip"));
  });

  it("годится как CSS-имя для любых путей", () => {
    for (const id of ["C:\\mods\\Лада 2106.zip", "repo/x (1).zip", "", "🚗"]) {
      expect(flowName(id)).toMatch(/^m-[0-9a-z]+-[0-9a-z]+$/);
    }
  });

  it("на 5 000 модов совпадений нет", () => {
    const names = new Set(Array.from({ length: 5000 }, (_, i) => flowName(`mods/bmm_mod_${i}.zip`)));
    expect(names.size).toBe(5000);
  });
});

describe("motion.run и settled", () => {
  function fakeDocument() {
    const classes = new Set<string>();
    const transitions: { finish: () => void; skipped: boolean }[] = [];
    const document = {
      documentElement: {
        classList: {
          contains: (name: string) => classes.has(name),
          add: (name: string) => classes.add(name),
          remove: (name: string) => classes.delete(name),
        },
      },
      visibilityState: "visible",
      startViewTransition(update: () => Promise<void>) {
        let finish!: () => void;
        const finished = new Promise<void>((resolve) => (finish = resolve));
        const updateCallbackDone = Promise.resolve().then(update);
        const entry = { finish, skipped: false };
        transitions.push(entry);
        return {
          finished,
          updateCallbackDone,
          ready: Promise.resolve(),
          skipTransition: () => {
            entry.skipped = true;
            finish();
          },
        };
      },
    };
    return { document, classes, transitions };
  }

  it("класс области стоит, пока переход идёт; settled — когда доиграл", async () => {
    const { vi } = await import("vitest");
    const fake = fakeDocument();
    vi.stubGlobal("document", fake.document);
    try {
      const { motion } = await import("./motion.svelte");
      let applied = false;
      await motion.run(() => {
        applied = true;
      }, "flow-test");
      expect(applied).toBe(true);
      expect(fake.classes.has("flow-test")).toBe(true);
      let settled = false;
      void motion.settled.then(() => (settled = true));
      await Promise.resolve();
      expect(settled).toBe(false);
      fake.transitions[0]!.finish();
      await motion.settled;
      await Promise.resolve();
      expect(settled).toBe(true);
      expect(fake.classes.has("flow-test")).toBe(false);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("новый переход обрывает недоигранный; классы снимаются по счёту", async () => {
    const { vi } = await import("vitest");
    const fake = fakeDocument();
    vi.stubGlobal("document", fake.document);
    try {
      const { motion } = await import("./motion.svelte");
      await motion.run(() => {}, "flow-a");
      await motion.run(() => {}, "flow-a");
      expect(fake.transitions.at(-2)!.skipped).toBe(true);
      await Promise.resolve();
      expect(fake.classes.has("flow-a"), "второй ещё идёт").toBe(true);
      fake.transitions.at(-1)!.finish();
      await motion.settled;
      await Promise.resolve();
      expect(fake.classes.has("flow-a")).toBe(false);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});

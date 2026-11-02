// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";
import { anyOverlay, enterOverlay, isTopOverlay, takeEscape } from "./overlays";

function escape(): KeyboardEvent {
  let prevented = false;
  return {
    key: "Escape",
    get defaultPrevented() {
      return prevented;
    },
    preventDefault() {
      prevented = true;
    },
  } as unknown as KeyboardEvent;
}

const opened: { leave: () => void }[] = [];
function open() {
  const overlay = enterOverlay();
  opened.push(overlay);
  return overlay;
}

describe("Escape и окна друг над другом", () => {
  afterEach(() => {
    while (opened.length) opened.pop()!.leave();
  });

  it("одно нажатие закрывает только верхнее окно", () => {
    // Было: приветствие и палитра закрывались вместе.
    const welcome = open();
    const palette = open();
    const event = escape();
    // Окна получают событие в любом порядке — забирает только верхнее.
    expect(takeEscape(event, welcome.id)).toBe(false);
    expect(takeEscape(event, palette.id)).toBe(true);
    expect(takeEscape(event, welcome.id)).toBe(false);
    palette.leave();
    expect(isTopOverlay(welcome.id)).toBe(true);
    expect(takeEscape(escape(), welcome.id)).toBe(true);
  });

  it("забранное нажатие не достаётся следующему верхнему", () => {
    // Палитра закрылась в том же событии — приветствие под ней не должно его видеть.
    const welcome = open();
    const palette = open();
    const event = escape();
    expect(takeEscape(event, palette.id)).toBe(true);
    palette.leave();
    expect(takeEscape(event, welcome.id)).toBe(false);
  });

  it("закрытие нижнего окна не ломает стопку", () => {
    const first = open();
    const second = open();
    first.leave();
    expect(isTopOverlay(second.id)).toBe(true);
    second.leave();
    expect(anyOverlay()).toBe(false);
    expect(takeEscape(escape(), second.id)).toBe(false);
  });
});

describe("окна поверх программы слушают Escape через стопку", () => {
  for (const file of [
    "./CommandPalette.svelte",
    "./WelcomeTour.svelte",
    "../features/incoming/IncomingHost.svelte",
  ]) {
    it(file, () => {
      const text: string = readFileSync(new URL(file, import.meta.url), "utf8");
      expect(text).toContain("enterOverlay()");
      expect(text).toContain("takeEscape(event, overlay)");
    });
  }

  it("гараж и поиск не трогают Escape, пока сверху окно", () => {
    for (const file of [
      "../features/garage/GaragePage.svelte",
      "../../routes/(app)/search/+page.svelte",
    ]) {
      const text: string = readFileSync(new URL(file, import.meta.url), "utf8");
      expect(text, file).toContain("anyOverlay()");
    }
  });
});

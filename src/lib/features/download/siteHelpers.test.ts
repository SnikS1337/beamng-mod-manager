// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

/**
 * Скрипт, который программа встраивает в страницу мода в окне скачивания. Новые окна
 * там запрещены: ссылка «Download» с target=_blank на ModsGaming и «Continue to
 * External Site» на Modland не делали ничего. Проверяем сам скрипт на поддельной
 * странице: щёлкаем по ссылкам и смотрим, куда ушло окно.
 */
const source: string = readFileSync(
  new URL("../../../../src-tauri/src/commands/site_helpers.js", import.meta.url),
  "utf8",
).replace("__FILE_HOSTS__", JSON.stringify(["modsfire.com", "mediafire.com"]));

type Listener = (event: unknown) => void;

function page(href: string) {
  const listeners: Listener[] = [];
  const location = new URL(href) as URL & { href: string };
  const opened: string[] = [];
  const window = {
    open: (url: string) => {
      opened.push(url);
      return {};
    },
  } as { open: (url?: string, name?: string, features?: string) => unknown };
  const document = { addEventListener: (_type: string, fn: Listener) => listeners.push(fn) };
  new Function("window", "document", "location", "URL", source)(window, document, location, URL);
  const click = (attrs: Record<string, string>, text = "") => {
    let prevented = false;
    const anchor = {
      getAttribute: (name: string) => attrs[name] ?? null,
      textContent: text,
      href: attrs.href,
    };
    const target = {
      closest: (selector: string) =>
        selector.startsWith("a[") && /_blank|_new/.test(attrs.target ?? "") ? anchor : null,
    };
    const before = location.href;
    for (const fn of listeners) fn({ target, preventDefault: () => (prevented = true) });
    return { prevented, moved: location.href !== before ? location.href : null };
  };
  return { click, location, window, opened };
}

describe("переходы «в новое окно» в окне скачивания", () => {
  it("Download на ModsGaming (свой адрес /load/…-20) — в этом же окне", () => {
    const site = page("https://www.modsgaming.us/load/beamng/others/x/76-1-0-122983");
    const result = site.click({ href: "/load/0-0-1-122983-20", target: "_blank", title: "Download" }, "Download");
    expect(result.prevented).toBe(true);
    expect(result.moved).toBe("https://www.modsgaming.us/load/0-0-1-122983-20");
  });

  it("«Continue to External Site» на файловый хостинг — в этом же окне", () => {
    const site = page("https://www.modland.net/beamng.drive-mods/cars/nissan-silvia-s14-31.html");
    const result = site.click(
      { href: "https://modsfire.com/667i3gKghVvtoDV", target: "_blank" },
      "Continue to External Site",
    );
    expect(result.moved).toBe("https://modsfire.com/667i3gKghVvtoDV");
  });

  it("переадресация своего сайта на хостинг (?url=…) — тоже", () => {
    const site = page("https://www.modland.net/mod.html");
    const result = site.click({
      href: "/go?url=https%3A%2F%2Fwww.mediafire.com%2Ffile%2Fabc",
      target: "_blank",
    });
    expect(result.moved).toBe("https://www.modland.net/go?url=https%3A%2F%2Fwww.mediafire.com%2Ffile%2Fabc");
  });

  it("реклама и чужие сайты в новом окне не трогаем", () => {
    const site = page("https://www.modsgaming.us/load/beamng/x");
    expect(site.click({ href: "https://ads.example.com/promo", target: "_blank" }, "Download now").moved).toBeNull();
    expect(site.click({ href: "/forum/12", target: "_blank" }, "Форум").moved).toBeNull();
    expect(site.click({ href: "javascript:alert(1)", target: "_blank" }, "Download").moved).toBeNull();
    // Обычная ссылка без нового окна — браузер ведёт её сам.
    expect(site.click({ href: "https://modsfire.com/x" }).prevented).toBe(false);
  });

  it("window.open к файлу — в этом окне, остальное — как было", () => {
    const site = page("https://www.modland.net/mod.html");
    expect(site.window.open("https://modsfire.com/abc")).toBeNull();
    expect(site.location.href).toBe("https://modsfire.com/abc");
    site.window.open("https://ads.example.com/");
    expect(site.opened).toEqual(["https://ads.example.com/"]);
  });
});

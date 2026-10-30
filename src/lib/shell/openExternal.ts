import { openUrl } from "@tauri-apps/plugin-opener";
import { safeExternalUrl } from "#lib/url.js";
import { toasts } from "./toasts.svelte";

/**
 * Открывает страницу в системном браузере. Только http и https: адрес приходит с сайтов
 * модов, и `file:` или `javascript:` оттуда открывать нельзя. `false` — не открыли.
 */
export async function openExternal(raw: string): Promise<boolean> {
  const url = safeExternalUrl(raw);
  if (!url) {
    toasts.show("Эту ссылку открыть нельзя: нужен адрес http или https");
    return false;
  }
  try {
    await openUrl(url);
    return true;
  } catch {
    try {
      // С "noopener" window.open по стандарту возвращает null и при успехе: открываем
      // без него и сразу обрываем связь со страницей — так видно, открылось ли окно.
      const opened = window.open(url, "_blank");
      if (opened) {
        opened.opener = null;
        return true;
      }
    } catch {
      // Браузер запретил и запасной путь.
    }
    toasts.show("Не удалось открыть ссылку в браузере");
    return false;
  }
}

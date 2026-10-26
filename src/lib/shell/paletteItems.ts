import type { ModCardDto } from "#lib/ipc/library.js";
import { matchesQuery, words } from "#lib/features/garage/format.js";
import { typeLabel, typeTag } from "#lib/features/garage/view.js";

export type PaletteAction =
  | { kind: "mod"; id: string }
  | { kind: "go"; href: string }
  | { kind: "search"; query: string }
  | { kind: "check" }
  | { kind: "archive" }
  | { kind: "filter"; filter: "updates" | "unchecked" | "disabled" };

export interface PaletteItem {
  id: string;
  group: "Ваши моды" | "Команды" | "Разделы";
  label: string;
  hint: string;
  tag?: { text: string; tone: string };
  keys?: string;
  action: PaletteAction;
}

const COMMANDS: Omit<PaletteItem, "id" | "group">[] = [
  { label: "Проверить обновления сейчас", hint: "все сайты", action: { kind: "check" } },
  { label: "Установить из архива…", hint: "zip, rar, 7z", action: { kind: "archive" } },
  { label: "Показать моды с обновлением", hint: "фильтр гаража", action: { kind: "filter", filter: "updates" } },
  { label: "Показать моды без страницы", hint: "фильтр гаража", action: { kind: "filter", filter: "unchecked" } },
  { label: "Показать выключенные моды", hint: "фильтр гаража", action: { kind: "filter", filter: "disabled" } },
];

const SECTIONS: Omit<PaletteItem, "id" | "group">[] = [
  { label: "Гараж", hint: "ваши моды", action: { kind: "go", href: "/" } },
  { label: "Обновления", hint: "найденные версии", action: { kind: "go", href: "/updates" } },
  { label: "Поиск на сайтах", hint: "Modland, STMods, ModsGaming", action: { kind: "go", href: "/search" } },
  { label: "Настройки", hint: "оформление, сайты, проверка", action: { kind: "go", href: "/settings" } },
  { label: "Папки игры и модов", hint: "настройки", action: { kind: "go", href: "/settings?section=folders" } },
];

const MOD_LIMIT = 6;

/** Пункты палитры для запроса: моды по имени, команды и разделы по словам. */
export function paletteItems(query: string, mods: ModCardDto[]): PaletteItem[] {
  const text = query.trim();
  const items: PaletteItem[] = [];
  if (text) {
    const found = mods.filter((mod) => !mod.deleted && !mod.duplicateOf && matchesQuery(mod, text));
    for (const mod of found.slice(0, MOD_LIMIT)) {
      const tag = typeTag(mod);
      items.push({
        id: `mod:${mod.id}`,
        group: "Ваши моды",
        label: mod.name,
        hint: [mod.version, mod.enabled ? "включён" : "выключен"].filter(Boolean).join(" · "),
        tag: { text: typeLabel(tag), tone: tag },
        action: { kind: "mod", id: mod.id },
      });
    }
  }
  const commands = COMMANDS.filter((item) => matchesWords(item.label, text));
  if (text) {
    commands.unshift({
      label: `Найти «${text}» на сайтах`,
      hint: "Modland · STMods · ModsGaming",
      action: { kind: "search", query: text },
    });
  }
  commands.forEach((item, index) => items.push({ ...item, id: `cmd:${index}`, group: "Команды" }));
  SECTIONS.filter((item) => matchesWords(`${item.label} ${item.hint}`, text)).forEach((item, index) =>
    items.push({ ...item, id: `go:${index}`, group: "Разделы" }),
  );
  return items;
}

function matchesWords(label: string, query: string): boolean {
  const needles = words(query);
  if (needles.length === 0) return true;
  const haystack = words(label);
  return needles.every((needle) => haystack.some((word) => word.startsWith(needle)));
}

import type { ModCardDto } from "#lib/ipc/library.js";

/** Версия на карточке: «1.0 → 1.2», если на сайте новее; `fresh` — подсветить. */
export function versionLine(
  mod: Pick<ModCardDto, "version" | "update" | "updateVersion">,
): { text: string; fresh: boolean } | null {
  const next = mod.update !== "none" ? mod.updateVersion?.trim() : null;
  const current = mod.version?.trim() || null;
  if (next && next !== current) {
    return { text: current ? `${current} → ${next}` : `→ ${next}`, fresh: true };
  }
  return current ? { text: current, fresh: false } : null;
}

/**
 * Мод под другую версию игры: «для 0.38». Голое «0.38» на карточке читалось как
 * ещё одна версия мода.
 */
export function compatNote(mod: Pick<ModCardDto, "compat">): { text: string; title: string } | null {
  const { modFor, game, differs } = mod.compat;
  if (!differs || !modFor) return null;
  return {
    text: `для ${modFor}`,
    title: game ? `Сделан для игры ${modFor}, у вас ${game}` : `Сделан для игры ${modFor}`,
  };
}

/**
 * Подпись на месте картинки у дополнения. Повтор метки («Звук» под «ЗВУК») не пишем —
 * только то, что метка не говорит.
 */
export function addonCaption(mod: Pick<ModCardDto, "kind">): string | null {
  if (mod.kind === "vehicle") return "Для машин игры";
  if (mod.kind === "level") return "Для карт игры";
  return null;
}

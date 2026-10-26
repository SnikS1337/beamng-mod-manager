import type { ModCardDto } from "#lib/ipc/library.js";

export type StatusTone = "update" | "possible" | "problem" | "pending" | "off" | "deleted";

/** Одна метка состояния на карточке и в строке списка — самая важная. */
export function modStatus(
  mod: Pick<ModCardDto, "deleted" | "problem" | "update" | "pending" | "enabled">,
): { text: string; tone: StatusTone } | null {
  if (mod.deleted) return { text: "Удалён", tone: "deleted" };
  if (mod.problem) return { text: "Игра не загрузит", tone: "problem" };
  if (mod.update === "likely") return { text: "Обновление", tone: "update" };
  if (mod.update === "possible") return { text: "Возможно, обновление", tone: "possible" };
  if (mod.pending === "queued") return { text: "После игры", tone: "pending" };
  if (!mod.enabled) return { text: "Выключен", tone: "off" };
  return null;
}

import type { ModCardDto } from "#lib/ipc/library.js";

/** Метка типа на карточке: по составу архива, пока он не прочитан — по угадыванию из файла. */
export type TypeTag = "vehicle" | "map" | "addon" | "script" | "sound" | "mod";

/** Группа для фильтра «Машины / Карты / Дополнения». */
export type TypeGroup = "vehicle" | "map" | "addon";

export function typeTag(mod: Pick<ModCardDto, "role" | "kind">): TypeTag {
  if (mod.role === "vehicle") return "vehicle";
  if (mod.role === "map") return "map";
  if (mod.role === "addon") {
    if (mod.kind === "script") return "script";
    if (mod.kind === "sound") return "sound";
    return "addon";
  }
  switch (mod.kind) {
    case "vehicle":
      return "vehicle";
    case "level":
      return "map";
    case "script":
      return "script";
    case "sound":
      return "sound";
    default:
      return "mod";
  }
}

export function typeLabel(tag: TypeTag): string {
  switch (tag) {
    case "vehicle":
      return "Машина";
    case "map":
      return "Карта";
    case "script":
      return "Скрипт";
    case "sound":
      return "Звук";
    case "addon":
      return "Дополнение";
    default:
      return "Мод";
  }
}

export function typeGroup(tag: TypeTag): TypeGroup | null {
  if (tag === "vehicle" || tag === "map") return tag;
  if (tag === "mod") return null;
  return "addon";
}

export function typeCounts(mods: Pick<ModCardDto, "role" | "kind">[]): Record<TypeGroup, number> {
  const counts: Record<TypeGroup, number> = { vehicle: 0, map: 0, addon: 0 };
  for (const mod of mods) {
    const group = typeGroup(typeTag(mod));
    if (group) counts[group] += 1;
  }
  return counts;
}

export function matchesType(
  mod: Pick<ModCardDto, "role" | "kind">,
  wanted: TypeGroup | "all",
): boolean {
  return wanted === "all" || typeGroup(typeTag(mod)) === wanted;
}

export type ViewMode = "grid" | "list";

const VIEW_KEY = "bmm.garage.view";

/** Вид гаража запоминается в этом окне программы; нет хранилища — сетка. */
export function loadViewMode(storage: Pick<Storage, "getItem"> | null = safeStorage()): ViewMode {
  try {
    return storage?.getItem(VIEW_KEY) === "list" ? "list" : "grid";
  } catch {
    return "grid";
  }
}

export function saveViewMode(
  mode: ViewMode,
  storage: Pick<Storage, "setItem"> | null = safeStorage(),
): void {
  try {
    storage?.setItem(VIEW_KEY, mode);
  } catch {
    // Хранилище недоступно — вид просто не запомнится.
  }
}

function safeStorage(): Storage | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

/**
 * Куда прокрутить список строк одинаковой высоты, чтобы строка `index` стала видна.
 * `null` — она уже на экране. В режиме списка стрелки раньше меняли выбор, но
 * строка за краем экрана так и оставалась невидимой.
 */
export function revealScroll(
  index: number,
  row: number,
  scrollTop: number,
  height: number,
): number | null {
  const top = index * row;
  const bottom = top + row;
  if (top < scrollTop) return top;
  if (bottom > scrollTop + height) return Math.max(0, bottom - height);
  return null;
}

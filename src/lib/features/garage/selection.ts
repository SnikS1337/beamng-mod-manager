import type { ModCardDto } from "#lib/ipc/library.js";

/**
 * Выбор модов в гараже: один (панель мода справа) или несколько (панель действий).
 * Как в проводнике: щелчок — только этот, Ctrl — добавить/убрать, Shift — диапазон от
 * последнего выбранного по видимому порядку, Ctrl+Shift — добавить диапазон.
 */
export interface Selection {
  ids: string[];
  /** От чего считать диапазон при Shift. */
  anchor: string | null;
  /**
   * Пачку собирают галочками: панель мода не открывается даже на одном выбранном.
   * Иначе первая галочка открывала панель, сетка перестраивалась — и вторая
   * галочка попадала уже по другой карточке.
   */
  checking?: boolean;
}

export const EMPTY: Selection = { ids: [], anchor: null };

export interface ClickKeys {
  ctrl?: boolean;
  shift?: boolean;
  /** Щелчок по галочке на карточке: добавить или убрать, панель мода не открывать. */
  check?: boolean;
}

export function clickSelect(
  current: Selection,
  id: string,
  order: string[],
  keys: ClickKeys = {},
): Selection {
  if (keys.check) {
    const ids = current.ids.includes(id)
      ? current.ids.filter((item) => item !== id)
      : inOrder([...current.ids, id], order);
    return ids.length === 0 ? EMPTY : { ids, anchor: id, checking: true };
  }
  if (keys.shift && current.anchor && order.includes(current.anchor) && order.includes(id)) {
    const from = order.indexOf(current.anchor);
    const to = order.indexOf(id);
    const range = order.slice(Math.min(from, to), Math.max(from, to) + 1);
    const ids = keys.ctrl ? unique([...current.ids, ...range]) : range;
    return { ids: inOrder(ids, order), anchor: current.anchor, checking: current.checking };
  }
  if (keys.ctrl) {
    const ids = current.ids.includes(id)
      ? current.ids.filter((item) => item !== id)
      : inOrder([...current.ids, id], order);
    return ids.length === 0 ? EMPTY : { ids, anchor: id, checking: current.checking };
  }
  // Повторный щелчок по единственному выбранному снимает выбор (закрывает панель).
  if (current.ids.length === 1 && current.ids[0] === id) return EMPTY;
  return { ids: [id], anchor: id };
}

export function selectOnly(id: string | null): Selection {
  return id ? { ids: [id], anchor: id } : EMPTY;
}

export function selectAll(order: string[]): Selection {
  return { ids: [...order], anchor: order[0] ?? null };
}

/** После смены фильтра или удаления: выбранными остаются только видимые. */
export function keepVisible(current: Selection, visible: string[]): Selection {
  const shown = new Set(visible);
  const ids = current.ids.filter((id) => shown.has(id));
  if (ids.length === current.ids.length) return current;
  if (ids.length === 0) return EMPTY;
  const anchor = current.anchor && shown.has(current.anchor) ? current.anchor : (ids[0] ?? null);
  return { ids, anchor, checking: current.checking };
}

/** Выбран ровно один — показываем его панель. */
export function single(current: Selection): string | null {
  return current.ids.length === 1 && !current.checking ? current.ids[0] : null;
}

/** Какая панель справа: никакой, панель мода или «Выбрано N». Сменилась — раскладка двигается. */
export function panelOf(current: Selection): "none" | "one" | "many" {
  if (current.ids.length > 1) return "many";
  return single(current) ? "one" : "none";
}

/** Что можно сделать с выбранными: включить выключенные, выключить включённые, удалить. */
export interface BulkPlan {
  enable: ModCardDto[];
  disable: ModCardDto[];
  remove: ModCardDto[];
}

export function bulkPlan(cards: ModCardDto[], ids: string[]): BulkPlan {
  const wanted = new Set(ids);
  const chosen = cards.filter((card) => wanted.has(card.id) && !card.deleted);
  return {
    enable: chosen.filter((card) => !card.enabled),
    disable: chosen.filter((card) => card.enabled),
    // Моды из репозитория игры удаляются в самой игре.
    remove: chosen.filter((card) => !card.fromGameRepo),
  };
}

function unique(ids: string[]): string[] {
  return [...new Set(ids)];
}

function inOrder(ids: string[], order: string[]): string[] {
  const position = new Map(order.map((id, index) => [id, index]));
  return unique(ids).sort(
    (a, b) => (position.get(a) ?? Number.MAX_SAFE_INTEGER) - (position.get(b) ?? Number.MAX_SAFE_INTEGER),
  );
}

/** Сколько держать Ctrl, прежде чем показать подсказку: короткое нажатие — не вопрос. */
export const HOLD_MS = 500;

export interface ShortcutHint {
  keys: string;
  text: string;
}

/** Какие сочетания есть на этой странице. Ctrl+Z — только когда есть что отменить. */
export function hintsFor(path: string, canUndo: boolean): ShortcutHint[] {
  const hints: ShortcutHint[] = [];
  if (path === "/") {
    hints.push({ keys: "Ctrl F", text: "найти мод" });
    hints.push({ keys: "Ctrl A", text: "выбрать все" });
    hints.push({ keys: "Ctrl + клик", text: "добавить к выбору" });
    if (canUndo) hints.push({ keys: "Ctrl Z", text: "отменить" });
  } else if (path === "/search") {
    hints.push({ keys: "Ctrl F", text: "к строке поиска" });
  }
  hints.push({ keys: "Ctrl K", text: "команды и переходы" });
  return hints;
}

/**
 * Подсказка горячих клавиш, пока Ctrl держат один. Нажали ещё клавишу или
 * кликнули (Ctrl+клик — выбор карточек), отпустили, окно ушло из фокуса — прячем.
 */
export class CtrlHold {
  shown = $state(false);
  #timer: ReturnType<typeof setTimeout> | undefined;

  keydown(event: Pick<KeyboardEvent, "key" | "repeat" | "shiftKey" | "altKey" | "metaKey">): void {
    if (event.key === "Control") {
      // Удержание повторяет keydown — таймер не перезапускаем.
      if (event.repeat || this.#timer || this.shown) return;
      if (event.shiftKey || event.altKey || event.metaKey) return;
      this.#timer = setTimeout(() => {
        this.#timer = undefined;
        this.shown = true;
      }, HOLD_MS);
      return;
    }
    this.cancel();
  }

  keyup(event: Pick<KeyboardEvent, "key">): void {
    if (event.key === "Control") this.cancel();
  }

  cancel(): void {
    if (this.#timer) clearTimeout(this.#timer);
    this.#timer = undefined;
    this.shown = false;
  }
}

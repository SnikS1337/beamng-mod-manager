import { tick } from "svelte";

/**
 * Плавный поток (режим «Живо»): смена раздела, фильтра, открытие панели мода идут
 * одним движением через View Transitions — карточки переезжают на новые места,
 * а не перерисовываются скачком. Без анимаций или без поддержки — мгновенно.
 */

type FlowDocument = Pick<Document, "documentElement"> & {
  startViewTransition?: Document["startViewTransition"];
  visibilityState?: DocumentVisibilityState;
};

/** Можно ли сейчас плавно: браузер умеет, анимации включены, окно видно. */
export function canFlow(doc: FlowDocument | undefined): boolean {
  if (!doc || typeof doc.startViewTransition !== "function") return false;
  if (doc.documentElement.classList.contains("no-motion")) return false;
  return doc.visibilityState !== "hidden";
}

/**
 * Имя карточки для перехода — из id мода. Id бывают путями с любыми знаками,
 * а имя должно быть CSS-идентификатором, поэтому берём хеш FNV-1a.
 */
export function flowName(id: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < id.length; i++) {
    hash ^= id.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return `m-${(hash >>> 0).toString(36)}`;
}

class Motion {
  /** Идёт переход: карточкам не нужно своё «появление» поверх него. */
  active = $state(false);
  #current: ViewTransition | null = null;
  #measures = new Set<() => void>();

  /**
   * Замерить размеры до снимка нового вида. Сетка узнаёт свою ширину через
   * ResizeObserver уже после перехода — и перестройка посреди анимации обрывала её.
   */
  measure(fn: () => void): () => void {
    this.#measures.add(fn);
    return () => this.#measures.delete(fn);
  }

  /**
   * Пока идёт переход, браузер отдаёт клики корню страницы — второй быстрый клик
   * терялся. Ловим такой клик, обрываем переход и повторяем клик по тому, что под
   * курсором уже в новом виде. Вызывается один раз из оболочки; возвращает отписку.
   */
  install(): () => void {
    const onclick = (event: MouseEvent) => {
      const transition = this.#current;
      if (!transition || event.target !== document.documentElement) return;
      event.stopPropagation();
      event.preventDefault();
      transition.skipTransition();
      const replay = () => {
        const target = document.elementFromPoint(event.clientX, event.clientY);
        if (!target || target === document.documentElement) return;
        target.dispatchEvent(
          new MouseEvent("click", {
            bubbles: true,
            cancelable: true,
            clientX: event.clientX,
            clientY: event.clientY,
            ctrlKey: event.ctrlKey,
            shiftKey: event.shiftKey,
            altKey: event.altKey,
            metaKey: event.metaKey,
            button: event.button,
          }),
        );
      };
      transition.finished.then(replay, replay);
    };
    window.addEventListener("click", onclick, true);
    return () => window.removeEventListener("click", onclick, true);
  }

  /**
   * Выполнить изменение плавно. `scope` — класс на <html> на время перехода:
   * по нему CSS решает, что именно двигается (`flow-garage`, `flow-route`…).
   */
  run(update: () => void | Promise<void>, scope: string): Promise<void> {
    if (typeof document === "undefined" || !canFlow(document)) {
      return Promise.resolve(update()).then(() => undefined);
    }
    // Новое действие важнее недоигранного: старый переход обрываем, не ждём.
    this.#current?.skipTransition();
    const root = document.documentElement;
    root.classList.add(scope);
    this.active = true;
    const transition = document.startViewTransition(async () => {
      await update();
      await tick();
      for (const fn of this.#measures) fn();
      await tick();
    });
    this.#current = transition;
    const done = () => {
      if (this.#current !== transition) return;
      this.#current = null;
      this.active = false;
      root.classList.remove(scope);
    };
    transition.finished.then(done, done);
    return transition.updateCallbackDone.catch(() => undefined);
  }
}

export const motion = new Motion();

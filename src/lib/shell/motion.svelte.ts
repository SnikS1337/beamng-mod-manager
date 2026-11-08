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
 * а имя должно быть CSS-идентификатором, поэтому берём два независимых 32-битных
 * хеша: для больших библиотек 64 бита практически исключают совпадение имён.
 */
export function flowName(id: string): string {
  let first = 0x811c9dc5;
  let second = 0x9e3779b9;
  for (let i = 0; i < id.length; i++) {
    const code = id.charCodeAt(i);
    first = Math.imul(first ^ code, 0x01000193);
    second = Math.imul(second ^ code, 0x85ebca6b);
  }
  return `m-${(first >>> 0).toString(36)}-${(second >>> 0).toString(36)}`;
}

/** Место карточки на экране. */
export type Box = { left: number; top: number; width: number; height: number };

/**
 * Карточка переезжает далеко — через ряд и вбок (перенос строки сетки) или больше
 * чем на полтора ряда: такой полёт идёт поверх соседей, и карточки на миг закрывают
 * друг друга (было — казалось, что нажатая карточка пропадает). Тогда не летим, а
 * «перескакиваем»: на старом месте тает, на новом проявляется. Сдвиг в своём ряду
 * или на соседнее место — обычный переезд.
 */
export function isHop(from: Box, to: Box): boolean {
  const height = Math.max(from.height, 1);
  const width = Math.max(from.width, 1);
  const dy = Math.abs(to.top - from.top);
  const dx = Math.abs(to.left - from.left);
  const otherRow = dy > height * 0.5;
  return (otherRow && dx > width * 0.5) || dy > height * 1.5;
}

/** Имя карточки для перехода (переменная `--flow` в её стиле). */
function cardName(node: HTMLElement): string {
  return node.style.getPropertyValue("--flow").trim();
}

function cardBoxes(root: ParentNode): Map<string, Box> {
  const boxes = new Map<string, Box>();
  for (const node of root.querySelectorAll<HTMLElement>(".flow-card")) {
    const name = cardName(node);
    if (!name) continue;
    const rect = node.getBoundingClientRect();
    if (rect.width === 0 || rect.height === 0) continue;
    boxes.set(name, { left: rect.left, top: rect.top, width: rect.width, height: rect.height });
  }
  return boxes;
}

/** Области, где двигаются карточки: для них считаем «перескоки». */
const CARD_SCOPES = new Set(["flow-garage", "flow-search", "flow-list"]);

class Motion {
  /** Идёт переход: карточкам не нужно своё «появление» поверх него. */
  active = $state(false);
  #current: ViewTransition | null = null;
  #measures = new Set<() => void>();
  /** Сколько переходов ещё используют класс: у двух подряд не должен гаснуть раньше времени. */
  #scopes = new Map<string, number>();

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
  /** Последний переход доигран (или оборван). Без перехода — сразу. */
  settled: Promise<void> = Promise.resolve();

  run(update: () => void | Promise<void>, scope: string): Promise<void> {
    if (typeof document === "undefined" || !canFlow(document)) {
      return Promise.resolve(update()).then(() => undefined);
    }
    // Новое действие важнее недоигранного: старый переход обрываем, не ждём.
    this.#current?.skipTransition();
    const root = document.documentElement;
    this.#scopes.set(scope, (this.#scopes.get(scope) ?? 0) + 1);
    root.classList.add(scope);
    this.active = true;
    // Где карточки стояли до изменения: по этому решаем, кому лететь, а кому
    // «перескочить» (isHop).
    const before = CARD_SCOPES.has(scope) ? cardBoxes(document) : null;
    const hopped: HTMLElement[] = [];
    const transition = document.startViewTransition(async () => {
      await update();
      await tick();
      for (const fn of this.#measures) fn();
      await tick();
      if (before && before.size > 0) {
        for (const node of document.querySelectorAll<HTMLElement>(".flow-card")) {
          const name = cardName(node);
          const from = before.get(name);
          if (!from) continue;
          const rect = node.getBoundingClientRect();
          if (rect.width === 0 || !isHop(from, rect)) continue;
          // Новое имя — значит, старое место и новое разные слои: там тает, тут появляется.
          node.style.setProperty("view-transition-name", `${name}-hop`);
          node.style.setProperty("view-transition-class", "card hop");
          hopped.push(node);
        }
      }
    });
    this.#current = transition;
    this.settled = transition.finished.then(
      () => undefined,
      () => undefined,
    );
    // При skipTransition() встроенный `ready` отклоняется с AbortError. Мы его не
    // ждём, но без обработчика WebView пишет «Uncaught (in promise)» в консоль.
    transition.ready.catch(() => undefined);
    const done = () => {
      for (const node of hopped) {
        node.style.removeProperty("view-transition-name");
        node.style.removeProperty("view-transition-class");
      }
      const left = (this.#scopes.get(scope) ?? 1) - 1;
      if (left > 0) {
        this.#scopes.set(scope, left);
      } else {
        this.#scopes.delete(scope);
        root.classList.remove(scope);
      }
      // Старый переход мог закончиться уже после начала нового: его класс снят
      // по счётчику, а «активным» остаётся новый.
      if (this.#current !== transition) return;
      this.#current = null;
      this.active = false;
    };
    transition.finished.then(done, done);
    return transition.updateCallbackDone.catch(() => undefined);
  }
}

export const motion = new Motion();

/**
 * Элемент со своим появлением (CSS-анимацией), который родился внутри перехода:
 * его уже ведёт переход, и своё появление не нужно — ни во время перехода, ни
 * после. Было: панель мода выезжала переходом, а когда класс области снимался,
 * её собственная анимация запускалась заново — панель на миг пропадала.
 */
export function bornInFlow(node: HTMLElement, active: () => boolean = () => motion.active) {
  if (active()) node.dataset.flowed = "";
}

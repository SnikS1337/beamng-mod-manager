/** Куда перевести фокус по Tab внутри окна: с последнего — на первый, и наоборот. */
export function wrapTarget<T>(items: T[], active: T | null, backwards: boolean): T | null {
  if (items.length === 0) return null;
  const first = items[0];
  const last = items[items.length - 1];
  if (backwards && (active === first || !items.includes(active as T))) return last;
  if (!backwards && (active === last || !items.includes(active as T))) return first;
  return null;
}

const FOCUSABLE =
  'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * Окно поверх программы держит фокус у себя: Tab не уходит на карточки под ним.
 * `use:trapFocus` на корне окна; `autofocus` — элемент, на который встать сразу.
 */
export function trapFocus(node: HTMLElement, autofocus?: string) {
  // Откуда открыли окно: после закрытия фокус возвращается туда, а не на body
  // (Ctrl+K → Escape раньше оставлял фокус «нигде»).
  const previous =
    typeof document === "undefined" ? null : (document.activeElement as HTMLElement | null);
  const start = autofocus ? node.querySelector<HTMLElement>(autofocus) : null;
  queueMicrotask(() => (start ?? node.querySelector<HTMLElement>(FOCUSABLE))?.focus());
  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Tab") return;
    const items = Array.from(node.querySelectorAll<HTMLElement>(FOCUSABLE));
    const target = wrapTarget(items, document.activeElement as HTMLElement | null, event.shiftKey);
    if (target) {
      event.preventDefault();
      target.focus();
    }
  }
  node.addEventListener("keydown", onkeydown);
  return {
    destroy() {
      node.removeEventListener("keydown", onkeydown);
      if (previous && previous.isConnected && typeof previous.focus === "function") {
        queueMicrotask(() => previous.focus({ preventScroll: true }));
      }
    },
  };
}

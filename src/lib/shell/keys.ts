/**
 * Сочетание Ctrl+буква независимо от раскладки: в русской `event.key` для Ctrl+K — «л»,
 * поэтому смотрим на физическую клавишу (`event.code`).
 */
export function isShortcut(
  event: Pick<KeyboardEvent, "ctrlKey" | "metaKey" | "shiftKey" | "code" | "key">,
  letter: string,
): boolean {
  if ((!event.ctrlKey && !event.metaKey) || event.shiftKey) return false;
  const wanted = letter.toLowerCase();
  return event.code === `Key${wanted.toUpperCase()}` || event.key.toLowerCase() === wanted;
}

/**
 * Вкладки по стрелкам, как положено для `role="tablist"`: ←/→ — соседняя (по кругу),
 * Home/End — первая/последняя. Другая клавиша — `null`.
 */
export function tabByKey<T>(order: readonly T[], current: T, key: string): T | null {
  if (order.length === 0) return null;
  const index = Math.max(0, order.indexOf(current));
  switch (key) {
    case "ArrowRight":
      return order[(index + 1) % order.length];
    case "ArrowLeft":
      return order[(index - 1 + order.length) % order.length];
    case "Home":
      return order[0];
    case "End":
      return order[order.length - 1];
    default:
      return null;
  }
}

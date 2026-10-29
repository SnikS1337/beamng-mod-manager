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

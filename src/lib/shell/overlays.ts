/**
 * Стопка открытых окон поверх программы: палитра, приветствие, проверка архива.
 * Каждое слушало Escape на всём окне, и одно нажатие закрывало сразу два — в том
 * числе проверку архива с её рабочей папкой. Теперь Escape достаётся только верхнему.
 */
const stack: symbol[] = [];

/** Окно открылось. Возвращает функцию, которую вызвать при закрытии. */
export function enterOverlay(): { id: symbol; leave: () => void } {
  const id = Symbol("overlay");
  stack.push(id);
  return {
    id,
    leave: () => {
      const index = stack.indexOf(id);
      if (index >= 0) stack.splice(index, 1);
    },
  };
}

export function isTopOverlay(id: symbol): boolean {
  return stack.length > 0 && stack[stack.length - 1] === id;
}

export function anyOverlay(): boolean {
  return stack.length > 0;
}

/**
 * Escape для окна `id`: только если оно сверху и нажатие ещё никто не забрал.
 * Забирает нажатие себе, чтобы окно под ним в том же событии его не увидело.
 */
export function takeEscape(event: KeyboardEvent, id: symbol | null): boolean {
  if (event.key !== "Escape" || event.defaultPrevented || !id || !isTopOverlay(id)) return false;
  event.preventDefault();
  return true;
}

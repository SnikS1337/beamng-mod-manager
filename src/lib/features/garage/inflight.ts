/**
 * Что уже меняется: переключение мода идёт, пока ждём ответа программы.
 *
 * Было: двойной клик по переключателю отправлял две одинаковые команды и показывал
 * два сообщения. Повтор, пока первая команда не ответила, теперь пропускается.
 */
export class InFlight {
  #busy = new Set<string>();

  /** Начать; `false` — уже идёт, второй раз не начинаем. */
  start(id: string): boolean {
    if (this.#busy.has(id)) return false;
    this.#busy.add(id);
    return true;
  }

  done(id: string): void {
    this.#busy.delete(id);
  }

  has(id: string): boolean {
    return this.#busy.has(id);
  }
}

/**
 * Карточка мода в текущем списке. Список перечитывают целиком, и прежний объект
 * карточки остаётся в замыканиях («Отменить»): правка по нему не видна на экране.
 */
export function liveCard<T extends { id: string }>(list: readonly T[] | undefined, stale: T): T {
  return list?.find((item) => item.id === stale.id) ?? stale;
}

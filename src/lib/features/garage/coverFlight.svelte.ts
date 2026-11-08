import { tick } from "svelte";
import { motion } from "#lib/shell/motion.svelte.js";

/** Сколько обложек помнить для панели: больше, чем карточек на экране. */
const KNOWN_LIMIT = 400;

/**
 * Обложка карточки «переезжает» в панель мода, когда панель открывается.
 *
 * Имя перехода сначала только у обложки карточки (`from`, старый вид), потом только
 * у обложки в панели (`to`, новый вид): у двух элементов одно имя в одном виде —
 * и браузер отменяет переход. Пока данные мода грузятся, панель показывает уже
 * известную обложку карточки — летит картинка, а не серая заглушка.
 */
class CoverFlight {
  from = $state<string | null>(null);
  to = $state<string | null>(null);
  #known = new Map<string, string>();

  /** Карточка узнала свою обложку. */
  remember(id: string, url: string | null): void {
    this.#known.delete(id);
    if (!url) return;
    this.#known.set(id, url);
    while (this.#known.size > KNOWN_LIMIT) {
      const oldest = this.#known.keys().next().value;
      if (oldest === undefined) break;
      this.#known.delete(oldest);
    }
  }

  known(id: string | null): string | null {
    return id ? (this.#known.get(id) ?? null) : null;
  }

  /**
   * Открыть панель мода `id`. `canFly` — карточка на экране (сетка, не список):
   * иначе лететь неоткуда, панель просто выезжает.
   */
  async open(id: string, canFly: boolean, update: () => void): Promise<void> {
    if (!canFly) {
      this.from = null;
      this.to = null;
      await motion.run(update, "flow-garage");
      return;
    }
    this.from = id;
    this.to = null;
    // Имя у обложки карточки должно стоять до снимка старого вида.
    await tick();
    await motion.run(() => {
      this.from = null;
      this.to = id;
      update();
    }, "flow-garage");
    await motion.settled;
    // Следующий переход (фильтр, другой мод) не должен снова «везти» обложку.
    if (this.to === id) this.to = null;
  }
}

export const coverFlight = new CoverFlight();

import { tick } from "svelte";

/** Сколько держать вспышку: столько же длится анимация в карточке. */
export const FLASH_MS = 900;

/**
 * Карточка, у которой только что включились две копии сразу: метка «копии»
 * коротко вздрагивает и подсвечивается. Повтор на той же карточке перезапускает
 * анимацию, а не теряется.
 */
class ConflictFlash {
  id = $state<string | null>(null);
  #timer: ReturnType<typeof setTimeout> | undefined;

  async flash(id: string): Promise<void> {
    if (this.#timer) clearTimeout(this.#timer);
    if (this.id === id) {
      // Тот же класс второй раз анимацию не запустит: снимаем и ставим заново.
      this.id = null;
      await tick();
    }
    this.id = id;
    this.#timer = setTimeout(() => {
      if (this.id === id) this.id = null;
      this.#timer = undefined;
    }, FLASH_MS);
  }
}

export const conflictFlash = new ConflictFlash();

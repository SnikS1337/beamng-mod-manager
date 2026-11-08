import { tick } from "svelte";

/** Сколько держать вспышку копий: столько же длится анимация в карточке. */
export const FLASH_MS = 900;
/** Сколько держать «Установлен» на только что поставленной карточке. */
export const INSTALLED_MS = 1800;

/**
 * Короткая отметка на одной карточке: «вторая копия включена», «только что
 * установлен». Повтор на той же карточке перезапускает анимацию, а не теряется;
 * другая карточка перехватывает отметку.
 */
export class Flash {
  id = $state<string | null>(null);
  #timer: ReturnType<typeof setTimeout> | undefined;

  constructor(private readonly duration: number) {}

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
    }, this.duration);
  }
}

export const conflictFlash = new Flash(FLASH_MS);
export const installedFlash = new Flash(INSTALLED_MS);

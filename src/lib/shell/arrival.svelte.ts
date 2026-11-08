import { tick } from "svelte";
import { motion } from "./motion.svelte.js";

/** Дольше гараж не ждём: переход замораживает экран, пока ждёт новый вид. */
export const LAND_TIMEOUT_MS = 1200;

/**
 * Установленный мод «прилетает» в гараж: строка из окна установки одним движением
 * переезжает на место своей карточки. Окно установки зовёт `fly`, гараж — `landed`,
 * когда карточка уже на экране. Гаража нет на экране или анимации выключены —
 * всё меняется сразу, как раньше.
 *
 * Имя перехода карточка получает только после посадки: при обновлении мода его
 * старая карточка уже стоит в гараже под окном, и два элемента с одним именем
 * сорвали бы переход.
 */
class Arrival {
  /** Файл мода, который летит сейчас (в нижнем регистре): у его строки в окне — имя перехода. */
  flying = $state<string | null>(null);
  /** Карточка этого файла уже на месте — имя перехода у неё. */
  landedName = $state<string | null>(null);
  #landing: (() => void) | null = null;
  #hosts = 0;

  /** Гараж на экране и умеет принять мод. Возвращает отписку. */
  attach(): () => void {
    this.#hosts += 1;
    let attached = true;
    return () => {
      if (!attached) return;
      attached = false;
      this.#hosts -= 1;
      // Гараж ушёл посреди полёта — не держим экран до тайм-аута.
      if (this.#hosts === 0) this.landed();
    };
  }

  get canLand(): boolean {
    return this.#hosts > 0;
  }

  isFlying(fileName: string): boolean {
    return this.flying !== null && fileName.toLowerCase() === this.flying;
  }

  isLanded(fileName: string): boolean {
    return this.landedName !== null && fileName.toLowerCase() === this.landedName;
  }

  /**
   * Закрыть окно и отдать мод гаражу. `update` — само изменение (закрыть окно,
   * сообщить гаражу); переход ждёт посадки, но не дольше `LAND_TIMEOUT_MS`.
   */
  async fly(fileName: string | null | undefined, update: () => void): Promise<void> {
    this.#landing?.();
    if (!fileName || !this.canLand) {
      this.flying = null;
      this.landedName = null;
      update();
      return;
    }
    this.flying = fileName.toLowerCase();
    this.landedName = null;
    // Имя прошлой посадки снимаем до снимка старого вида: иначе у двух элементов
    // одно имя, и браузер отменяет переход.
    await tick();
    await motion.run(async () => {
      const landed = new Promise<void>((resolve) => {
        const timer = setTimeout(resolve, LAND_TIMEOUT_MS);
        this.#landing = () => {
          clearTimeout(timer);
          resolve();
        };
      });
      update();
      await landed;
      this.#landing = null;
    }, "flow-arrive");
  }

  /** Гараж показал карточку `fileName` (или не нашёл её — тогда без имени). */
  landed(fileName?: string): void {
    if (fileName && this.isFlying(fileName)) this.landedName = this.flying;
    const landing = this.#landing;
    this.#landing = null;
    landing?.();
  }
}

export const arrival = new Arrival();

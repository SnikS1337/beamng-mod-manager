export interface ToastOptions {
  undo?: () => void | Promise<void>;
  commit?: () => void | Promise<void>;
  duration?: number;
}

interface ToastItem extends ToastOptions {
  message: string;
  timer?: ReturnType<typeof setTimeout>;
}

/** Сколько держать тост: по длине текста, чтобы успеть прочитать. */
export function readingTime(message: string, withUndo = false): number {
  // ~15 знаков в секунду плюс время заметить тост.
  const reading = 2000 + message.length * 65;
  const least = withUndo ? 6000 : 3000;
  return Math.min(Math.max(reading, least), 15000);
}

function errorText(reason: unknown): string {
  if (typeof reason === "string") return reason;
  if (reason && typeof reason === "object" && "message" in reason) {
    return String((reason as { message: unknown }).message);
  }
  return "Не удалось выполнить действие.";
}

class Toasts {
  current = $state<ToastItem | null>(null);
  /** Следующие сообщения ждут своей очереди, а не перебивают «Отменить». */
  #queue: ToastItem[] = [];
  /** Сколько осталось показывать текущему, пока таймер стоит на паузе. */
  #left: number | null = null;
  #endsAt = 0;
  /** Когда исчез прошлый тост: новый появляется после того, как старый погас. */
  hiddenAt = 0;

  get canUndo(): boolean {
    return this.current?.undo !== undefined;
  }

  show(message: string, options: ToastOptions = {}): void {
    const item: ToastItem = {
      message,
      undo: options.undo,
      commit: options.commit,
      duration: options.duration,
    };
    if (this.current) {
      // Одно и то же сообщение дважды подряд не ждёт своей очереди ещё раз:
      // повторы растягивали показ на десятки секунд.
      const repeat = (other: ToastItem) => !other.undo && other.message === message;
      if (!item.undo && (repeat(this.current) || this.#queue.some(repeat))) return;
      // Фоновое «нашли обновления» не должно отнимать кнопку у удаления.
      this.#queue.push(item);
      if (this.#queue.length > 8) this.#queue.shift();
      return;
    }
    this.#activate(item);
  }

  async undo(): Promise<boolean> {
    const current = this.current;
    if (!current?.undo) return false;
    if (current.timer) clearTimeout(current.timer);
    this.#hide();
    try {
      await current.undo();
    } catch (reason) {
      this.show(errorText(reason));
    }
    this.#next();
    return true;
  }

  /** Курсор на тосте или фокус на «Отменить»: время не идёт, текст не убегает. */
  pause(): void {
    const current = this.current;
    if (!current?.timer || this.#left !== null) return;
    clearTimeout(current.timer);
    this.#left = Math.max(this.#endsAt - Date.now(), 0);
    this.current = { ...current, timer: undefined };
  }

  resume(): void {
    const current = this.current;
    if (!current || this.#left === null) return;
    // После паузы — не меньше полутора секунд, чтобы тост не исчез из-под курсора.
    const left = Math.max(this.#left, 1500);
    this.#left = null;
    this.#start(current, left);
  }

  #activate(item: ToastItem): void {
    // Очередь длинная — показываем короче, но не меньше 3 секунд: иначе хвост
    // сообщений висел бы минуту.
    const base = item.duration ?? readingTime(item.message, item.undo !== undefined);
    const duration =
      this.#queue.length > 1 && !item.undo && item.duration === undefined
        ? Math.max(3000, Math.round(base * 0.6))
        : base;
    this.#left = null;
    this.#start(item, duration);
  }

  #start(item: ToastItem, duration: number): void {
    this.#endsAt = Date.now() + duration;
    const timer = setTimeout(() => {
      if (this.current?.timer !== timer) return;
      this.#finish();
    }, duration);
    this.current = { ...item, timer };
  }

  #hide(): void {
    this.current = null;
    this.#left = null;
    this.hiddenAt = Date.now();
  }

  /** Свой срок вышел: действие закрепляем, показываем следующее сообщение. */
  #finish(): void {
    const current = this.current;
    if (!current) return;
    if (current.timer) clearTimeout(current.timer);
    this.#hide();
    if (current.commit) void Promise.resolve(current.commit()).catch(() => {});
    this.#next();
  }

  #next(): void {
    if (this.current) return;
    const item = this.#queue.shift();
    if (item) this.#activate(item);
  }
}

export const toasts = new Toasts();

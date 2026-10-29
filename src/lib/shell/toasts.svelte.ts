export interface ToastOptions {
  undo?: () => void | Promise<void>;
  commit?: () => void | Promise<void>;
  duration?: number;
}

interface ToastItem extends ToastOptions {
  message: string;
  timer?: ReturnType<typeof setTimeout>;
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
    this.current = null;
    try {
      await current.undo();
    } catch (reason) {
      this.show(errorText(reason));
    }
    this.#next();
    return true;
  }

  #activate(item: ToastItem): void {
    const duration =
      item.duration ?? (item.undo ? 6000 : item.message.length > 48 ? 5000 : 3000);
    const timer = setTimeout(() => {
      if (this.current?.timer !== timer) return;
      this.#finish();
    }, duration);
    this.current = { ...item, timer };
  }

  /** Свой срок вышел: действие закрепляем, показываем следующее сообщение. */
  #finish(): void {
    const current = this.current;
    if (!current) return;
    if (current.timer) clearTimeout(current.timer);
    this.current = null;
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

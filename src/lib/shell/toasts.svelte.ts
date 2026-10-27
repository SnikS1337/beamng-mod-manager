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

  show(message: string, options: ToastOptions = {}): void {
    const previous = this.current;
    if (previous?.timer) clearTimeout(previous.timer);
    // Предыдущее действие финализируем: удалённый мод иначе остаётся в
    // скрытом тикете и пропадает из «Удалённых» до перезапуска.
    if (previous?.commit) void Promise.resolve(previous.commit()).catch(() => {});
    const duration =
      options.duration ?? (options.undo ? 6000 : message.length > 48 ? 5000 : 3000);
    const timer = setTimeout(() => {
      const current = this.current;
      if (current?.timer !== timer) return;
      this.current = null;
      if (current.commit) void Promise.resolve(current.commit()).catch(() => {});
    }, duration);
    this.current = { message, undo: options.undo, commit: options.commit, timer };
  }

  async undo(): Promise<void> {
    const current = this.current;
    if (!current) return;
    if (current.timer) clearTimeout(current.timer);
    this.current = null;
    if (!current.undo) return;
    try {
      await current.undo();
    } catch (reason) {
      this.show(errorText(reason));
    }
  }
}

export const toasts = new Toasts();

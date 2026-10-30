/**
 * Сохранения настроек по одному. Запрос во время сохранения не теряется: после
 * текущего выполнится ещё одно. `idle()` ждёт, пока сохранится всё, — так окно
 * не закрывается, пока последняя правка ещё в очереди.
 */
export class SaveQueue {
  private running: Promise<void> | null = null;
  private again = false;

  constructor(private readonly task: () => Promise<void>) {}

  /** Сохранить; если сохранение уже идёт — ещё раз после него. */
  request(): Promise<void> {
    if (this.running) {
      this.again = true;
      return this.running;
    }
    const run = async () => {
      try {
        do {
          this.again = false;
          await this.task();
        } while (this.again);
      } finally {
        this.running = null;
      }
    };
    this.running = run();
    return this.running;
  }

  /** Дождаться конца всех запрошенных сохранений. */
  idle(): Promise<void> {
    return this.running ?? Promise.resolve();
  }

  get busy(): boolean {
    return this.running !== null;
  }
}

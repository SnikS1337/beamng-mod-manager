import { listenUpdates, type UpdateProgress, type UpdatesEvent } from "#lib/ipc/updates.js";

/** Фоновая проверка при запуске и по расписанию приходит с этим номером. */
export const AUTO_REQUEST = 0;

/**
 * Идущая проверка живёт дольше страницы «Обновления»: раньше уход в другой раздел
 * отменял её, а при возврате раздел показывал «ничего не идёт», хотя сайты ещё
 * проверялись. Ход и итог держим здесь, страница только показывает их.
 */
class UpdatesRun {
  checking = $state(false);
  progress = $state<UpdateProgress | null>(null);
  autoProgress = $state<UpdateProgress | null>(null);
  /** Номер своей (нажатой человеком) проверки: чужие события не путаем с ней. */
  requestId = 0;
  /** Итог, пришедший, пока раздел был закрыт: страница покажет его при возврате. */
  pending: UpdatesEvent | null = null;
  private started = false;

  start(): void {
    if (this.started) return;
    this.started = true;
    void listenUpdates((event) => this.onEvent(event)).catch(() => {
      this.started = false;
    });
  }

  /** Новая проверка: следующий номер, ход с нуля. */
  begin(): number {
    this.requestId += 1;
    this.checking = true;
    this.progress = null;
    this.pending = null;
    return this.requestId;
  }

  onEvent(event: UpdatesEvent): void {
    if (event.requestId === AUTO_REQUEST) {
      if (event.phase === "progress") this.autoProgress = event.progress;
      else this.autoProgress = null;
      return;
    }
    if (event.requestId !== this.requestId) return;
    if (event.phase === "progress") {
      this.progress = event.progress;
      return;
    }
    this.checking = false;
    this.progress = null;
    this.pending = event;
  }
}

export const updatesRun = new UpdatesRun();

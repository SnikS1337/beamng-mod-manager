import { listenUpdates, type UpdateProgress, type UpdatesEvent } from "#lib/ipc/updates.js";
import { isSiteFinish } from "./siteLive.js";
import { outcomeOf, type CheckOutcome } from "./outcome.js";

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
  /** Итог последней законченной проверки: «новых обновлений нет» не пропадает с тостом. */
  outcome = $state<CheckOutcome | null>(null);
  /** Номер своей (нажатой человеком) проверки: чужие события не путаем с ней. */
  requestId = 0;
  /** Итог, пришедший, пока раздел был закрыт: страница покажет его при возврате. */
  pending: UpdatesEvent | null = null;
  /**
   * Раздел открыт и сам разбирает события. Тогда здесь их не трогаем: оба слушателя
   * получают одно событие в неизвестном порядке, и итог мог остаться в `pending` —
   * при следующем входе раздел показал бы старый итог и тост ещё раз.
   */
  attached = false;
  /** Свою проверку с этим номером раздел уже показал: второй раз её итог не нужен. */
  handledDone = 0;
  private started = false;

  /**
   * Раздел подписался на события сам. До этого момента итог ловит этот объект —
   * раньше `attached` ставился раньше подписки раздела, и «проверка закончилась»,
   * пришедшее в этот промежуток, не доставалось никому: полоса крутилась вечно.
   * Возвращает итог, который раздел ещё не видел.
   */
  attach(): UpdatesEvent | null {
    this.attached = true;
    const pending = this.pending;
    this.pending = null;
    if (!pending || pending.requestId === this.handledDone) return null;
    return pending;
  }

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
    if (this.attached) return;
    if (event.requestId === AUTO_REQUEST) {
      if (event.phase === "auto") this.outcome = outcomeOf(event, { background: true }) ?? this.outcome;
      if (event.phase !== "progress") this.autoProgress = null;
      // «Сайт закончил» общую строку хода не меняет: другие сайты ещё идут.
      else if (!isSiteFinish(event.progress) || this.autoProgress === null) {
        this.autoProgress = event.progress;
      }
      return;
    }
    if (event.requestId !== this.requestId) return;
    if (event.phase === "progress") {
      if (!isSiteFinish(event.progress)) this.progress = event.progress;
      return;
    }
    this.checking = false;
    this.progress = null;
    this.pending = event;
    // Охват (один сайт или все) знает только раздел — он уточнит итог при возврате.
    if (event.phase === "done") this.outcome = outcomeOf(event) ?? this.outcome;
  }
}

export const updatesRun = new UpdatesRun();

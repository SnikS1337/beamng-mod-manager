import { listenUpdates, updatesSnapshot, type UpdatesEvent } from "$lib/ipc/updates";
import { plural } from "$lib/features/garage/format";
import { toasts } from "$lib/shell/toasts.svelte";

/**
 * Сколько модов ждут обновления — для значка на «Обновлениях». Знает и о фоновой проверке
 * при запуске: про найденное программа говорит сама, раздел открывать не нужно.
 */
class UpdatesBadge {
  available = $state(0);
  private started = false;

  start(): void {
    if (this.started) return;
    this.started = true;
    void updatesSnapshot()
      .then((snapshot) => {
        this.available = snapshot.findings.length;
      })
      .catch(() => {});
    void listenUpdates((event) => this.onEvent(event));
  }

  private onEvent(event: UpdatesEvent): void {
    if (event.phase === "progress" || event.phase === "error") return;
    this.available = event.findings.length;
    if (event.phase === "auto" && event.newCount > 0) {
      const count = event.newCount;
      toasts.show(
        `Нашлись обновления: ${count} ${plural(count, "мод", "мода", "модов")}. Они в разделе «Обновления»`,
      );
    }
  }
}

export const updatesBadge = new UpdatesBadge();

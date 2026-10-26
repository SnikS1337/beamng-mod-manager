import { listenUpdates, updatesSnapshot, type UpdatesEvent } from "#lib/ipc/updates.js";
import { plural } from "#lib/features/garage/format.js";
import { toasts } from "#lib/shell/toasts.svelte.js";

/**
 * Сколько модов ждут обновления — для значка на «Обновлениях». Знает и о фоновой проверке
 * при запуске: про найденное программа говорит сама, раздел открывать не нужно.
 */
class UpdatesBadge {
  available = $state(0);
  private started = false;
  private refreshId = 0;

  start(): void {
    if (this.started) return;
    this.started = true;
    this.refresh();
    void listenUpdates((event) => this.onEvent(event)).catch(() => {});
    // Поставили обновление или сняли страницу — число на значке меняется сразу.
    if (typeof window !== "undefined") {
      window.addEventListener("bmm:library-changed", () => this.refresh());
    }
  }

  refresh(): void {
    const id = ++this.refreshId;
    void updatesSnapshot()
      .then((snapshot) => {
        if (id === this.refreshId) this.available = snapshot.findings.length;
      })
      .catch(() => {});
  }

  private onEvent(event: UpdatesEvent): void {
    if (event.phase === "progress" || event.phase === "error") return;
    this.refreshId += 1;
    this.available = event.findings.length;
    if (event.phase === "auto" && event.newCount > 0) {
      const count = event.newCount;
      toasts.show(
        `Нашли обновления: ${count} ${plural(count, "мод", "мода", "модов")}. Они в разделе «Обновления»`,
      );
    }
  }
}

export const updatesBadge = new UpdatesBadge();

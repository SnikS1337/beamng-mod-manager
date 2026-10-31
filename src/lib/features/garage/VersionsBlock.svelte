<script lang="ts">
  import { Button } from "#lib/components/ui/button/index.js";
  import { modDeleteVersion, modVersions, type VersionRow } from "#lib/ipc/library.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import { formatDateTime, formatSize } from "./format";

  let {
    modId,
    versionsKey = 0,
    onRestore,
  }: {
    modId: string;
    /** Меняется после восстановления копии: список нужно перечитать. */
    versionsKey?: number;
    onRestore: (version: VersionRow) => void;
  } = $props();

  let versions = $state<VersionRow[]>([]);
  let loading = $state(true);
  let token = 0;
  /** Копия, у которой «Удалить» уже нажали один раз: второе нажатие стирает её. */
  let confirming = $state<string | null>(null);
  let deleting = $state(false);

  // Копия стирается насовсем, поэтому — второе нажатие «Точно удалить».
  async function remove(version: VersionRow) {
    if (deleting) return;
    if (confirming !== version.backupId) {
      confirming = version.backupId;
      return;
    }
    deleting = true;
    try {
      await modDeleteVersion(modId, version.backupId);
      versions = versions.filter((row) => row.backupId !== version.backupId);
      toasts.show("Копия удалена");
    } catch (reason) {
      toasts.show(
        reason && typeof reason === "object" && "message" in reason
          ? String((reason as { message: unknown }).message)
          : "Копию удалить не удалось",
      );
    } finally {
      confirming = null;
      deleting = false;
    }
  }

  $effect(() => {
    const id = modId;
    // Чтение ключа делает эффект зависимым от него.
    void versionsKey;
    const current = ++token;
    versions = [];
    loading = true;
    void modVersions(id)
      .then((rows) => {
        if (current === token) versions = rows;
      })
      .catch(() => {})
      .finally(() => {
        if (current === token) loading = false;
      });
  });
</script>

{#if loading || versions.length > 0}
  <section>
    <h3>Прошлые версии</h3>
    {#if loading}
      <p class="muted">Читаем копии…</p>
    {:else}
      <div class="rows">
        {#each versions as version (version.backupId)}
          <div class="row">
            <div class="text">
              <b>{version.version ? `Версия ${version.version}` : "Прошлая копия"}</b>
              <span>{formatDateTime(version.savedAt)} · {formatSize(version.sizeBytes)}</span>
            </div>
            <div class="actions">
              <Button variant="outline" size="sm" onclick={() => onRestore(version)}>Вернуть</Button>
              <button
                class="delete"
                class:confirm={confirming === version.backupId}
                type="button"
                disabled={deleting}
                onclick={() => void remove(version)}
                onblur={() => {
                  if (confirming === version.backupId) confirming = null;
                }}
              >
                {confirming === version.backupId ? "Точно удалить" : "Удалить"}
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<style>
  section {
    padding-top: 18px;
  }

  h3 {
    margin: 0 0 8px;
    color: var(--faint-foreground);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .rows {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }

  .text {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
    font-size: 13px;
  }

  .text b {
    font-weight: 600;
  }

  .text span {
    color: var(--muted-foreground);
    font-size: 11.5px;
  }

  .actions {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 6px;
  }

  .delete {
    padding: 4px 6px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--muted-foreground);
    font-size: 12px;
    cursor: pointer;
  }

  .delete:hover,
  .delete.confirm {
    color: var(--danger);
  }

  .delete.confirm {
    background: var(--danger-soft);
    font-weight: 600;
  }

  .muted {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 13px;
  }
</style>

<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import { modVersions, type VersionRow } from "$lib/ipc/library";
  import { formatDateTime, formatSize } from "./format";

  let {
    modId,
    onRestore,
  }: {
    modId: string;
    onRestore: (version: VersionRow) => void;
  } = $props();

  let versions = $state<VersionRow[]>([]);
  let loading = $state(true);
  let token = 0;

  $effect(() => {
    const id = modId;
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
            <Button variant="outline" size="sm" onclick={() => onRestore(version)}>Вернуть</Button>
          </div>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<style>
  section {
    padding-top: 12px;
  }

  h3 {
    margin: 0 0 8px;
    color: var(--muted-foreground);
    font-size: 9.5px;
    font-weight: 650;
    letter-spacing: 0.1em;
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
    border-radius: 12px;
    background: color-mix(in oklab, var(--foreground) 3%, transparent);
  }

  .text {
    display: flex;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
    font-size: 10.5px;
  }

  .text b {
    font-weight: 600;
  }

  .text span {
    color: var(--muted-foreground);
    font-size: 9.5px;
  }

  .muted {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 10.5px;
  }
</style>

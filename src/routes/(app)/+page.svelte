<script lang="ts">
  import { onMount } from "svelte";
  import { bootstrap } from "$lib/ipc/client";
  import type { BootstrapDto } from "$lib/ipc/types";
  import StagePlaceholder from "$lib/shell/StagePlaceholder.svelte";

  let info = $state<BootstrapDto | null>(null);
  let error = $state<string | null>(null);

  onMount(() => {
    bootstrap()
      .then((value) => (info = value))
      .catch((reason: unknown) => (error = String(reason)));
  });
</script>

<StagePlaceholder
  title="Моды"
  text="Каркас M0. Здесь появится гараж: карточки, фильтры, включение и выключение, правая панель."
/>

{#if info}
  <section class="bootstrap glass liquid-edge">
    <p>Rust IPC отвечает: {info.appVersion} · {info.os}/{info.arch}</p>
    <p class="mono">{info.dataDir}</p>
  </section>
{:else if error}
  <section class="bootstrap glass liquid-edge error">{error}</section>
{/if}

<style>
  .bootstrap {
    margin-top: 12px;
    border-radius: 16px;
    padding: 14px 18px;
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .bootstrap p {
    margin: 0;
  }

  .mono {
    margin-top: 4px !important;
    font-family: "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
    font-size: 10px;
  }

  .error {
    color: oklch(0.72 0.18 25);
  }
</style>

<script lang="ts">
  import { page } from "$app/state";
  import { toasts } from "./toasts.svelte";
  import { pop } from "./transitions";
  import { CtrlHold, hintsFor } from "./ctrlHold.svelte";

  // Держат Ctrl один — внизу видно, какие сочетания есть на этой странице.
  const hold = new CtrlHold();
  const hints = $derived(hintsFor(page.url.pathname, toasts.canUndo));

  function onblur() {
    hold.cancel();
  }
</script>

<svelte:window
  onkeydown={(event) => hold.keydown(event)}
  onkeyup={(event) => hold.keyup(event)}
  onpointerdown={() => hold.cancel()}
  {onblur}
/>

{#if hold.shown}
  <div
    class="hints glass solid"
    class:above-toast={toasts.current !== null}
    role="note"
    aria-label="Горячие клавиши"
    transition:pop={{ duration: 200, y: 8 }}
  >
    {#each hints as hint (hint.keys)}
      <span><kbd>{hint.keys}</kbd>{hint.text}</span>
    {/each}
  </div>
{/if}

<style>
  .hints {
    position: fixed;
    left: 96px;
    bottom: 20px;
    z-index: 65;
    display: flex;
    flex-wrap: wrap;
    gap: 6px 14px;
    max-width: min(560px, calc(100vw - 120px));
    padding: 10px 14px;
    border-radius: 10px;
    color: var(--muted-foreground);
    font-size: 12.5px;
    pointer-events: none;
  }

  /* Тост внизу по центру: на узком окне подсказка встала бы на него. */
  .hints.above-toast {
    bottom: 76px;
  }

  .hints span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  kbd {
    padding: 1px 6px;
    border: 1px solid var(--border-strong);
    border-radius: 4px;
    background: var(--surface-2);
    color: var(--foreground);
    font-family: var(--font-mono, monospace);
    font-size: 11.5px;
  }
</style>

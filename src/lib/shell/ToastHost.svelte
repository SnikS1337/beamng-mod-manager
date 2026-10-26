<script lang="ts">
  import { toasts } from "./toasts.svelte";
  import { pop, softFade } from "./transitions";
</script>

{#each toasts.current ? [toasts.current] : [] as toast (toast)}
  <div
    class="toast"
    role="status"
    aria-live="polite"
    in:pop={{ duration: 240, y: 10, centered: "x" }}
    out:softFade={{ duration: 140 }}
  >
    <span>{toast.message}</span>
    {#if toast.undo}
      <button
        type="button"
        onclick={() => {
          if (toasts.current === toast) void toasts.undo();
        }}
      >Отменить</button>
    {/if}
  </div>
{/each}

<style>
  .toast {
    position: fixed;
    left: calc(50% + 38px);
    bottom: 22px;
    z-index: 70;
    display: flex;
    align-items: center;
    gap: 14px;
    max-width: min(640px, calc(100vw - 180px));
    padding: 10px 12px 10px 16px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface-2);
    box-shadow: var(--shadow-2);
    color: var(--foreground);
    font-size: 13px;
    font-weight: 600;
    transform: translateX(-50%);
  }

  .toast span {
    min-width: 0;
    line-height: 1.4;
  }

  .toast button {
    flex: none;
    height: 28px;
    padding: 0 10px;
    border: 0;
    border-radius: 6px;
    background: var(--accent-brand-soft);
    color: var(--accent-brand-text);
    font-size: 12.5px;
    font-weight: 700;
    cursor: default;
  }

  .toast button:hover {
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }
</style>

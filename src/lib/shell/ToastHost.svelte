<script lang="ts">
  import { toasts } from "./toasts.svelte";
</script>

{#if toasts.current}
  {#key toasts.current}
    <div class="toast" role="status" aria-live="polite">
      <span>{toasts.current.message}</span>
      {#if toasts.current.undo}
        <button type="button" onclick={() => void toasts.undo()}>Отменить</button>
      {/if}
    </div>
  {/key}
{/if}

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
    animation: toast-in var(--dur-3) var(--ease-out) both;
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

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translate(-50%, 10px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }
</style>

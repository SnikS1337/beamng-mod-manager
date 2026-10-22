<script lang="ts">
  import { toasts } from "./toasts.svelte";
</script>

{#if toasts.current}
  <div class="toast glass liquid-edge" role="status" aria-live="polite">
    <span>{toasts.current.message}</span>
    {#if toasts.current.undo}
      <button type="button" onclick={() => void toasts.undo()}>Отменить</button>
    {/if}
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    left: 50%;
    bottom: 26px;
    z-index: 70;
    display: flex;
    align-items: center;
    gap: 14px;
    max-width: min(620px, calc(100vw - 150px));
    padding: 10px 14px;
    border-radius: 999px;
    color: var(--foreground);
    font-size: 11px;
    transform: translateX(-50%);
    animation: toast-in 0.2s ease-out;
  }

  .toast span {
    min-width: 0;
    line-height: 1.4;
  }

  .toast button {
    flex: none;
    border: 0;
    background: transparent;
    color: var(--primary);
    font-size: 10.5px;
    font-weight: 600;
    cursor: default;
  }

  @keyframes toast-in {
    from {
      opacity: 0;
      transform: translate(-50%, 7px);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }
</style>

<script lang="ts">
  import { ringTiming, toasts } from "./toasts.svelte";
  import { pop, softFade } from "./transitions";

  const OUT = 140;

  /** Следующий тост ждёт, пока прошлый погаснет: иначе два текста секунду лежали друг на друге. */
  function enterDelay(): number {
    return Math.max(0, OUT - (Date.now() - toasts.hiddenAt));
  }
</script>

<!-- Ключ — сообщение, а не объект: пауза под курсором пересоздаёт объект, и тост
     заново «выпрыгивал». -->
{#each toasts.current ? [toasts.current] : [] as toast (toast.message + (toast.undo ? "↶" : ""))}
  <div
    class="toast"
    role="status"
    aria-live="polite"
    in:pop={{ duration: 240, y: 10, centered: "x", delay: enterDelay() }}
    out:softFade={{ duration: OUT }}
    onpointerenter={() => toasts.pause()}
    onpointerleave={() => toasts.resume()}
    onfocusin={() => toasts.pause()}
    onfocusout={() => toasts.resume()}
  >
    <span>{toast.message}</span>
    {#if toast.undo}
      {@const ring = ringTiming(toast.total ?? 1, toast.left ?? toast.total ?? 1)}
      <button
        type="button"
        onclick={() => {
          if (toasts.current?.message === toast.message) void toasts.undo();
        }}
      >
        <!-- Сколько ещё можно отменить: кольцо убывает и стоит, пока курсор на тосте. -->
        <svg class="ring" class:paused={!toast.timer} viewBox="0 0 16 16" aria-hidden="true">
          <circle cx="8" cy="8" r="6" />
          <circle
            class="left"
            cx="8"
            cy="8"
            r="6"
            pathLength="100"
            style={`animation-duration:${ring.duration}ms; animation-delay:${ring.delay}ms`}
          />
        </svg>
        Отменить
      </button>
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

  .toast button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .ring {
    width: 14px;
    height: 14px;
    transform: rotate(-90deg);
  }

  .ring circle {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    opacity: 0.25;
  }

  .ring .left {
    opacity: 1;
    stroke-dasharray: 100;
    stroke-linecap: round;
    animation-name: ring-drain;
    animation-timing-function: linear;
    animation-fill-mode: both;
  }

  .ring.paused .left {
    animation-play-state: paused;
  }

  @keyframes ring-drain {
    from {
      stroke-dashoffset: 0;
    }
    to {
      stroke-dashoffset: 100;
    }
  }

  /* «Быстро» оборвал бы кольцо в пустое: без движения его просто нет. */
  :global(.no-motion) .ring {
    display: none;
  }

  .toast button:hover {
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }
</style>

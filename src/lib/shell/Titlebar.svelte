<script lang="ts">
  import { Minus, Square, X } from "@lucide/svelte";
  import { windowAction } from "$lib/ipc/window";
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <span class="cube" aria-hidden="true"></span>
    <span class="name" data-tauri-drag-region>ModManager BeamNG</span>
    <span class="tag">1.0</span>
  </div>

  <div class="window-capsule glass liquid-edge">
    <button type="button" aria-label="Свернуть" onclick={() => void windowAction("minimize")}>
      <Minus size={14} strokeWidth={1.8} />
    </button>
    <button type="button" aria-label="Развернуть" onclick={() => void windowAction("maximize")}>
      <Square size={12} strokeWidth={1.8} />
    </button>
    <button
      type="button"
      class="close"
      aria-label="Закрыть"
      onclick={() => void windowAction("close")}
    >
      <X size={14} strokeWidth={1.8} />
    </button>
  </div>
</header>

<style>
  .titlebar {
    position: absolute;
    inset: 0 0 auto 0;
    z-index: 30;
    display: flex;
    align-items: center;
    gap: 14px;
    height: 44px;
    padding: 0 12px 0 16px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }

  .cube {
    width: 17px;
    height: 17px;
    border-radius: 5px;
    background:
      linear-gradient(140deg, rgb(255 255 255 / 0.75), transparent 55%),
      linear-gradient(320deg, oklch(0.6 0.19 278), oklch(0.52 0.15 210));
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.6),
      0 4px 12px oklch(0.55 0.18 278 / 0.4);
    transform: rotate(8deg);
  }

  .name {
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.01em;
    white-space: nowrap;
  }

  .tag {
    padding: 2px 6px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--primary) 20%, transparent);
    color: var(--primary);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .window-capsule {
    margin-left: auto;
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
  }

  .window-capsule button {
    display: grid;
    place-items: center;
    width: 32px;
    height: 25px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: default;
    transition:
      background 160ms ease,
      color 160ms ease;
  }

  .window-capsule button:hover {
    background: color-mix(in oklab, var(--foreground) 12%, transparent);
    color: var(--foreground);
  }

  .window-capsule .close:hover {
    background: oklch(0.59 0.21 25);
    color: white;
  }
</style>

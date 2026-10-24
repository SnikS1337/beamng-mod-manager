<script lang="ts">
  import { Minus, Square, X } from "@lucide/svelte";
  import { windowAction } from "#lib/ipc/window.js";
</script>

<!-- Своей полосы заголовка нет (как в legacy): окно таскается за верхнюю кромку и за
     заголовки разделов, кнопки окна — стеклянная капсула в правом верхнем углу. -->
<div class="drag-strip" data-tauri-drag-region></div>

<div class="window-capsule glass liquid-edge">
  <button type="button" aria-label="Свернуть" onclick={() => void windowAction("minimize")}>
    <Minus size={15} strokeWidth={1.7} />
  </button>
  <button type="button" aria-label="Развернуть" onclick={() => void windowAction("maximize")}>
    <Square size={12} strokeWidth={1.7} />
  </button>
  <button type="button" class="close" aria-label="Закрыть" onclick={() => void windowAction("close")}>
    <X size={15} strokeWidth={1.7} />
  </button>
</div>

<style>
  .drag-strip {
    position: absolute;
    inset: 0 0 auto 0;
    z-index: 25;
    height: 14px;
  }

  .window-capsule {
    position: absolute;
    top: 16px;
    right: 16px;
    z-index: 30;
    display: flex;
    gap: 0;
    height: 36px;
    padding: 0;
    border-radius: 999px;
    overflow: hidden;
  }

  .window-capsule button {
    display: grid;
    place-items: center;
    width: 44px;
    height: 36px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: color-mix(in oklab, var(--foreground) 78%, transparent);
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

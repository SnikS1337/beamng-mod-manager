<script lang="ts">
  import Minus from "phosphor-svelte/lib/Minus";
  import Square from "phosphor-svelte/lib/Square";
  import X from "phosphor-svelte/lib/X";
  import { windowAction } from "#lib/ipc/window.js";
</script>

<!-- Своей полосы заголовка нет: окно таскается за верхнюю кромку, за полосу разделов и за
     заголовки экранов; кнопки окна — в правом верхнем углу. -->
<div class="drag-strip" data-tauri-drag-region></div>

<div class="window-buttons">
  <button type="button" aria-label="Свернуть" onclick={() => void windowAction("minimize")}>
    <Minus size={15} />
  </button>
  <button type="button" aria-label="Развернуть" onclick={() => void windowAction("maximize")}>
    <Square size={13} />
  </button>
  <button type="button" class="close" aria-label="Закрыть" onclick={() => void windowAction("close")}>
    <X size={15} />
  </button>
</div>

<style>
  .drag-strip {
    position: absolute;
    inset: 0 0 auto 0;
    z-index: 25;
    height: 10px;
  }

  .window-buttons {
    position: absolute;
    top: 0;
    right: 0;
    z-index: 30;
    display: flex;
  }

  .window-buttons button {
    display: grid;
    place-items: center;
    width: 46px;
    height: 34px;
    border: 0;
    background: transparent;
    color: var(--muted-foreground);
    cursor: default;
    transition:
      background-color var(--dur-1) var(--ease-out),
      color var(--dur-1) var(--ease-out);
  }

  .window-buttons button:hover {
    background: var(--surface-3);
    color: var(--foreground);
  }

  .window-buttons .close:hover {
    background: #d9362b;
    color: #fff;
  }
</style>

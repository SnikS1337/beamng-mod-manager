<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { Minus, Square, X } from "@lucide/svelte";
  import { windowAction } from "$lib/ipc/window";

  let version = $state("1.0");

  onMount(() => {
    void getVersion()
      .then((value) => {
        version = value;
      })
      .catch(() => {});
  });
</script>

<header class="titlebar" data-tauri-drag-region>
  <div class="brand" data-tauri-drag-region>
    <img class="brand-icon" src="/app-icon.png" alt="" />
    <span class="name" data-tauri-drag-region>ModManager BeamNG</span>
    <span class="tag">{version}</span>
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

  .brand-icon {
    width: 20px;
    height: 20px;
    border-radius: 6px;
    box-shadow:
      0 0 0 1px rgb(255 255 255 / 0.18),
      0 4px 12px rgb(6 10 22 / 0.35);
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

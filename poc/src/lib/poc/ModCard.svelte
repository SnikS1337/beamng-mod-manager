<script lang="ts">
  import { Switch } from "$lib/components/ui/switch";
  import type { ModEntry } from "./api";

  let {
    mod,
    selected,
    onselect,
    ontoggle,
  }: {
    mod: ModEntry;
    selected: boolean;
    onselect: (id: number) => void;
    ontoggle: (id: number) => void;
  } = $props();

  const sourceLabel: Record<ModEntry["source"], string> = {
    repo: "Репозиторий",
    modland: "Modland",
    stmods: "STMods",
    modsgaming: "ModsGaming",
  };

  let hue = $derived((mod.id * 37) % 360);
  let hue2 = $derived((hue + 58) % 360);
  let thumbStyle = $derived(
    `background: radial-gradient(120% 90% at 18% 0%, hsl(${hue} 85% 68% / 0.9), transparent 62%), radial-gradient(100% 80% at 90% 100%, hsl(${hue2} 80% 55% / 0.55), transparent 55%), linear-gradient(135deg, hsl(${hue} 40% 30%), hsl(${hue2} 45% 17%))`,
  );

  function select() {
    onselect(mod.id);
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      select();
    }
  }
</script>

<div
  class="card"
  class:selected
  class:sleeping={!mod.enabled}
  role="button"
  tabindex="0"
  aria-pressed={selected}
  aria-label={mod.title}
  onclick={select}
  onkeydown={onkeydown}
>
  <div class="thumb" style={thumbStyle}>
    <span class="beam"></span>
    <span class="source">{sourceLabel[mod.source]}</span>
    {#if mod.updateAvailable}
      <span class="update">Есть обновление</span>
    {/if}
  </div>

  <div class="body">
    <div class="titles">
      <p class="title">{mod.title}</p>
      <p class="file">{mod.file}</p>
    </div>

    <div class="meta">
      <span class="version">v{mod.version}</span>
      <span>{Math.round(mod.sizeMb)} МБ</span>
    </div>
  </div>

  <div class="toggle">
    <Switch
      checked={mod.enabled}
      onCheckedChange={() => ontoggle(mod.id)}
      onclick={(event: MouseEvent) => event.stopPropagation()}
      aria-label={mod.enabled ? "Выключить мод" : "Включить мод"}
    />
  </div>
</div>

<style>
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 236px;
    padding: 7px;
    border: 1px solid var(--glass-border);
    border-radius: 20px;
    background: var(--glass-fill);
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      0 12px 30px rgb(0 0 0 / 0.14);
    cursor: default;
    overflow: hidden;
    transition:
      transform 220ms cubic-bezier(0.22, 1, 0.36, 1),
      border-color 160ms ease,
      box-shadow 220ms ease;
  }

  .card:hover {
    transform: translateY(-2px);
    border-color: color-mix(in oklab, var(--primary) 40%, var(--glass-border));
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      0 18px 38px rgb(0 0 0 / 0.22);
  }

  .card:focus-visible {
    outline: 2px solid var(--ring);
    outline-offset: 2px;
  }

  .card.selected {
    border-color: color-mix(in oklab, var(--primary) 70%, white 10%);
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      0 0 0 1px color-mix(in oklab, var(--primary) 55%, transparent),
      0 20px 44px rgb(0 0 0 / 0.26);
  }

  .thumb {
    position: relative;
    flex: 0 0 124px;
    border-radius: 14px;
    overflow: hidden;
    transition: filter 260ms ease;
  }

  .sleeping .thumb {
    filter: grayscale(0.9) brightness(0.72);
  }

  .beam {
    position: absolute;
    inset: -20% -10%;
    background: linear-gradient(
      115deg,
      transparent 38%,
      rgb(255 255 255 / 0.22) 46%,
      transparent 54%
    );
    transform: translateX(-30%);
    animation: sheen 6s ease-in-out infinite;
  }

  @keyframes sheen {
    0%,
    70%,
    100% {
      transform: translateX(-40%);
    }
    85% {
      transform: translateX(40%);
    }
  }

  .source,
  .update {
    position: absolute;
    top: 8px;
    padding: 3px 7px;
    border-radius: 999px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.02em;
    backdrop-filter: blur(8px);
    -webkit-backdrop-filter: blur(8px);
  }

  .source {
    left: 8px;
    background: rgb(10 14 25 / 0.42);
    color: rgb(255 255 255 / 0.82);
  }

  .update {
    right: 8px;
    background: oklch(0.74 0.14 278 / 0.92);
    color: oklch(0.18 0.025 270);
  }

  .body {
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    flex: 1;
    min-height: 0;
    padding: 10px 7px 5px;
  }

  .titles {
    min-width: 0;
    padding-right: 42px;
  }

  .title {
    margin: 0;
    overflow: hidden;
    color: var(--foreground);
    font-size: 13px;
    font-weight: 600;
    line-height: 1.25;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file {
    margin: 3px 0 0;
    overflow: hidden;
    color: var(--muted-foreground);
    font-family: "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
    font-size: 9.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    display: flex;
    gap: 10px;
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .version {
    color: var(--primary);
    font-weight: 600;
  }

  .toggle {
    position: absolute;
    right: 14px;
    bottom: 14px;
  }
</style>

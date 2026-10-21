<script lang="ts">
  import { Switch } from "$lib/components/ui/switch";
  import { modPreview, type ModCardDto } from "$lib/ipc/library";
  import { formatSize, kindLabel } from "./format";

  let {
    mod,
    selected,
    onselect,
    ontoggle,
  }: {
    mod: ModCardDto;
    selected: boolean;
    onselect: (id: string) => void;
    ontoggle: (mod: ModCardDto) => void;
  } = $props();

  let preview = $state<string | null>(null);
  let requested = $state("");

  $effect(() => {
    const id = mod.id;
    if (mod.previewUrl) {
      preview = mod.previewUrl;
      return;
    }
    if (requested === id) return;
    requested = id;
    preview = null;
    void modPreview(id)
      .then((url) => {
        if (requested === id) preview = url;
      })
      .catch(() => {});
  });

  const hue = $derived(hashHue(mod.id));
  const placeholder = $derived(
    `radial-gradient(120% 90% at 18% 0%, hsl(${hue} 85% 68% / 0.85), transparent 62%), radial-gradient(100% 80% at 90% 100%, hsl(${(hue + 58) % 360} 80% 55% / 0.5), transparent 55%), linear-gradient(135deg, hsl(${hue} 38% 28%), hsl(${(hue + 58) % 360} 42% 16%))`,
  );

  function hashHue(id: string): number {
    let hash = 0;
    for (let index = 0; index < id.length; index += 1) {
      hash = (hash * 31 + id.charCodeAt(index)) % 360;
    }
    return hash;
  }

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
  class:pending={mod.pending === "queued"}
  role="button"
  tabindex="0"
  aria-pressed={selected}
  aria-label={mod.name}
  onclick={select}
  onkeydown={onkeydown}
>
  <div class="thumb" style={preview ? undefined : `background:${placeholder}`}>
    {#if preview}
      <img src={preview} alt="" loading="lazy" />
    {:else}
      <span class="glyph">{kindLabel(mod.kind)}</span>
    {/if}
    <span class="source">{kindLabel(mod.kind)}</span>
    {#if mod.update !== "none"}
      <span class="update">Есть обновление</span>
    {:else if !mod.enabled}
      <span class="off">Выключен</span>
    {/if}
  </div>

  <div class="body">
    <div class="titles">
      <p class="title">{mod.name}</p>
      <p class="file">{mod.fileName}</p>
    </div>

    <div class="meta">
      {#if mod.version}
        <span class="version">{mod.version}</span>
      {/if}
      <span>{formatSize(mod.sizeBytes)}</span>
      {#if mod.compat.differs}
        <span class="compat">{mod.compat.modFor}</span>
      {/if}
    </div>
  </div>

  {#if mod.pending === "queued"}
    <p class="pending-note">Применится после игры</p>
  {/if}

  <div class="toggle">
    <Switch
      checked={mod.enabled}
      onCheckedChange={() => ontoggle(mod)}
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

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .glyph {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: rgb(255 255 255 / 0.2);
    font-size: 20px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .sleeping .thumb {
    filter: grayscale(0.9) brightness(0.72);
  }

  .source,
  .update,
  .off {
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

  .off {
    right: 8px;
    background: rgb(10 14 25 / 0.5);
    color: rgb(255 255 255 / 0.8);
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
    align-items: center;
    color: var(--muted-foreground);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }

  .version {
    color: var(--primary);
    font-weight: 600;
  }

  .compat {
    color: oklch(0.82 0.15 85);
  }

  .pending-note {
    position: absolute;
    left: 12px;
    bottom: 12px;
    margin: 0;
    color: var(--muted-foreground);
    font-size: 9px;
  }

  .toggle {
    position: absolute;
    right: 14px;
    bottom: 14px;
  }
</style>

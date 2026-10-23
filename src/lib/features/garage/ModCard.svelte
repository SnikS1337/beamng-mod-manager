<script lang="ts">
  import { Car, FileCode, Map, Music, Package } from "@lucide/svelte";
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

  const icon = $derived(
    mod.kind === "vehicle"
      ? Car
      : mod.kind === "level"
        ? Map
        : mod.kind === "sound"
          ? Music
          : mod.kind === "script"
            ? FileCode
            : Package,
  );

  const status = $derived(
    mod.deleted
      ? { text: "Удалён", class: "deleted" }
      : mod.update === "likely"
        ? { text: "Есть обновление", class: "update" }
        : mod.update === "possible"
          ? { text: "Возможно, обновлён", class: "possible" }
          : !mod.enabled
            ? { text: "Выключен", class: "off" }
            : null,
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
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      select();
    }
  }
</script>

<div
  class="card"
  class:selected
  class:sleeping={!mod.enabled && !mod.deleted}
  class:deleted={mod.deleted}
  class:pending={mod.pending === "queued"}
  role="button"
  tabindex="0"
  aria-pressed={selected}
  aria-label={mod.name}
  title={mod.fileName}
  onclick={select}
  onkeydown={onkeydown}
>
  <div class="thumb" style={preview ? undefined : `background:${placeholder}`}>
    {#if preview}
      <img src={preview} alt="" loading="lazy" />
    {:else}
      {@const Icon = icon}
      <span class="placeholder">
        <Icon size={34} strokeWidth={1.4} />
        <span>{kindLabel(mod.kind)}</span>
      </span>
    {/if}

    {#if mod.pending === "queued"}
      <span class="badge pending-badge">После игры</span>
    {/if}
    {#if status}
      <span class="badge {status.class}">{status.text}</span>
    {/if}

    <div class="caption">
      <div class="titles">
        <p class="title">{mod.name}</p>
        <p class="meta">
          {#if mod.version}
            <span class="version">{mod.version}</span>
            <span class="dot">·</span>
          {/if}
          <span>{formatSize(mod.sizeBytes)}</span>
          {#if mod.deleted}
            <span class="dot">·</span>
            <span>{mod.copies > 1 ? `${mod.copies} копии` : "копия"}</span>
          {:else if mod.compat.differs}
            <span class="dot">·</span>
            <span class="compat">{mod.compat.modFor}</span>
          {/if}
        </p>
      </div>
      {#if !mod.deleted}
        <div class="toggle">
          <Switch
            checked={mod.enabled}
            onCheckedChange={() => ontoggle(mod)}
            onclick={(event: MouseEvent) => event.stopPropagation()}
            aria-label={mod.enabled ? "Выключить мод" : "Включить мод"}
          />
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .card {
    position: relative;
    height: 204px;
    border: 1px solid var(--glass-border);
    border-radius: 18px;
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
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    transition: transform 320ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .card:hover .thumb img {
    transform: scale(1.035);
  }

  .placeholder {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 7px;
    color: rgb(255 255 255 / 0.46);
    font-size: 9.5px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .sleeping .thumb {
    filter: grayscale(0.85) brightness(0.7);
  }

  .deleted .thumb {
    filter: grayscale(0.7) brightness(0.58);
  }

  .deleted {
    border-color: color-mix(in oklab, var(--foreground) 14%, transparent);
  }

  .badge {
    position: absolute;
    top: 10px;
    padding: 4px 8px;
    border-radius: 999px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.01em;
    backdrop-filter: blur(10px);
    -webkit-backdrop-filter: blur(10px);
  }

  .badge.update {
    right: 10px;
    background: oklch(0.74 0.14 278 / 0.94);
    color: oklch(0.17 0.025 270);
  }

  .badge.possible {
    right: 10px;
    background: oklch(0.82 0.15 85 / 0.94);
    color: oklch(0.2 0.03 70);
  }

  .badge.off,
  .badge.deleted {
    right: 10px;
    background: rgb(8 12 22 / 0.58);
    color: rgb(255 255 255 / 0.82);
  }

  .badge.pending-badge {
    left: 10px;
    background: rgb(8 12 22 / 0.58);
    color: rgb(255 255 255 / 0.82);
  }

  .caption {
    position: absolute;
    inset: auto 0 0;
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 8px;
    padding: 42px 12px 12px;
    background: linear-gradient(
      to top,
      rgb(4 7 14 / 0.94),
      rgb(4 7 14 / 0.62) 55%,
      transparent
    );
  }

  .titles {
    min-width: 0;
    color: #fff;
  }

  .title {
    margin: 0;
    overflow: hidden;
    font-size: 13px;
    font-weight: 620;
    line-height: 1.25;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-shadow: 0 1px 8px rgb(0 0 0 / 0.35);
  }

  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    margin: 3px 0 0;
    overflow: hidden;
    color: rgb(255 255 255 / 0.68);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .version {
    color: rgb(255 255 255 / 0.9);
    font-weight: 600;
  }

  .compat {
    color: oklch(0.86 0.11 85);
  }

  .dot {
    color: rgb(255 255 255 / 0.38);
  }

  .toggle {
    flex: none;
    filter: drop-shadow(0 2px 6px rgb(0 0 0 / 0.45));
  }
</style>

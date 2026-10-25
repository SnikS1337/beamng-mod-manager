<script lang="ts">
  import Car from "phosphor-svelte/lib/Car";
  import { Switch } from "#lib/components/ui/switch/index.js";
  import type { ModCardDto } from "#lib/ipc/library.js";
  import { copiesBadge } from "./duplicates";
  import { formatSize } from "./format";
  import { modStatus } from "./status";
  import { typeLabel, typeTag } from "./view";

  let {
    items,
    selectedIds,
    onselect,
    ontoggle,
    resetKey = "",
  }: {
    items: ModCardDto[];
    /** Выбранные: одна карточка или несколько. */
    selectedIds: string[];
    onselect: (id: string, keys: { ctrl: boolean; shift: boolean }) => void;
    ontoggle: (mod: ModCardDto) => void;
    resetKey?: string;
  } = $props();

  // Плотный список для сотен модов: строка 48, рисуются только видимые.
  const ROW = 48;

  let viewport = $state<HTMLDivElement | null>(null);
  let height = $state(0);
  let scrollTop = $state(0);

  let start = $derived(Math.max(0, Math.floor(scrollTop / ROW) - 4));
  let end = $derived(Math.min(items.length, Math.ceil((scrollTop + Math.max(height, 1)) / ROW) + 4));
  let visible = $derived(items.slice(start, end));

  $effect(() => {
    resetKey;
    if (viewport) {
      viewport.scrollTop = 0;
      scrollTop = 0;
    }
  });

  function onkeydown(event: KeyboardEvent, id: string) {
    if (event.target !== event.currentTarget) return;
    if (event.key === "Enter") {
      event.preventDefault();
      onselect(id, { ctrl: event.ctrlKey || event.metaKey, shift: event.shiftKey });
    }
  }
</script>

<div class="list">
  <div class="head" role="row">
    <span></span>
    <span>Название</span>
    <span>Тип</span>
    <span>Версия</span>
    <span>Размер</span>
    <span>Состояние</span>
    <span class="end">Вкл.</span>
  </div>
  <div
    class="scroll-area viewport"
    role="grid"
    aria-label="Моды"
    bind:this={viewport}
    bind:clientHeight={height}
    onscroll={(event) => (scrollTop = (event.currentTarget as HTMLDivElement).scrollTop)}
  >
    <div style={`height:${items.length * ROW}px; position:relative`}>
      {#each visible as mod, offset (mod.id)}
        {@const tag = typeTag(mod)}
        {@const status = modStatus(mod)}
        {@const copies = copiesBadge(mod)}
        <div
          class="row"
          class:selected={selectedIds.includes(mod.id)}
          class:sleeping={!mod.enabled && !mod.deleted}
          style={`top:${(start + offset) * ROW}px`}
          data-mod-id={mod.id}
          role="row"
          tabindex="0"
          aria-selected={selectedIds.includes(mod.id)}
          title={mod.fileName}
          onclick={(event) =>
            onselect(mod.id, { ctrl: event.ctrlKey || event.metaKey, shift: event.shiftKey })}
          onkeydown={(event) => onkeydown(event, mod.id)}
        >
          <span class="thumb" class:addon={mod.role === "addon"}>
            {#if mod.previewUrl && mod.role !== "addon"}
              <img src={mod.previewUrl} alt="" loading="lazy" decoding="async" />
            {:else}
              <Car size={16} weight="thin" />
            {/if}
          </span>
          <span class="name">
            <b>{mod.name}</b>
            <small>{mod.fileName}</small>
          </span>
          <span><span class="kind-tag {tag}">{typeLabel(tag)}</span></span>
          <span class="mono">{mod.version ?? "—"}</span>
          <span class="muted">{formatSize(mod.sizeBytes)}</span>
          <span class="states">
            {#if status}<span class="status {status.tone}">{status.text}</span>{/if}
            {#if copies}<span class="status copies" class:conflict={mod.groupConflict}>{copies}</span>{/if}
          </span>
          <span class="end">
            {#if !mod.deleted}
              <Switch
                checked={mod.enabled}
                onCheckedChange={() => ontoggle(mod)}
                onclick={(event: MouseEvent) => event.stopPropagation()}
                aria-label={mod.enabled ? "Выключить мод" : "Включить мод"}
              />
            {/if}
          </span>
        </div>
      {/each}
    </div>
  </div>
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    padding: 8px 24px 0;
    box-sizing: border-box;
  }

  .head,
  .row {
    display: grid;
    grid-template-columns: 64px minmax(180px, 1fr) 120px 90px 80px minmax(140px, 220px) 56px;
    align-items: center;
    gap: 12px;
    padding: 0 10px;
  }

  .head {
    height: 34px;
    border-bottom: 1px solid var(--border);
    color: var(--faint-foreground);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .viewport {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }

  .row {
    position: absolute;
    left: 0;
    right: 0;
    height: 48px;
    border-bottom: 1px solid var(--border);
    cursor: default;
    transition: background-color var(--dur-1) var(--ease-out);
  }

  .row:hover {
    background: var(--surface);
  }

  .row.selected {
    background: var(--accent-brand-soft);
    box-shadow: inset 3px 0 0 var(--accent-brand);
  }

  .row:focus-visible {
    outline: 2px solid var(--ring);
    outline-offset: -2px;
  }

  .thumb {
    display: grid;
    place-items: center;
    width: 56px;
    height: 34px;
    overflow: hidden;
    border-radius: 5px;
    background: var(--surface-2);
    color: var(--faint-foreground);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .thumb.addon {
    background: repeating-linear-gradient(135deg, var(--surface-2) 0 6px, var(--surface-3) 6px 12px);
  }

  .name {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name b,
  .name small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .name b {
    font-weight: 700;
  }

  .name small {
    color: var(--faint-foreground);
    font-size: 11.5px;
  }

  .muted {
    color: var(--muted-foreground);
  }

  .states {
    display: flex;
    gap: 6px;
    overflow: hidden;
  }

  .status {
    display: inline-flex;
    align-items: center;
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    color: var(--muted-foreground);
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
  }

  .status.update {
    border-color: var(--accent-brand);
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .status.possible {
    border-color: var(--warning-border);
    color: var(--warning);
  }

  .status.problem,
  .status.copies.conflict {
    border-color: color-mix(in oklab, var(--danger) 45%, transparent);
    color: var(--danger);
  }

  .end {
    display: flex;
    justify-content: flex-end;
  }

  .sleeping b {
    color: var(--muted-foreground);
  }

  .sleeping .thumb img {
    filter: grayscale(0.9) brightness(0.65);
  }
</style>

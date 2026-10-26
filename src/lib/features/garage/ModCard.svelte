<script lang="ts">
  import Car from "phosphor-svelte/lib/Car";
  import MapTrifold from "phosphor-svelte/lib/MapTrifold";
  import PuzzlePiece from "phosphor-svelte/lib/PuzzlePiece";
  import FileCode from "phosphor-svelte/lib/FileCode";
  import SpeakerHigh from "phosphor-svelte/lib/SpeakerHigh";
  import Package from "phosphor-svelte/lib/Package";
  import Copy from "phosphor-svelte/lib/Copy";
  import { Switch } from "#lib/components/ui/switch/index.js";
  import { modPreview, type ModCardDto } from "#lib/ipc/library.js";
  import { copiesBadge } from "./duplicates";
  import { addonCaption, compatNote, versionLine } from "./cardText";
  import { formatSize, plural } from "./format";
  import { typeLabel, typeTag } from "./view";
  import { modStatus } from "./status";
  import { flowName } from "#lib/shell/motion.svelte.js";

  let {
    mod,
    selected,
    index = 0,
    appear = false,
    onselect,
    ontoggle,
  }: {
    mod: ModCardDto;
    selected: boolean;
    /** Порядок в видимой части — для небольшой задержки появления. */
    index?: number;
    /** Появиться с анимацией (первый показ списка, смена фильтра). */
    appear?: boolean;
    onselect: (id: string, keys: { ctrl: boolean; shift: boolean }) => void;
    ontoggle: (mod: ModCardDto) => void;
  } = $props();

  let preview = $state<string | null>(null);
  let requested = $state("");
  let loaded = $state(false);

  // Дополнение (детали для машин игры, погода, скрипт): картинки у таких модов
  // случайные — текстуры, шум, — поэтому карточка без картинки, с подписью.
  const addon = $derived(mod.role === "addon");
  const tag = $derived(typeTag(mod));
  const copies = $derived(copiesBadge(mod));
  const status = $derived(modStatus(mod));

  $effect(() => {
    const id = mod.id;
    // Ключ включает версию файла: после замены архива старая картинка не должна
    // оставаться на карточке, даже если превью у нового файла нет.
    const key = `${id}|${mod.fileName}|${mod.sizeBytes}|${mod.version ?? ""}|${mod.role ?? ""}`;
    if (addon || mod.deleted) {
      preview = null;
      return;
    }
    if (mod.previewUrl) {
      preview = mod.previewUrl;
      return;
    }
    if (requested === key) return;
    requested = key;
    preview = null;
    loaded = false;
    void modPreview(id)
      .then((url) => {
        if (requested === key) preview = url;
      })
      .catch(() => {});
  });

  const icon = $derived(
    tag === "vehicle"
      ? Car
      : tag === "map"
        ? MapTrifold
        : tag === "script"
          ? FileCode
          : tag === "sound"
            ? SpeakerHigh
            : tag === "addon"
              ? PuzzlePiece
              : Package,
  );

  function select(event: MouseEvent) {
    onselect(mod.id, { ctrl: event.ctrlKey || event.metaKey, shift: event.shiftKey });
  }

  const version = $derived(versionLine(mod));
  const compat = $derived(compatNote(mod));
  const caption = $derived(addonCaption(mod));
</script>

<div
  class="card flow-card"
  class:selected
  class:appear
  class:sleeping={!mod.enabled && !mod.deleted}
  class:deleted={mod.deleted}
  style={`--i:${Math.min(index, 14)}; --flow:${flowName(mod.id)}`}
  data-mod-id={mod.id}
>
  <!-- Выбор — отдельной кнопкой поверх карточки: переключатель не вложен в кнопку. -->
  <button
    class="select"
    type="button"
    aria-pressed={selected}
    aria-label={mod.name}
    title={`${mod.name} — ${mod.fileName}`}
    onclick={select}
  ></button>
  <div class="thumb" class:addon>
    {#if addon}
      {@const Icon = icon}
      <span class="addon-label">
        <Icon size={34} weight="duotone" />
        {#if caption}<span>{caption}</span>{/if}
      </span>
    {:else if preview}
      <img
        src={preview}
        alt=""
        loading="lazy"
        decoding="async"
        class:loaded
        onload={() => (loaded = true)}
      />
    {:else}
      {@const Icon = icon}
      <span class="placeholder"><Icon size={40} weight="thin" /></span>
    {/if}

    <span class="kind-tag {tag}">{typeLabel(tag)}</span>
    {#if status}
      <span class="status {status.tone}">{status.text}</span>
    {/if}
    {#if copies}
      <span
        class="copies"
        class:conflict={mod.groupConflict}
        title={mod.groupConflict
          ? "Включено несколько копий — игра загрузит все"
          : "Остальные копии — в панели мода"}
      >
        <Copy size={12} weight="bold" />{copies}
      </span>
    {/if}
  </div>

  <div class="body">
    <div class="text">
      <p class="name">{mod.name}</p>
      <p class="meta">
        {#if version}
          <span class="mono version" class:fresh={version.fresh}>{version.text}</span>
        {/if}
        <span>{formatSize(mod.sizeBytes)}</span>
        {#if mod.deleted}
          <span>{mod.copies > 1 ? `${mod.copies} ${plural(mod.copies, "копия", "копии", "копий")}` : "копия"}</span>
        {:else if compat}
          <span class="compat" title={compat.title}>{compat.text}</span>
        {/if}
      </p>
    </div>
    {#if !mod.deleted}
      <div class="toggle">
        <Switch
          checked={mod.enabled}
          onCheckedChange={() => ontoggle(mod)}
          aria-label={mod.enabled ? "Выключить мод" : "Включить мод"}
        />
      </div>
    {/if}
  </div>
</div>

<style>
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 206px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    cursor: default;
    transition:
      transform var(--dur-2) var(--ease-out),
      border-color var(--dur-2) var(--ease-out),
      box-shadow var(--dur-2) var(--ease-out);
  }

  .card.appear {
    animation: rise-in var(--dur-3) var(--ease-out) both;
    animation-delay: calc(var(--i) * 18ms);
  }

  /* Без сдвига при наведении: карточка под краем курсора начинала дрожать. */
  .card:hover {
    border-color: var(--border-strong);
    box-shadow: var(--shadow-1);
  }

  .card.selected {
    border-color: var(--accent-brand);
    box-shadow: 0 0 0 1px var(--accent-brand);
  }

  .select {
    position: absolute;
    inset: 0;
    z-index: 1;
    padding: 0;
    border: 0;
    border-radius: inherit;
    background: transparent;
    cursor: default;
  }

  .select:focus-visible {
    outline: 2px solid var(--ring);
    outline-offset: 2px;
  }

  .thumb {
    position: relative;
    flex: none;
    height: 140px;
    overflow: hidden;
    background: var(--surface-2);
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0;
    transition:
      opacity var(--dur-3) var(--ease-out),
      transform var(--dur-3) var(--ease-out),
      filter var(--dur-2) var(--ease-out);
  }

  .thumb img.loaded {
    opacity: 1;
  }

  .card:hover .thumb img {
    transform: scale(1.04);
  }

  .placeholder {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--faint-foreground);
    opacity: 0.6;
  }

  .thumb.addon {
    background: repeating-linear-gradient(
      135deg,
      var(--surface-2) 0 12px,
      color-mix(in oklab, var(--surface-2) 70%, var(--surface-3)) 12px 24px
    );
  }

  .addon-label {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 700;
  }

  .kind-tag {
    position: absolute;
    top: 10px;
    left: 10px;
  }

  .status {
    position: absolute;
    top: 10px;
    right: 10px;
    height: 22px;
    padding: 0 8px;
    border-radius: 5px;
    display: inline-flex;
    align-items: center;
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
    animation: pop-in var(--dur-3) var(--ease-out) both;
  }

  .status.update {
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .status.possible {
    border: 1px solid var(--warning-border);
    background: color-mix(in oklab, var(--background) 82%, transparent);
    color: var(--warning);
  }

  .status.problem {
    border: 1px solid color-mix(in oklab, var(--danger) 45%, transparent);
    background: color-mix(in oklab, var(--background) 82%, transparent);
    color: var(--danger);
  }

  .status.off,
  .status.deleted,
  .status.pending {
    border: 1px solid var(--border-strong);
    background: color-mix(in oklab, var(--background) 82%, transparent);
    color: var(--muted-foreground);
  }

  .copies {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    background: var(--background);
    color: var(--foreground);
    font-size: 11px;
    font-weight: 700;
  }

  .copies.conflict {
    border-color: color-mix(in oklab, var(--danger) 60%, transparent);
    color: var(--danger);
  }

  .body {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 0;
    padding: 0 12px;
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  .name {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    font-size: 14.5px;
    font-weight: 700;
    line-height: 1.25;
    overflow-wrap: anywhere;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    transition: color var(--dur-2) var(--ease-out);
  }

  /* По базовой линии: моноширинная версия мельче и без этого «висела» выше «3 МБ». */
  .meta {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 2px 0 0;
    overflow: hidden;
    color: var(--muted-foreground);
    font-size: 12px;
    white-space: nowrap;
  }

  .version {
    color: var(--foreground);
  }

  .version.fresh {
    color: var(--accent-brand-text);
  }

  .compat {
    color: var(--warning);
  }

  .toggle {
    position: relative;
    z-index: 2;
    flex: none;
  }

  .sleeping .thumb > img,
  .sleeping .thumb .placeholder {
    filter: grayscale(0.9) brightness(0.65);
  }

  .sleeping .name {
    color: var(--muted-foreground);
  }

  .deleted {
    opacity: 0.6;
  }
</style>

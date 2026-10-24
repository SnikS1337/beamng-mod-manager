<script lang="ts">
  import { addonHint, copiesBadge } from "./duplicates";
  import { Car, FileCode, Map, Music, Package } from "@lucide/svelte";
  import { Switch } from "#lib/components/ui/switch/index.js";
  import { modPreview, type ModCardDto } from "#lib/ipc/library.js";
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

  // Дополнение (детали для машин игры, погода, скрипт): картинки у таких модов
  // случайные — текстуры, шум, — поэтому карточка без картинки, с подписью.
  const addon = $derived(mod.role === "addon");
  const copies = $derived(copiesBadge(mod));

  $effect(() => {
    const id = mod.id;
    if (addon) {
      preview = null;
      return;
    }
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
      : mod.problem
        ? { text: "Игра не загрузит", class: "problem" }
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
  <div class="thumb" class:addon style={preview || addon ? undefined : `background:${placeholder}`}>
    {#if addon}
      <span class="addon-label">
        <b>{addonHint(mod)}</b>
        <span>не машина и не карта</span>
      </span>
    {:else if preview}
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
    {#if copies}
      <span
        class="copies"
        class:conflict={mod.groupConflict}
        title={mod.groupConflict ? "Включено несколько копий — игра загрузит все" : "Остальные копии — в панели мода"}
      >{copies}</span>
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
    height: 236px;
    border: 1px solid rgb(255 255 255 / 0.16);
    border-radius: 26px;
    background: var(--glass-fill);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.28),
      0 16px 32px rgb(8 12 24 / 0.22);
    cursor: default;
    overflow: hidden;
    transition:
      transform 220ms cubic-bezier(0.22, 1, 0.36, 1),
      border-color 160ms ease,
      box-shadow 220ms ease;
  }

  /* Наведение (legacy): приподнимается на 5 px, голубое свечение и светлая окантовка. */
  .card:hover {
    transform: translateY(-5px);
    border-color: rgb(255 255 255 / 0.55);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.4),
      0 22px 40px rgb(8 12 24 / 0.28),
      0 0 26px rgb(170 205 255 / 0.28);
  }

  .card:focus-visible {
    outline: 3px solid var(--ring);
    outline-offset: 2px;
  }

  /* Выбранная — без обводки: приподнята, кромка ярче, холодное свечение снизу. */
  .card.selected {
    transform: translateY(-4px);
    border-color: rgb(255 255 255 / 0.5);
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.45),
      0 22px 44px rgb(8 12 24 / 0.3),
      0 14px 34px rgb(150 190 255 / 0.32);
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
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .thumb.addon {
    background:
      repeating-linear-gradient(135deg, rgb(255 255 255 / 0.035) 0 10px, transparent 10px 20px),
      linear-gradient(160deg, rgb(52 60 82), rgb(28 33 48));
  }

  .addon-label {
    position: absolute;
    inset: 0 0 64px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    text-align: center;
    color: rgb(255 255 255 / 0.82);
  }

  .addon-label b {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  .addon-label span {
    font-size: 11px;
    color: rgb(255 255 255 / 0.5);
  }

  .copies {
    position: absolute;
    top: 12px;
    left: 12px;
    display: inline-flex;
    align-items: center;
    height: 24px;
    padding: 0 10px;
    border: 1px solid rgb(255 255 255 / 0.16);
    border-radius: 999px;
    background: rgb(10 14 26 / 0.78);
    color: rgb(255 255 255 / 0.94);
    font-size: 12px;
    font-weight: 700;
  }

  .copies.conflict {
    border-color: rgb(255 107 94 / 0.7);
    color: #ffb3ab;
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

  /* Плашки legacy: тёмное стекло, белый текст и цветная точка — статус читается словом. */
  .badge {
    position: absolute;
    top: 12px;
    right: 12px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border: 1px solid rgb(255 255 255 / 0.16);
    border-radius: 999px;
    background: rgb(10 14 26 / 0.78);
    color: rgb(255 255 255 / 0.94);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.005em;
  }

  .badge::before {
    content: "";
    width: 7px;
    height: 7px;
    border-radius: 999px;
    background: currentColor;
  }

  .badge.update::before {
    background: #6fb3ff;
    box-shadow: 0 0 8px #6fb3ff;
  }

  .badge.problem::before {
    background: #ff6b5e;
    box-shadow: 0 0 8px #ff6b5e;
  }

  .badge.possible::before {
    background: #ffb340;
    box-shadow: 0 0 8px #ffb340;
  }

  .badge.off::before,
  .badge.deleted::before {
    background: rgb(255 255 255 / 0.7);
  }

  /* Выключенный переключатель на тёмной подложке фото должен быть виден в обоих оформлениях. */
  .toggle :global([data-slot="switch"][data-state="unchecked"]) {
    background: rgb(255 255 255 / 0.24);
    box-shadow: inset 0 0 0 1px rgb(255 255 255 / 0.3);
  }

  .badge.pending-badge {
    right: auto;
    left: 12px;
  }

  .caption {
    position: absolute;
    inset: auto 0 0;
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 8px;
    padding: 56px 14px 14px;
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
    font-size: 15px;
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
    color: rgb(255 255 255 / 0.72);
    font-size: 12.5px;
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

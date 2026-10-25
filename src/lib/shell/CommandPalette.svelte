<script lang="ts">
  import { isShortcut } from "./keys";
  import { tick } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import MagnifyingGlass from "phosphor-svelte/lib/MagnifyingGlass";
  import Car from "phosphor-svelte/lib/Car";
  import ArrowsClockwise from "phosphor-svelte/lib/ArrowsClockwise";
  import Plus from "phosphor-svelte/lib/Plus";
  import Funnel from "phosphor-svelte/lib/Funnel";
  import ArrowRight from "phosphor-svelte/lib/ArrowRight";
  import { libraryLoad } from "#lib/ipc/library.js";
  import { palette } from "./palette.svelte";
  import { paletteItems, type PaletteItem } from "./paletteItems";

  let query = $state("");
  let active = $state(0);
  let input = $state<HTMLInputElement | null>(null);
  let list = $state<HTMLDivElement | null>(null);
  let loadingMods = false;

  const items = $derived(paletteItems(query, palette.mods));
  const groups = $derived(
    (["Ваши моды", "Команды", "Разделы"] as const)
      .map((name) => ({ name, items: items.filter((item) => item.group === name) }))
      .filter((group) => group.items.length > 0),
  );

  $effect(() => {
    if (!palette.open) return;
    query = "";
    active = 0;
    void tick().then(() => input?.focus());
    // Палитру открыли не из гаража, а моды ещё не загружались — подгружаем для поиска.
    if (palette.mods.length === 0 && !loadingMods) {
      loadingMods = true;
      void libraryLoad()
        .then((snapshot) => {
          if (palette.mods.length === 0) palette.mods = snapshot.mods;
        })
        .catch(() => {})
        .finally(() => (loadingMods = false));
    }
  });

  $effect(() => {
    query;
    active = 0;
  });

  function onWindowKey(event: KeyboardEvent) {
    if (isShortcut(event, "k")) {
      event.preventDefault();
      palette.toggle();
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      palette.hide();
    } else if (event.key === "ArrowDown") {
      event.preventDefault();
      move(1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      move(-1);
    } else if (event.key === "Enter") {
      event.preventDefault();
      const item = items[active];
      if (item) void run(item);
    }
  }

  function move(step: number) {
    if (items.length === 0) return;
    active = (active + step + items.length) % items.length;
    void tick().then(() =>
      list?.querySelector(`[data-index="${active}"]`)?.scrollIntoView({ block: "nearest" }),
    );
  }

  async function toGarage() {
    if (page.url.pathname !== "/") await goto("/");
  }

  async function run(item: PaletteItem) {
    palette.hide();
    const action = item.action;
    switch (action.kind) {
      case "mod":
        await toGarage();
        palette.garageRequest = { kind: "select", id: action.id };
        break;
      case "filter":
        await toGarage();
        palette.garageRequest = { kind: "filter", filter: action.filter };
        break;
      case "archive":
        await toGarage();
        palette.garageRequest = { kind: "archive" };
        break;
      case "search":
        await goto(`/search?q=${encodeURIComponent(action.query)}`);
        break;
      case "check":
        await goto("/updates?check=1");
        break;
      case "go":
        await goto(action.href);
        break;
    }
  }

  function iconOf(item: PaletteItem) {
    switch (item.action.kind) {
      case "mod":
        return Car;
      case "search":
        return MagnifyingGlass;
      case "check":
        return ArrowsClockwise;
      case "archive":
        return Plus;
      case "filter":
        return Funnel;
      default:
        return ArrowRight;
    }
  }
</script>

<svelte:window onkeydown={onWindowKey} />

{#if palette.open}
  <div class="scrim" role="presentation" onclick={() => palette.hide()}></div>
  <div class="palette" role="dialog" aria-modal="true" aria-label="Палитра команд">
    <label class="field">
      <MagnifyingGlass size={20} />
      <input
        bind:this={input}
        bind:value={query}
        type="text"
        placeholder="Мод, команда или раздел"
        aria-label="Мод, команда или раздел"
        aria-controls="palette-list"
        aria-activedescendant={items[active]?.id}
        {onkeydown}
      />
      <kbd>Esc</kbd>
    </label>
    <div class="list scroll-area" id="palette-list" role="listbox" bind:this={list}>
      {#if items.length === 0}
        <p class="empty">Ничего не нашлось</p>
      {/if}
      {#each groups as group (group.name)}
        <div class="group">{group.name}</div>
        {#each group.items as item (item.id)}
          {@const index = items.indexOf(item)}
          {@const Icon = iconOf(item)}
          <button
            id={item.id}
            class="item"
            class:active={index === active}
            type="button"
            role="option"
            aria-selected={index === active}
            data-index={index}
            onmousemove={() => (active = index)}
            onclick={() => void run(item)}
          >
            <span class="icon"><Icon size={16} /></span>
            <span class="label">{item.label}</span>
            <span class="hint">
              {#if item.tag}<span class="kind-tag {item.tag.tone}">{item.tag.text}</span>{/if}
              {item.hint}
            </span>
          </button>
        {/each}
      {/each}
    </div>
    <div class="foot">
      <span><kbd>↑</kbd> <kbd>↓</kbd> выбрать</span>
      <span><kbd>Enter</kbd> выполнить</span>
      <span><kbd>Ctrl K</kbd> открыть и закрыть</span>
    </div>
  </div>
{/if}

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 90;
    background: var(--overlay);
    animation: fade-in var(--dur-2) var(--ease-out) both;
  }

  .palette {
    position: fixed;
    top: 96px;
    left: 50%;
    z-index: 91;
    display: flex;
    flex-direction: column;
    width: min(640px, calc(100vw - 32px));
    max-height: min(560px, calc(100vh - 140px));
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: 14px;
    background: var(--surface);
    box-shadow: var(--shadow-2);
    transform: translateX(-50%);
    animation: palette-in var(--dur-3) var(--ease-out) both;
  }

  @keyframes palette-in {
    from {
      opacity: 0;
      transform: translate(-50%, -8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translate(-50%, 0);
    }
  }

  .field {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 56px;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
    color: var(--muted-foreground);
  }

  .field input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--foreground);
    font-size: 17px;
    font-weight: 600;
    outline: none;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 8px 8px;
  }

  .group {
    padding: 10px 10px 6px;
    color: var(--faint-foreground);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    height: 44px;
    padding: 0 10px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--foreground);
    font-size: 14.5px;
    font-weight: 600;
    text-align: left;
    cursor: default;
  }

  .item.active {
    background: var(--surface-3);
    box-shadow: inset 3px 0 0 var(--accent-brand);
  }

  .icon {
    display: grid;
    flex: none;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--muted-foreground);
  }

  .item.active .icon {
    color: var(--accent-brand-text);
  }

  .label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .hint {
    display: flex;
    flex: none;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    color: var(--faint-foreground);
    font-size: 12px;
    font-weight: 500;
  }

  .empty {
    margin: 0;
    padding: 24px;
    color: var(--muted-foreground);
    text-align: center;
  }

  .foot {
    display: flex;
    gap: 16px;
    padding: 9px 16px;
    border-top: 1px solid var(--border);
    color: var(--faint-foreground);
    font-size: 12px;
  }
</style>

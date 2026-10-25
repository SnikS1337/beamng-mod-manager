<script lang="ts">
  import type { Snippet } from "svelte";
  import { ContextMenu } from "bits-ui";
  import Power from "phosphor-svelte/lib/Power";
  import ArrowSquareOut from "phosphor-svelte/lib/ArrowSquareOut";
  import FolderOpen from "phosphor-svelte/lib/FolderOpen";
  import ClockCounterClockwise from "phosphor-svelte/lib/ClockCounterClockwise";
  import Trash from "phosphor-svelte/lib/Trash";
  import type { ModCardDto } from "#lib/ipc/library.js";

  let {
    mods,
    children,
    onToggle,
    onOpenPage,
    onShowFolder,
    onShowDetails,
    onDelete,
  }: {
    /** Все карточки, по которым можно щёлкнуть правой кнопкой. */
    mods: ModCardDto[];
    children: Snippet;
    onToggle: (mod: ModCardDto) => void;
    onOpenPage: (mod: ModCardDto) => void;
    onShowFolder: (mod: ModCardDto) => void;
    onShowDetails: (mod: ModCardDto) => void;
    onDelete: (mod: ModCardDto) => void;
  } = $props();

  // Одно меню на весь список: мод — тот, по чьей карточке или строке щёлкнули.
  let target = $state<ModCardDto | null>(null);

  function pick(event: MouseEvent) {
    const element = (event.target as HTMLElement | null)?.closest<HTMLElement>("[data-mod-id]");
    const id = element?.dataset.modId;
    const found = id ? mods.find((mod) => mod.id === id) : undefined;
    if (!found || found.deleted) {
      // Щелчок мимо мода — меню не открываем.
      event.stopImmediatePropagation();
      event.preventDefault();
      target = null;
      return;
    }
    target = found;
  }
</script>

<ContextMenu.Root>
  <ContextMenu.Trigger>
    {#snippet child({ props })}
      <div {...props} class="menu-area" oncontextmenucapture={pick}>
        {@render children()}
      </div>
    {/snippet}
  </ContextMenu.Trigger>
  <ContextMenu.Portal>
    <ContextMenu.Content class="mod-menu">
      {#if target}
        {@const mod = target}
        <div class="menu-title">{mod.name}</div>
        <ContextMenu.Item class="menu-item" onSelect={() => onToggle(mod)}>
          <Power size={16} />{mod.enabled ? "Выключить" : "Включить"}<kbd>Space</kbd>
        </ContextMenu.Item>
        <ContextMenu.Item class="menu-item" onSelect={() => onOpenPage(mod)}>
          <ArrowSquareOut size={16} />Открыть страницу мода
        </ContextMenu.Item>
        <ContextMenu.Item class="menu-item" onSelect={() => onShowFolder(mod)}>
          <FolderOpen size={16} />Показать в папке
        </ContextMenu.Item>
        <ContextMenu.Item class="menu-item" onSelect={() => onShowDetails(mod)}>
          <ClockCounterClockwise size={16} />Подробно и прошлые версии
        </ContextMenu.Item>
        {#if !mod.fromGameRepo}
          <ContextMenu.Separator class="menu-sep" />
          <ContextMenu.Item class="menu-item danger" onSelect={() => onDelete(mod)}>
            <Trash size={16} />Удалить<kbd>Del</kbd>
          </ContextMenu.Item>
        {/if}
      {/if}
    </ContextMenu.Content>
  </ContextMenu.Portal>
</ContextMenu.Root>

<style>
  .menu-area {
    height: 100%;
    min-height: 0;
  }

  :global(.mod-menu) {
    z-index: 80;
    min-width: 248px;
    max-width: 320px;
    padding: 6px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface-2);
    box-shadow: var(--shadow-2);
    outline: none;
    animation: pop-in var(--dur-2) var(--ease-out) both;
  }

  .menu-title {
    padding: 6px 10px 8px;
    overflow: hidden;
    color: var(--faint-foreground);
    font-size: 11.5px;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  :global(.mod-menu .menu-item) {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 34px;
    padding: 0 10px;
    border-radius: 6px;
    color: var(--foreground);
    font-size: 13.5px;
    font-weight: 600;
    outline: none;
    cursor: default;
  }

  :global(.mod-menu .menu-item kbd) {
    margin-left: auto;
  }

  :global(.mod-menu .menu-item[data-highlighted]) {
    background: var(--surface-3);
  }

  :global(.mod-menu .menu-item.danger) {
    color: var(--danger);
  }

  :global(.mod-menu .menu-sep) {
    height: 1px;
    margin: 5px 4px;
    background: var(--border);
  }
</style>

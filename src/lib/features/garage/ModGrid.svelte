<script lang="ts">
  import type { ModCardDto } from "#lib/ipc/library.js";
  import ModCard from "./ModCard.svelte";

  let {
    items,
    selectedIds,
    onselect,
    ontoggle,
    resetKey = "",
    renderedCount = $bindable(0),
  }: {
    items: ModCardDto[];
    /** Выбранные: одна карточка или несколько. */
    selectedIds: string[];
    onselect: (id: string, keys: { ctrl: boolean; shift: boolean }) => void;
    ontoggle: (mod: ModCardDto) => void;
    resetKey?: string;
    renderedCount?: number;
  } = $props();

  // Рисуются только видимые ряды (+ по ряду сверху и снизу): сотни модов не тормозят.
  const CARD_HEIGHT = 206;
  const GAP = 16;
  const ROW_HEIGHT = CARD_HEIGHT + GAP;
  const MIN_CARD = 228;
  const PADDING = 24;

  let viewport = $state<HTMLDivElement | null>(null);
  let width = $state(0);
  let height = $state(0);
  let scrollTop = $state(0);
  /** Карточки появляются с анимацией только при первом показе и смене фильтра, не при прокрутке. */
  let appear = $state(true);

  let inner = $derived(Math.max(0, width - PADDING * 2));
  let columns = $derived(Math.min(8, Math.max(1, Math.floor((inner + GAP) / (MIN_CARD + GAP)))));
  let rows = $derived(Math.ceil(items.length / columns));
  let startRow = $derived(Math.max(0, Math.floor((scrollTop - PADDING) / ROW_HEIGHT) - 1));
  let endRow = $derived(
    Math.min(rows, Math.ceil((scrollTop + Math.max(height, 1)) / ROW_HEIGHT) + 1),
  );
  let visible = $derived(items.slice(startRow * columns, endRow * columns));
  let topPad = $derived(startRow * ROW_HEIGHT);
  let bottomPad = $derived(Math.max(0, (rows - endRow) * ROW_HEIGHT - GAP));

  $effect(() => {
    renderedCount = visible.length;
  });

  $effect(() => {
    resetKey;
    if (viewport) {
      viewport.scrollTop = 0;
      scrollTop = 0;
    }
    appear = true;
    const timer = setTimeout(() => (appear = false), 700);
    return () => clearTimeout(timer);
  });

  function onscroll(event: Event) {
    scrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }
</script>

<div
  class="scroll-area viewport"
  bind:this={viewport}
  bind:clientWidth={width}
  bind:clientHeight={height}
  {onscroll}
>
  <div style={`height:${topPad}px`}></div>

  <div class="grid" style={`grid-template-columns:repeat(${columns}, minmax(0, 1fr)); gap:${GAP}px`}>
    {#each visible as mod, index (mod.id)}
      <ModCard {mod} {index} {appear} selected={selectedIds.includes(mod.id)} {onselect} {ontoggle} />
    {/each}
  </div>

  <div style={`height:${bottomPad}px`}></div>
</div>

<style>
  .viewport {
    height: 100%;
    box-sizing: border-box;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 20px 24px 24px;
  }

  .grid {
    display: grid;
  }
</style>

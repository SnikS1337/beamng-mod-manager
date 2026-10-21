<script lang="ts">
  import ModCard from "./ModCard.svelte";
  import type { ModEntry } from "./api";

  let {
    items,
    selectedId,
    onselect,
    ontoggle,
    resetKey = 0,
    renderedCount = $bindable(0),
  }: {
    items: ModEntry[];
    selectedId: number | null;
    onselect: (id: number) => void;
    ontoggle: (id: number) => void;
    resetKey?: string | number;
    renderedCount?: number;
  } = $props();

  const ROW_HEIGHT = 250;
  const GAP = 14;

  let viewport = $state<HTMLDivElement | null>(null);
  let width = $state(0);
  let height = $state(0);
  let scrollTop = $state(0);

  let columns = $derived(
    width < 640 ? 2 : width < 900 ? 3 : width < 1180 ? 4 : width < 1480 ? 5 : 6,
  );
  let rows = $derived(Math.ceil(items.length / columns));
  let startRow = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - 1));
  let endRow = $derived(
    Math.min(rows, Math.ceil((scrollTop + Math.max(height, 1)) / ROW_HEIGHT) + 1),
  );
  let visible = $derived(items.slice(startRow * columns, endRow * columns));
  let topPad = $derived(startRow * ROW_HEIGHT);
  let bottomPad = $derived(Math.max(0, (rows - endRow) * ROW_HEIGHT));

  $effect(() => {
    renderedCount = visible.length;
  });

  $effect(() => {
    resetKey;
    if (viewport) {
      viewport.scrollTop = 0;
      scrollTop = 0;
    }
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

  {#if items.length === 0}
    <div class="empty">
      <p>Ничего не найдено</p>
      <span>Измените запрос или фильтр</span>
    </div>
  {:else}
    <div
      class="grid"
      style={`grid-template-columns:repeat(${columns}, minmax(0, 1fr)); gap:${GAP}px`}
    >
      {#each visible as mod (mod.id)}
        <ModCard
          {mod}
          selected={mod.id === selectedId}
          {onselect}
          {ontoggle}
        />
      {/each}
    </div>
  {/if}

  <div style={`height:${bottomPad}px`}></div>
</div>

<style>
  .viewport {
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
    scroll-behavior: smooth;
    padding-right: 6px;
  }

  .grid {
    display: grid;
  }

  .empty {
    display: grid;
    place-content: center;
    gap: 4px;
    min-height: 320px;
    color: var(--muted-foreground);
    text-align: center;
  }

  .empty p {
    margin: 0;
    color: var(--foreground);
    font-size: 15px;
    font-weight: 600;
  }

  .empty span {
    font-size: 11px;
  }
</style>

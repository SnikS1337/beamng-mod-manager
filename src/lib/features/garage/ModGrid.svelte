<script lang="ts">
  import type { ModCardDto } from "$lib/ipc/library";
  import ModCard from "./ModCard.svelte";

  let {
    items,
    selectedId,
    onselect,
    ontoggle,
    resetKey = "",
    renderedCount = $bindable(0),
  }: {
    items: ModCardDto[];
    selectedId: string | null;
    onselect: (id: string) => void;
    ontoggle: (mod: ModCardDto) => void;
    resetKey?: string;
    renderedCount?: number;
  } = $props();

  const ROW_HEIGHT = 218;
  const GAP = 14;

  let viewport = $state<HTMLDivElement | null>(null);
  let width = $state(0);
  let height = $state(0);
  let scrollTop = $state(0);

  let columns = $derived(
    width < 620 ? 2 : width < 930 ? 3 : width < 1660 ? 4 : width < 2000 ? 5 : 6,
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

  <div
    class="grid"
    style={`grid-template-columns:repeat(${columns}, minmax(0, 1fr)); gap:${GAP}px`}
  >
    {#each visible as mod (mod.id)}
      <ModCard {mod} selected={mod.id === selectedId} {onselect} {ontoggle} />
    {/each}
  </div>

  <div style={`height:${bottomPad}px`}></div>
</div>

<style>
  .viewport {
    height: 100%;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding-right: 6px;
  }

  .grid {
    display: grid;
  }
</style>

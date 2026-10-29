<script lang="ts">
  import type { ModCardDto } from "#lib/ipc/library.js";
  import { tick, untrack } from "svelte";
  import { motion } from "#lib/shell/motion.svelte.js";
  import ModCard from "./ModCard.svelte";

  let {
    items,
    selectedIds,
    onselect,
    ontoggle,
    resetKey = "",
    appearKey = resetKey,
    revealId = null,
  }: {
    items: ModCardDto[];
    /** Выбранные: одна карточка или несколько. */
    selectedIds: string[];
    onselect: (id: string, keys: { ctrl: boolean; shift: boolean }) => void;
    ontoggle: (mod: ModCardDto) => void;
    /** Когда сбросить прокрутку (фильтр, тип, поиск). */
    resetKey?: string;
    /** Когда карточки появляются заново. Поиск сюда не входит: иначе каждая буква перезапускает анимацию. */
    appearKey?: string;
    /** Карточка, к которой нужно прокрутить и передать фокус после стрелок. */
    revealId?: string | null;
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
    resetKey;
    if (viewport) {
      viewport.scrollTop = 0;
      scrollTop = 0;
    }
  });

  $effect(() => {
    appearKey;
    // В плавном переходе карточки уже двигаются — своё появление лишнее.
    appear = !untrack(() => motion.active);
    const timer = setTimeout(() => (appear = false), 700);
    return () => clearTimeout(timer);
  });

  $effect(() => {
    const el = viewport;
    if (!el) return;
    return motion.measure(() => {
      width = el.clientWidth;
      height = el.clientHeight;
    });
  });

  $effect(() => {
    const id = revealId;
    if (!id || !viewport) return;
    const el = viewport;
    const index = items.findIndex((mod) => mod.id === id);
    if (index < 0) return;
    // Карточка вне окна виртуализации: сначала прокручиваем к её ряду,
    // иначе фокусировать нечего.
    if (index < startRow * columns || index >= endRow * columns) {
      const target = Math.max(0, Math.floor(index / columns) * ROW_HEIGHT);
      el.scrollTop = target;
      scrollTop = target;
    }
    void tick().then(() => {
      const card = [...el.querySelectorAll<HTMLElement>("[data-mod-id]")].find(
        (element) => element.dataset.modId === id,
      );
      const control = card?.querySelector<HTMLElement>(
        ".toggle [role='switch'], .toggle button, .toggle input",
      );
      control?.focus({ preventScroll: true });
    });
  });

  function onscroll(event: Event) {
    scrollTop = (event.currentTarget as HTMLDivElement).scrollTop;
  }
</script>

<div
  class="scroll-area viewport flow-mods"
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

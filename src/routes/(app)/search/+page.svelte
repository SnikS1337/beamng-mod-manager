<script lang="ts">
  import { isShortcut } from "#lib/shell/keys.js";
  import { onDestroy, onMount } from "svelte";
  import { page } from "$app/state";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import ArrowSquareOut from "phosphor-svelte/lib/ArrowSquareOut";
  import MagnifyingGlass from "phosphor-svelte/lib/MagnifyingGlass";
  import X from "phosphor-svelte/lib/X";
  import Car from "phosphor-svelte/lib/Car";
  import { formatSize, plural } from "#lib/features/garage/format.js";
  import { searchKindLabel } from "#lib/features/search/kind.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import {
    listenSearch,
    searchCancel,
    searchDecide,
    searchOpenModland,
    searchPagePreview,
    searchPreview,
    searchStart,
    sourceName,
    type SearchCard,
    type SearchEvent,
    type SearchListing,
    type SearchSource,
    type SearchPriceSummary,
  } from "#lib/ipc/search.js";

  let query = $state("");
  let submitted = $state("");
  let searching = $state(false);
  let passing = $state(false);
  let searched = $state(false);
  let cards = $state<SearchCard[]>([]);
  let sources = $state<SearchSource[]>([]);
  let error = $state<string | null>(null);
  let deciding = $state<string | null>(null);
  let previews = $state<Record<string, string>>({});
  const requestedPreviews = new Set<string>();
  const requestedPages = new Set<string>();
  let pageRequests = 0;
  const PAGE_PREVIEW_LIMIT = 24;
  let requestId = 0;
  let input = $state<HTMLInputElement | null>(null);
  let unlisten: (() => void) | undefined;

  const totalListings = $derived(
    cards.reduce((sum, card) => sum + card.listings.length, 0),
  );

  $effect(() => {
    for (const card of cards) {
      if (previews[card.id] || requestedPreviews.has(card.id)) continue;
      const url = card.listings.find((listing) => listing.previewUrl)?.previewUrl;
      if (url) {
        requestedPreviews.add(card.id);
        void searchPreview(url).then((value) => {
          if (value) previews[card.id] = value;
        });
        continue;
      }
      // В выдаче ModsGaming картинок нет — берём её со страницы мода, как legacy.
      if (pageRequests >= PAGE_PREVIEW_LIMIT || requestedPages.has(card.id)) continue;
      const listing = card.listings.find((item) => item.source === "modsgaming");
      if (!listing) continue;
      pageRequests += 1;
      requestedPages.add(card.id);
      void searchPagePreview(listing.source, listing.url)
        .then((value) => {
          if (value) previews[card.id] = value;
        })
        .catch(() => {});
    }
  });

  let listening = $state(false);

  onMount(() => {
    input?.focus();
    void listenSearch(onEvent).then((stop) => {
      unlisten = stop;
      listening = true;
    });
    return () => unlisten?.();
  });

  // «Найти … на сайтах» из палитры команд: /search?q=…
  let askedQuery = "";
  $effect(() => {
    const asked = page.url.searchParams.get("q")?.trim() ?? "";
    if (!listening || !asked || asked === askedQuery) return;
    askedQuery = asked;
    query = asked;
    void runSearch();
  });

  onDestroy(() => {
    unlisten?.();
    if (searching) void searchCancel(requestId);
  });

  function onEvent(event: SearchEvent) {
    if (event.requestId !== requestId) return;
    if (event.phase === "partial") {
      cards = event.cards;
      sources = event.sources;
      return;
    }
    if (event.phase === "done") {
      cards = event.cards;
      sources = event.sources;
      searching = false;
      return;
    }
    if (event.phase === "error") {
      error = event.error ?? "Поиск не удался.";
      searching = false;
    }
  }

  async function runSearch(event?: Event) {
    event?.preventDefault();
    const text = query.trim();
    if (text.length < 2) {
      error = "Введите хотя бы 2 символа.";
      return;
    }
    if (searching) await searchCancel(requestId).catch(() => {});
    requestId += 1;
    const current = requestId;
    submitted = text;
    searching = true;
    searched = true;
    error = null;
    cards = [];
    sources = [];
    previews = {};
    requestedPreviews.clear();
    requestedPages.clear();
    pageRequests = 0;
    try {
      await searchStart(text, current);
    } catch (reason) {
      if (current === requestId) {
        error = errorText(reason);
        searching = false;
      }
    }
  }

  async function cancel() {
    const current = requestId;
    requestId += 1;
    searching = false;
    await searchCancel(current).catch(() => {});
  }

  function clear() {
    query = "";
    submitted = "";
    searched = false;
    cards = [];
    sources = [];
    previews = {};
    requestedPreviews.clear();
    requestedPages.clear();
    pageRequests = 0;
    error = null;
    input?.focus();
  }

  async function openLink(url: string) {
    try {
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener");
    }
  }

  async function openModland() {
    if (passing) return;
    passing = true;
    try {
      const passed = await searchOpenModland(submitted || query);
      if (passed) {
        toasts.show("Modland пускает программу — ищем снова");
        if (submitted) {
          query = submitted;
          await runSearch();
        }
      } else {
        toasts.show("Проверка не завершена. Пройдите её в окне и попробуйте снова");
      }
    } catch (reason) {
      error = errorText(reason);
    } finally {
      passing = false;
    }
  }

  function cardById(id: string): SearchCard | undefined {
    return cards.find((card) => card.id === id);
  }

  async function decide(card: SearchCard, other: SearchCard, same: boolean) {
    const a = card.listings[0];
    const b = other.listings[0];
    if (!a || !b) return;
    deciding = card.id;
    error = null;
    try {
      const event = await searchDecide(
        submitted,
        { source: a.source, sourceId: a.sourceId },
        { source: b.source, sourceId: b.sourceId },
        same,
      );
      cards = event.cards;
      sources = event.sources;
    } catch (reason) {
      error = errorText(reason);
    } finally {
      deciding = null;
    }
  }

  function priceLabel(price: SearchPriceSummary): string {
    switch (price) {
      case "free":
        return "Бесплатно";
      case "paid":
        return "Платно";
      case "mixed":
        return "Есть платные";
      default:
        return "Цена неизвестна";
    }
  }

  function versionText(listing: SearchListing): string {
    const parts: string[] = [];
    if (listing.modVersion) parts.push(`v${listing.modVersion}`);
    if (listing.gameVersion) parts.push(`игра ${listing.gameVersion}`);
    return parts.join(" · ");
  }

  function metaText(card: SearchCard, listing: SearchListing): string {
    // Раздел сайта («mashiny», «Cars») уже показан меткой типа на фото.
    const category = card.kind ? null : listing.category;
    return [listing.author, category, formatSize(listing.sizeBytes ?? 0)]
      .filter((value) => value && value !== "—")
      .join(" · ");
  }

  function hasPaid(listing: SearchListing): boolean {
    return listing.offers.some((offer) => offer.kind === "paidLink");
  }

  function downloadOffer(listing: SearchListing) {
    return listing.offers.find((offer) => offer.kind !== "paidLink");
  }

  function paidOffer(listing: SearchListing) {
    return listing.offers.find((offer) => offer.kind === "paidLink");
  }

  function previewOf(card: SearchCard): string | null {
    return (
      previews[card.id] ??
      card.listings.find((listing) => listing.previewUrl?.startsWith("data:"))
        ?.previewUrl ??
      null
    );
  }

  type KindFilter = "all" | "vehicle" | "map" | "addon";
  let kindFilter = $state<KindFilter>("all");
  const kindFilters: { id: Exclude<KindFilter, "all">; label: string }[] = [
    { id: "vehicle", label: "Машины" },
    { id: "map", label: "Карты" },
    { id: "addon", label: "Дополнения" },
  ];
  const kindCount = (id: KindFilter) =>
    id === "all" ? cards.length : cards.filter((card) => card.kind === id).length;
  const shownCards = $derived(
    kindFilter === "all" ? cards : cards.filter((card) => card.kind === kindFilter),
  );

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось выполнить поиск.";
  }

  function onkeydown(event: KeyboardEvent) {
    if (isShortcut(event, "f")) {
      event.preventDefault();
      input?.focus();
      input?.select();
      return;
    }
    if (event.key === "Escape") {
      if (query) clear();
    }
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="search-page">
  <header class="top" data-tauri-drag-region>
    <div class="title" data-tauri-drag-region>
      <h1 class="display" data-tauri-drag-region>Поиск</h1>
      <p data-tauri-drag-region>
        {#if searching}
          ищем на сайтах…
        {:else if searched}
          {cards.length}
          {plural(cards.length, "мод", "мода", "модов")}
        {:else}
          сразу на всех сайтах
        {/if}
      </p>
    </div>

    <form class="bar" onsubmit={runSearch}>
      <MagnifyingGlass size={18} />
      <input
        bind:this={input}
        bind:value={query}
        type="search"
        placeholder="Название машины, карты или автора"
        aria-label="Поиск модов на сайтах"
        autocomplete="off"
      />
      {#if query}
        <button class="clear" type="button" aria-label="Очистить" onclick={clear}>
          <X size={14} weight="bold" />
        </button>
      {/if}
      {#if searching}
        <button class="btn-glass go" type="button" onclick={() => void cancel()}>Остановить</button>
      {:else}
        <button class="btn-primary go" type="submit" disabled={query.trim().length < 2}>Найти</button>
      {/if}
    </form>
  </header>

  {#if cards.length > 0}
    <div class="filters" role="group" aria-label="Тип">
      <button class="chip" class:on={kindFilter === "all"} type="button" onclick={() => (kindFilter = "all")}>
        Все <b>{kindCount("all")}</b>
      </button>
      {#each kindFilters as item (item.id)}
        <button
          class="chip"
          class:on={kindFilter === item.id}
          type="button"
          aria-pressed={kindFilter === item.id}
          onclick={() => (kindFilter = kindFilter === item.id ? "all" : item.id)}
        >{item.label} <b>{kindCount(item.id)}</b></button>
      {/each}
    </div>
  {/if}

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  <div class="body">
  <section class="results scroll-area">
  {#if searching && cards.length === 0}
    <div class="grid skeletons">
      {#each Array(6) as _}
        <div class="ghost">
          <div class="skeleton ghost-cover"></div>
          <div class="skeleton ghost-line wide"></div>
          <div class="skeleton ghost-line"></div>
        </div>
      {/each}
    </div>
  {:else if searched && cards.length === 0 && !searching && !error}
    <div class="state">
      <p>Ничего не нашлось</p>
      <span>Попробуйте короче: название машины, карты или автора. Сайты ищут по-разному.</span>
    </div>
  {:else if cards.length > 0}
    <div class="grid">
      {#each shownCards as card (card.id)}
        {@const preview = previewOf(card)}
        <article class="card">
          <div class="cover">
            {#if preview}
              <img src={preview} alt="" loading="lazy" decoding="async" />
            {:else}
              <span class="glyph"><Car size={44} weight="thin" /></span>
            {/if}
            {#if card.price !== "unknown"}
              <span class="price {card.price}">{priceLabel(card.price)}</span>
            {/if}
            {#if card.installed}
              <span class="installed">У вас стоит</span>
            {/if}
            {#if searchKindLabel(card)}
              <span class="kind-tag {card.kind === "map" ? "map" : card.kind}">{searchKindLabel(card)}</span>
            {/if}
          </div>

          <div class="card-body">
            <h2>{card.title}</h2>
            <div class="listings">
              {#each card.listings as listing (listing.source + listing.sourceId)}
                <div class="listing">
                  <div class="listing-main">
                    <span class="site-name">{sourceName(listing.source)}</span>
                    <span class="listing-title" title={listing.title}>{listing.name || listing.title}</span>
                    {#if versionText(listing)}
                      <span class="version mono">{versionText(listing)}</span>
                    {/if}
                    {#if metaText(card, listing)}
                      <span class="meta">{metaText(card, listing)}</span>
                    {/if}
                  </div>
                  <div class="actions">
                    {#if downloadOffer(listing)}
                      <button
                        class="action"
                        type="button"
                        title={`Скачать на ${downloadOffer(listing)?.host}`}
                        onclick={() => void openLink(downloadOffer(listing)!.url)}
                      >
                        Скачать
                      </button>
                    {:else}
                      <button
                        class="action"
                        type="button"
                        onclick={() => void openLink(listing.url)}
                      >
                        Открыть
                      </button>
                    {/if}
                    {#if hasPaid(listing)}
                      <button
                        class="action buy"
                        type="button"
                        onclick={() => void openLink(paidOffer(listing)!.url)}
                      >
                        Купить
                      </button>
                    {/if}
                    <button
                      class="icon-action"
                      type="button"
                      aria-label={`Открыть ${listing.title} на ${sourceName(listing.source)}`}
                      onclick={() => void openLink(listing.url)}
                    >
                      <ArrowSquareOut size={14} />
                    </button>
                  </div>
                </div>
              {/each}
            </div>

            {#if card.possibleSameAs.length > 0}
              <div class="maybe-block">
                {#each card.possibleSameAs as otherId (otherId)}
                  {@const other = cardById(otherId)}
                  {#if other}
                    <div class="maybe">
                      <span>Возможно, тот же мод: <b>{other.title}</b></span>
                      <div>
                        <button
                          type="button"
                          disabled={deciding !== null}
                          onclick={() => void decide(card, other, true)}
                        >
                          Это один мод
                        </button>
                        <button
                          type="button"
                          disabled={deciding !== null}
                          onclick={() => void decide(card, other, false)}
                        >
                          Это разные
                        </button>
                      </div>
                    </div>
                  {/if}
                {/each}
              </div>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {:else}
    <div class="state">
      <p>Найдите мод сразу везде</p>
      <span>
        Напишите название и нажмите «Найти». Карточки появляются по мере ответов сайтов,
        одинаковые моды с разных сайтов собираются в одну.
      </span>
    </div>
  {/if}
  </section>

  <aside class="panel">
    <h2 class="display">Сайты</h2>
    <p class="sub">Один мод с разных сайтов собран в одну карточку.</p>
    {#if sources.length > 0}
      <div class="site-list">
        {#each sources as source (source.id)}
          <div class="site-row" title={source.note ?? source.displayName}>
            <span
              class="dot"
              class:pending={source.pending}
              class:warn={source.status === "needsHumanCheck"}
              class:bad={source.status === "unavailable"}
            ></span>
            <b>{source.displayName}</b>
            <span class="state-text">
              {#if source.pending}
                ищем…
              {:else if source.status === "needsHumanCheck"}
                просит проверку
              {:else if source.status === "unavailable"}
                не ответил
              {:else}
                нашёл {source.count}
              {/if}
            </span>
          </div>
          {#if source.id === "modland" && source.status === "needsHumanCheck"}
            <button class="btn-glass pass" type="button" disabled={passing} onclick={() => void openModland()}>
              {passing ? "Ждём, пока сайт пустит…" : "Пройти проверку Modland"}
            </button>
          {/if}
        {/each}
      </div>
    {:else}
      <p class="sub">Modland, STMods, ModsGaming и репозиторий BeamNG — сразу, одним запросом.</p>
    {/if}
    <p class="note">
      Платные моды не скачиваются: вместо «Скачать» кнопка ведёт на страницу автора.
    </p>
    <a class="link" href="/settings">Включить или выключить сайты</a>
  </aside>
  </div>
</div>

<style>
  .search-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 16px 24px;
    padding: 18px 150px 14px 24px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    flex: 0 0 auto;
  }

  h1 {
    margin: 0;
    font-size: 30px;
    line-height: 1.05;
  }

  .title p {
    margin: 3px 0 0;
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .bar {
    display: flex;
    flex: 1 1 420px;
    align-items: center;
    gap: 10px;
    max-width: 680px;
    height: 46px;
    padding: 0 5px 0 14px;
    box-sizing: border-box;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--muted-foreground);
    transition:
      border-color var(--dur-2) var(--ease-out),
      box-shadow var(--dur-2) var(--ease-out);
  }

  .bar:focus-within {
    border-color: var(--accent-brand);
    box-shadow: 0 0 0 3px var(--accent-brand-soft);
  }

  .bar input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--foreground);
    font-size: 15.5px;
    font-weight: 600;
    outline: none;
  }

  .bar input::-webkit-search-cancel-button {
    display: none;
  }

  .clear {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: default;
  }

  .clear:hover {
    background: var(--surface-3);
    color: var(--foreground);
  }

  .go {
    height: 34px;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    padding: 12px 24px;
    border-bottom: 1px solid var(--border);
    animation: fade-in var(--dur-2) var(--ease-out) both;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: 15px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 13px;
    font-weight: 600;
    cursor: default;
    transition:
      background-color var(--dur-2) var(--ease-out),
      color var(--dur-2) var(--ease-out);
  }

  .chip b {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 500;
    color: var(--faint-foreground);
  }

  .chip:hover {
    color: var(--foreground);
  }

  .chip.on {
    border-color: var(--foreground);
    background: var(--foreground);
    color: var(--background);
  }

  .chip.on b {
    color: color-mix(in oklab, var(--background) 70%, transparent);
  }

  .banner {
    margin: 12px 24px 0;
    padding: 10px 14px;
    border-radius: 8px;
    font-size: 13px;
  }

  .banner.error {
    border: 1px solid color-mix(in oklab, var(--danger) 45%, transparent);
    background: var(--danger-soft);
    color: var(--danger);
  }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    flex: 1;
    min-height: 0;
  }

  .results {
    min-width: 0;
    overflow-y: auto;
    padding: 20px 24px 24px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
    gap: 16px;
    align-items: start;
  }

  .card {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    animation: rise-in var(--dur-3) var(--ease-out) both;
    transition: border-color var(--dur-2) var(--ease-out);
  }

  .card:hover {
    border-color: var(--border-strong);
  }

  .cover {
    position: relative;
    height: 150px;
    overflow: hidden;
    background: var(--surface-2);
  }

  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    animation: fade-in var(--dur-3) var(--ease-out) both;
  }

  .glyph {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--faint-foreground);
    opacity: 0.6;
  }

  .cover .kind-tag {
    position: absolute;
    left: 10px;
    top: 10px;
  }

  .price,
  .installed {
    position: absolute;
    display: inline-flex;
    align-items: center;
    height: 22px;
    padding: 0 8px;
    border-radius: 5px;
    font-size: 11px;
    font-weight: 700;
  }

  .price {
    right: 10px;
    top: 10px;
    border: 1px solid var(--border-strong);
    background: var(--background);
    color: var(--muted-foreground);
  }

  .price.free {
    color: var(--ok);
  }

  .price.paid,
  .price.mixed {
    border-color: var(--warning-border);
    color: var(--warning);
  }

  .installed {
    right: 10px;
    bottom: 10px;
    border: 1px solid color-mix(in oklab, var(--ok) 50%, transparent);
    background: var(--background);
    color: var(--ok);
  }

  .card-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 14px 14px;
  }

  h2 {
    margin: 0;
    overflow: hidden;
    font-size: 15.5px;
    font-weight: 800;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .listings {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .listing {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }

  .listing-main {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .site-name {
    color: var(--muted-foreground);
    font-size: 10.5px;
    font-weight: 800;
    letter-spacing: 0.07em;
    text-transform: uppercase;
  }

  .listing-title {
    overflow: hidden;
    font-size: 13px;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .version {
    overflow: hidden;
    color: var(--muted-foreground);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta {
    overflow: hidden;
    color: var(--faint-foreground);
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 4px;
  }

  .action {
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--surface);
    color: var(--foreground);
    font-size: 12.5px;
    font-weight: 700;
    cursor: default;
    transition: background-color var(--dur-1) var(--ease-out);
  }

  .action:hover {
    background: var(--surface-3);
  }

  .action.buy {
    border-color: var(--warning-border);
    color: var(--warning);
  }

  .icon-action {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: default;
  }

  .icon-action:hover {
    background: var(--surface-3);
    color: var(--foreground);
  }

  .maybe-block {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .maybe {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border: 1px dashed var(--warning-border);
    border-radius: 8px;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .maybe b {
    color: var(--foreground);
  }

  .maybe div {
    display: flex;
    gap: 6px;
  }

  .maybe button {
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--surface);
    color: var(--foreground);
    font-size: 12.5px;
    font-weight: 700;
    cursor: default;
  }

  .ghost {
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
  }

  .ghost-cover {
    height: 150px;
  }

  .ghost-line {
    width: 50%;
    height: 12px;
    margin: 12px 14px 0;
    border-radius: 4px;
  }

  .ghost-line.wide {
    width: 75%;
  }

  .ghost-line:last-child {
    margin-bottom: 14px;
  }

  .state {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    min-height: 320px;
    color: var(--muted-foreground);
    text-align: center;
    animation: fade-in var(--dur-3) var(--ease-out) both;
  }

  .state p {
    margin: 0;
    color: var(--foreground);
    font-family: var(--font-display);
    font-size: 22px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  .state span {
    max-width: 460px;
    font-size: 13.5px;
    line-height: 1.5;
  }

  .panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-height: 0;
    overflow-y: auto;
    padding: 20px;
    border-left: 1px solid var(--border);
    background: var(--surface);
  }

  .panel h2 {
    font-size: 20px;
    font-weight: 600;
  }

  .sub,
  .note {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 13px;
    line-height: 1.45;
  }

  .note {
    color: var(--faint-foreground);
    font-size: 12.5px;
  }

  .site-list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .site-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    font-size: 13.5px;
  }

  .site-row + .site-row,
  .pass + .site-row {
    border-top: 1px solid var(--border);
  }

  .site-row b {
    font-weight: 700;
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
  }

  .dot.pending {
    background: var(--faint-foreground);
    animation: blink 1s var(--ease-in-out) infinite alternate;
  }

  @keyframes blink {
    to {
      opacity: 0.3;
    }
  }

  .dot.warn {
    background: #f2b544;
  }

  .dot.bad {
    background: var(--danger);
  }

  .state-text {
    margin-left: auto;
    color: var(--muted-foreground);
    font-size: 12px;
    white-space: nowrap;
  }

  .pass {
    margin: 0 12px 10px;
    height: 32px;
  }

  .link {
    color: var(--accent-brand-text);
    font-size: 13px;
    font-weight: 700;
    text-decoration: none;
  }

  .link:hover {
    text-decoration: underline;
  }

  @media (max-width: 1100px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
    }

    .panel {
      border-left: 0;
      border-top: 1px solid var(--border);
    }
  }
</style>

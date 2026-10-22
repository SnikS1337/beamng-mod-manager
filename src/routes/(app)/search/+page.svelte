<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { ExternalLink, Search, X } from "@lucide/svelte";
  import { formatSize } from "$lib/features/garage/format";
  import {
    listenSearch,
    searchCancel,
    searchDecide,
    searchPreview,
    searchStart,
    sourceName,
    type SearchCard,
    type SearchEvent,
    type SearchListing,
    type SearchSource,
    type SearchPriceSummary,
  } from "$lib/ipc/search";

  let query = $state("");
  let submitted = $state("");
  let searching = $state(false);
  let searched = $state(false);
  let cards = $state<SearchCard[]>([]);
  let sources = $state<SearchSource[]>([]);
  let error = $state<string | null>(null);
  let deciding = $state<string | null>(null);
  let previews = $state<Record<string, string>>({});
  const requestedPreviews = new Set<string>();
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
      if (!url) continue;
      requestedPreviews.add(card.id);
      void searchPreview(url).then((value) => {
        if (value) previews[card.id] = value;
      });
    }
  });

  onMount(() => {
    input?.focus();
    void listenSearch(onEvent).then((stop) => {
      unlisten = stop;
    });
    return () => unlisten?.();
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

  function metaText(listing: SearchListing): string {
    return [listing.author, listing.category, formatSize(listing.sizeBytes ?? 0)]
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

  function hue(text: string): number {
    let hash = 0;
    for (let index = 0; index < text.length; index += 1) {
      hash = (hash * 31 + text.charCodeAt(index)) % 360;
    }
    return hash;
  }

  function placeholder(card: SearchCard): string {
    const value = hue(card.title);
    return `radial-gradient(120% 90% at 18% 0%, hsl(${value} 85% 68% / 0.8), transparent 62%), linear-gradient(135deg, hsl(${value} 38% 28%), hsl(${(value + 58) % 360} 42% 16%))`;
  }

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось выполнить поиск.";
  }

  function onkeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
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
  <header class="head">
    <div>
      <h1>Поиск</h1>
      <p>
        {#if searching}
          ищем на сайтах…
        {:else if searched}
          {cards.length}
          {cards.length === 1 ? "мод" : cards.length >= 2 && cards.length <= 4 ? "мода" : "модов"}
          · {totalListings}
          {totalListings === 1 ? "страница" : totalListings >= 2 && totalListings <= 4 ? "страницы" : "страниц"}
        {:else}
          один запрос сразу на Modland, STMods, ModsGaming и официальный репозиторий BeamNG
        {/if}
      </p>
    </div>
    {#if searching}
      <button class="cancel glass liquid-edge" type="button" onclick={() => void cancel()}>
        Отменить
      </button>
    {/if}
  </header>

  <form class="bar glass liquid-edge" onsubmit={runSearch}>
    <Search size={17} />
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
        <X size={14} />
      </button>
    {/if}
    <button class="go" type="submit" disabled={searching || query.trim().length < 2}>
      {searching ? "Ищем…" : "Найти"}
    </button>
  </form>

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  {#if sources.length > 0}
    <div class="sites">
      {#each sources as source (source.id)}
        <div
          class="site"
          class:pending={source.pending}
          class:warn={source.status !== "ok"}
          title={source.note ?? source.displayName}
        >
          <span class="dot"></span>
          <b>{source.displayName}</b>
          <span>
            {#if source.pending}
              ищем…
            {:else if source.status === "needsHumanCheck"}
              проверка
            {:else if source.status === "unavailable"}
              недоступен
            {:else}
              {source.count}
            {/if}
          </span>
        </div>
      {/each}
    </div>
  {/if}

  {#if searching && cards.length === 0}
    <div class="grid skeletons">
      {#each Array(6) as _}
        <div class="skeleton glass">
          <div class="skeleton-cover"></div>
          <div class="skeleton-line wide"></div>
          <div class="skeleton-line"></div>
        </div>
      {/each}
    </div>
  {:else if searched && cards.length === 0 && !searching && !error}
    <div class="state glass liquid-edge">
      <p>Ничего не нашлось</p>
      <span>Попробуйте короче: название машины, карты или автора. Сайты ищут по-разному.</span>
    </div>
  {:else if cards.length > 0}
    <div class="grid">
      {#each cards as card (card.id)}
        {@const preview = previewOf(card)}
        <article class="card glass liquid-edge">
          <div class="cover" style={`background:${placeholder(card)}`}>
            {#if preview}
              <img src={preview} alt="" loading="lazy" />
            {:else}
              <span class="glyph">{card.title.slice(0, 1).toUpperCase()}</span>
            {/if}
            <span class="price {card.price}">{priceLabel(card.price)}</span>
            {#if card.installed}
              <span class="installed">Установлен</span>
            {/if}
          </div>

          <div class="card-body">
            <h2>{card.title}</h2>
            <div class="listings">
              {#each card.listings as listing (listing.source + listing.sourceId)}
                <div class="listing">
                  <div class="listing-main">
                    <span class="site-name">{sourceName(listing.source)}</span>
                    <span class="listing-title" title={listing.title}>{listing.title}</span>
                    {#if versionText(listing)}
                      <span class="version">{versionText(listing)}</span>
                    {/if}
                    {#if metaText(listing)}
                      <span class="meta">{metaText(listing)}</span>
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
                      <ExternalLink size={13} />
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
    <div class="state glass liquid-edge">
      <p>Найдите мод сразу везде</p>
      <span>
        Напишите название и нажмите «Найти». Карточки появляются по мере ответов сайтов,
        одинаковые моды с разных сайтов собираются в одну.
      </span>
    </div>
  {/if}
</div>

<style>
  .search-page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 100%;
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    padding: 4px 4px 0;
  }

  h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 650;
    letter-spacing: -0.02em;
  }

  .head p {
    margin: 4px 0 0;
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .cancel {
    height: 30px;
    padding: 0 13px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    color: var(--foreground);
    font-size: 10.5px;
    cursor: default;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 7px 6px 14px;
    border-radius: 16px;
    color: var(--muted-foreground);
  }

  .bar input {
    min-width: 0;
    flex: 1;
    height: 32px;
    border: 0;
    background: transparent;
    color: var(--foreground);
    font-size: 13px;
    outline: none;
  }

  .bar input::-webkit-search-cancel-button {
    display: none;
  }

  .clear {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: default;
  }

  .clear:hover {
    color: var(--foreground);
  }

  .go {
    height: 32px;
    min-width: 86px;
    padding: 0 16px;
    border: 1px solid transparent;
    border-radius: 12px;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 11px;
    font-weight: 650;
    cursor: default;
  }

  .go:disabled {
    opacity: 0.45;
  }

  .banner {
    padding: 9px 12px;
    border: 1px solid var(--glass-border);
    border-radius: 12px;
    font-size: 10.5px;
  }

  .banner.error {
    border-color: color-mix(in oklab, oklch(0.75 0.16 25) 35%, transparent);
    background: color-mix(in oklab, oklch(0.75 0.16 25) 10%, transparent);
    color: oklch(0.82 0.12 25);
  }

  .sites {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .site {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .site b {
    color: var(--foreground);
    font-weight: 600;
  }

  .site .dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--primary);
  }

  .site.warn {
    border-color: color-mix(in oklab, oklch(0.82 0.15 85) 35%, transparent);
  }

  .site.warn .dot {
    background: oklch(0.82 0.15 85);
  }

  .site.pending .dot {
    animation: pulse 1.2s ease-in-out infinite;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
    gap: 12px;
    align-items: start;
  }

  .card {
    overflow: hidden;
    border-radius: 18px;
    min-width: 0;
  }

  .cover {
    position: relative;
    display: grid;
    place-items: center;
    height: 118px;
    overflow: hidden;
  }

  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .glyph {
    font-size: 48px;
    font-weight: 700;
    color: rgba(255, 255, 255, 0.28);
  }

  .price,
  .installed {
    position: absolute;
    top: 9px;
    padding: 3px 8px;
    border-radius: 999px;
    background: rgba(6, 10, 20, 0.66);
    color: var(--foreground);
    font-size: 9px;
    font-weight: 600;
    backdrop-filter: blur(6px);
  }

  .price {
    right: 9px;
  }

  .price.free {
    color: oklch(0.83 0.14 150);
  }

  .price.paid,
  .price.mixed {
    color: oklch(0.84 0.12 85);
  }

  .installed {
    left: 9px;
    color: oklch(0.84 0.13 265);
  }

  .card-body {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 12px 14px 13px;
  }

  h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 620;
    letter-spacing: -0.01em;
    overflow: hidden;
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
    justify-content: space-between;
    gap: 8px;
    padding: 7px 8px;
    border: 1px solid color-mix(in oklab, var(--foreground) 8%, transparent);
    border-radius: 11px;
    background: color-mix(in oklab, var(--foreground) 3%, transparent);
  }

  .listing-main {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .site-name {
    color: var(--primary);
    font-size: 9px;
    font-weight: 650;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  .listing-title {
    overflow: hidden;
    color: var(--foreground);
    font-size: 11px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .version,
  .meta {
    color: var(--muted-foreground);
    font-size: 9.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    align-items: center;
    flex: none;
    gap: 4px;
  }

  .action {
    height: 23px;
    padding: 0 8px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: transparent;
    color: var(--foreground);
    font-size: 9px;
    cursor: default;
  }

  .action:hover {
    border-color: color-mix(in oklab, var(--foreground) 32%, transparent);
  }

  .action.buy {
    color: oklch(0.84 0.12 85);
  }

  .icon-action {
    display: grid;
    place-items: center;
    width: 23px;
    height: 23px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: default;
  }

  .icon-action:hover {
    color: var(--foreground);
  }

  .maybe-block {
    display: flex;
    flex-direction: column;
    gap: 5px;
    margin-top: 2px;
    padding-top: 8px;
    border-top: 1px dashed color-mix(in oklab, var(--foreground) 13%, transparent);
  }

  .maybe {
    display: flex;
    flex-direction: column;
    gap: 5px;
    color: var(--muted-foreground);
    font-size: 9.5px;
    line-height: 1.35;
  }

  .maybe b {
    color: var(--foreground);
  }

  .maybe > div {
    display: flex;
    gap: 5px;
  }

  .maybe button {
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: transparent;
    color: var(--foreground);
    font-size: 9px;
    cursor: default;
  }

  .maybe button:disabled {
    opacity: 0.5;
  }

  .state {
    display: grid;
    place-content: center;
    gap: 6px;
    min-height: 280px;
    padding: 24px;
    border-radius: 20px;
    color: var(--muted-foreground);
    text-align: center;
  }

  .state p {
    margin: 0;
    color: var(--foreground);
    font-size: 15px;
    font-weight: 600;
  }

  .state span {
    max-width: 430px;
    font-size: 11px;
    line-height: 1.45;
  }

  .skeletons {
    grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  }

  .skeleton {
    height: 190px;
    border-radius: 18px;
    padding: 0 0 12px;
  }

  .skeleton-cover {
    height: 112px;
    border-radius: 18px 18px 0 0;
    background: color-mix(in oklab, var(--foreground) 7%, transparent);
  }

  .skeleton-line {
    width: 62%;
    height: 9px;
    margin: 12px 14px 0;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
    animation: breathe 1.3s ease-in-out infinite;
  }

  .skeleton-line.wide {
    width: 80%;
  }

  @keyframes breathe {
    0%,
    100% {
      opacity: 0.55;
    }
    50% {
      opacity: 1;
    }
  }

  @keyframes pulse {
    0%,
    100% {
      opacity: 0.25;
    }
    50% {
      opacity: 1;
    }
  }

  @media (max-width: 760px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>

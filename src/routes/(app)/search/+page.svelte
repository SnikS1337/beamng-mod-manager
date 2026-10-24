<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { ExternalLink, Search, X } from "@lucide/svelte";
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
  <header class="top" data-tauri-drag-region>
    <div class="title" data-tauri-drag-region>
      <h1 data-tauri-drag-region>Поиск</h1>
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
      {#if searching}
        <button class="go" type="button" onclick={() => void cancel()}>Остановить</button>
      {:else}
        <button class="go" type="submit" disabled={query.trim().length < 2}>Найти</button>
      {/if}
    </form>
    <div></div>
  </header>

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  <div class="body">
  <section class="results scroll-area">
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
            {#if card.price !== "unknown"}
              <span class="price {card.price}">{priceLabel(card.price)}</span>
            {/if}
            {#if card.installed}
              <span class="installed">Установлен</span>
            {/if}
            {#if searchKindLabel(card)}
              <span class="kind {card.kind}">{searchKindLabel(card)}</span>
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
  </section>

  <aside class="panel glass liquid-edge">
    <h2>Сайты</h2>
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
  .top {
    display: grid;
    grid-template-columns: minmax(150px, 1fr) minmax(360px, 620px) minmax(0, 1fr);
    align-items: center;
    gap: 16px;
    min-height: 52px;
    padding-right: 148px;
  }

  .title {
    min-width: 0;
  }

  .title p {
    margin: 3px 0 0;
    color: color-mix(in oklab, var(--foreground) 66%, transparent);
    font-size: 13px;
    white-space: nowrap;
  }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 328px;
    gap: 16px;
    flex: 1;
    min-height: 0;
  }

  .results {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 0;
    padding: 2px 4px 16px 2px;
    overflow-y: auto;
  }

  .panel {
    align-self: start;
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 20px 18px;
    border-radius: 32px;
    background: var(--glass-fill-strong);
  }

  .panel h2 {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
  }

  .panel .sub,
  .panel .note {
    margin: 0;
    color: color-mix(in oklab, var(--foreground) 70%, transparent);
    font-size: 13px;
    line-height: 1.4;
  }

  .panel .link {
    color: var(--foreground);
    font-size: 13px;
    font-weight: 600;
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .site-list {
    display: flex;
    flex-direction: column;
    padding: 4px 14px;
    border-radius: 16px;
    background: color-mix(in oklab, var(--foreground) 7%, transparent);
  }

  .site-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 0;
    font-size: 13px;
  }

  .site-row + .site-row {
    border-top: 1px solid color-mix(in oklab, var(--foreground) 7%, transparent);
  }

  .site-row b {
    flex: 1;
    font-weight: 600;
  }

  .state-text {
    color: color-mix(in oklab, var(--foreground) 62%, transparent);
    font-size: 12.5px;
  }

  .site-row .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: #34c759;
  }

  .site-row .dot.pending {
    background: color-mix(in oklab, var(--foreground) 35%, transparent);
  }

  .site-row .dot.warn {
    background: #ffb340;
  }

  .site-row .dot.bad {
    background: #ff6b5e;
  }

  .pass {
    width: 100%;
    height: 32px;
    margin: 2px 0 8px;
    font-size: 12.5px;
  }

  @media (max-width: 1100px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
    }

    .panel {
      order: -1;
    }
  }

  .search-page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    height: 100%;
    min-height: 0;
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
    font-weight: 700;
    line-height: 1.1;
    letter-spacing: -0.02em;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 52px;
    padding: 0 8px 0 18px;
    border-radius: 26px;
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
    height: 36px;
    min-width: 96px;
    padding: 0 18px;
    border: 1px solid transparent;
    border-radius: 999px;
    box-shadow: 0 6px 22px var(--primary-glow);
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 13px;
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
    font-size: 12.5px;
  }

  .banner.error {
    border-color: color-mix(in oklab, oklch(0.75 0.16 25) 35%, transparent);
    background: color-mix(in oklab, oklch(0.75 0.16 25) 10%, transparent);
    color: oklch(0.82 0.12 25);
  }

  .banner.warn {
    border-color: var(--warning-border);
    background: var(--warning-soft);
    color: var(--warning);
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
    color: rgb(255 255 255 / 0.9);
    font-size: 11px;
    font-weight: 600;
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

  .kind {
    position: absolute;
    left: 9px;
    bottom: 9px;
    padding: 3px 8px;
    border-radius: 6px;
    background: rgba(6, 10, 20, 0.72);
    color: rgb(255 255 255 / 0.92);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
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
    font-size: 11px;
    font-weight: 650;
    letter-spacing: 0.02em;
    text-transform: uppercase;
  }

  .listing-title {
    overflow: hidden;
    color: var(--foreground);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .version,
  .meta {
    color: var(--muted-foreground);
    font-size: 11px;
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
    font-size: 11px;
    cursor: default;
  }

  .action:hover {
    border-color: color-mix(in oklab, var(--foreground) 32%, transparent);
  }

  .action.buy {
    color: var(--warning);
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
    font-size: 11px;
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
    font-size: 11px;
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
    font-size: 13px;
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

<script lang="ts">
  import { openExternal } from "#lib/shell/openExternal.js";
  import { isShortcut } from "#lib/shell/keys.js";
  import { onDestroy, onMount } from "svelte";
  import { page } from "$app/state";
  import ArrowSquareOut from "phosphor-svelte/lib/ArrowSquareOut";
  import Funnel from "phosphor-svelte/lib/Funnel";
  import MagnifyingGlass from "phosphor-svelte/lib/MagnifyingGlass";
  import X from "phosphor-svelte/lib/X";
  import Car from "phosphor-svelte/lib/Car";
  import { formatSize, plural } from "#lib/features/garage/format.js";
  import { categoryIntent, type BrowseKind } from "#lib/features/search/category.js";
  import { searchKindLabel } from "#lib/features/search/kind.js";
  import { filterAndSort, type SearchKindFilter, type SearchPriceFilter, type SearchSortOrder } from "#lib/features/search/filters.js";
  import { parseSearchInput } from "#lib/features/search/queryInput.js";
  import { suggest } from "#lib/features/search/suggest.js";
  import { libraryLoad } from "#lib/ipc/library.js";
  import { downloadOpen } from "#lib/ipc/download.js";
  import { flowName, motion } from "#lib/shell/motion.svelte.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import {
    listenSearch,
    searchBrowse,
    searchCancel,
    searchDecide,
    searchOpenModland,
    searchPagePreview,
    searchPreview,
    searchRetrySource,
    searchStart,
    sourceName,
    type SearchCard,
    type SearchEvent,
    type SearchListing,
    type SearchSource,
    type SearchPriceSummary,
  } from "#lib/ipc/search.js";

  const EXAMPLES = ["Audi RS3", "Rolls-Royce", "Nissan Laurel", "карта"] as const;

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
  let previewGeneration = $state(0);
  const requestedPreviews = new Set<string>();
  const requestedPages = new Set<string>();
  let pageRequests = 0;
  const PAGE_PREVIEW_LIMIT = 24;
  let requestId = 0;
  let input = $state<HTMLInputElement | null>(null);
  let unlisten: (() => void) | undefined;
  let disposed = false;
  let retrying = $state<string | null>(null);
  let libraryNames = $state<string[]>([]);
  let suggestionsOpen = $state(false);
  let activeSuggestion = $state(-1);
  let browseKind = $state<BrowseKind | null>(null);
  let browseFrom = 0;
  let browseHasMore = $state(false);
  let filtersOpen = $state(false);
  let listenerReady = Promise.resolve();
  let markListenerReady: (() => void) | undefined;
  let listenFailed = false;

  const totalListings = $derived(
    cards.reduce((sum, card) => sum + card.listings.length, 0),
  );

  $effect(() => {
    const generation = previewGeneration;
    for (const card of cards) {
      if (previews[card.id] || requestedPreviews.has(card.id)) continue;
      const url = card.listings.find((listing) => listing.previewUrl)?.previewUrl;
      if (url) {
        requestedPreviews.add(card.id);
        void searchPreview(url).then((value) => {
          if (value && generation === previewGeneration) previews[card.id] = value;
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
          if (value && generation === previewGeneration) previews[card.id] = value;
        })
        .catch(() => {});
    }
  });

  let listening = $state(false);

  onMount(() => {
    input?.focus();
    loadFilters();
    filtersLoaded = true;
    loadHistory();
    void libraryLoad()
      .then((snapshot) => {
        if (snapshot.gameVersion) gameVersion = snapshot.gameVersion;
        libraryNames = snapshot.mods.map((mod) => mod.name);
      })
      .catch(() => {});
    listenerReady = new Promise<void>((resolve) => {
      markListenerReady = resolve;
    });
    void listenSearch(onEvent)
      .then((stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      })
      .catch(() => {
        listenFailed = true;
        error = "Не удалось подключиться к поиску.";
      })
      .finally(() => {
        listening = true;
        markListenerReady?.();
      });
    return () => {
      disposed = true;
      unlisten?.();
      markListenerReady?.();
    };
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
    disposed = true;
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

  function resetPreviews() {
    previewGeneration += 1;
    previews = {};
    requestedPreviews.clear();
    requestedPages.clear();
    pageRequests = 0;
  }

  async function runSearch(event?: Event) {
    event?.preventDefault();
    closeSuggestions();
    const text = query.trim();
    const parsed = parseSearchInput(text);
    // «карта» и «машины» — не текст, а раздел сайтов.
    const intent = parsed.author === null ? categoryIntent(parsed.text) : null;
    if (intent) {
      if (parsed.source) sourceFilter = parsed.source;
      await openBrowse(intent, text);
      return;
    }
    if (!parsed.text && !parsed.author && parsed.kind && parsed.kind !== "all") {
      if (parsed.source) sourceFilter = parsed.source;
      await openBrowse(parsed.kind);
      return;
    }
    const searchText = parsed.text || parsed.author || "";
    if (searchText.length < 2) {
      error = "Введите хотя бы 2 символа названия, автора или сайта.";
      return;
    }
    if (searching) await searchCancel(requestId).catch(() => {});
    authorFilter = parsed.author;
    if (parsed.kind) kindFilter = parsed.kind;
    if (parsed.source) sourceFilter = parsed.source;
    browseKind = null;
    browseFrom = 0;
    browseHasMore = false;
    if (sortOrder === "site") sortOrder = "relevance";
    requestId += 1;
    const current = requestId;
    submitted = searchText;
    rememberQuery(text);
    searching = true;
    searched = true;
    error = null;
    cards = [];
    sources = [];
    resetPreviews();
    try {
      await listenerReady;
      if (disposed || current !== requestId) return;
      if (listenFailed) {
        error = "Не удалось подключиться к поиску.";
        searching = false;
        return;
      }
      await searchStart(searchText, current);
    } catch (reason) {
      if (current === requestId) {
        error = errorText(reason);
        searching = false;
      }
    }
  }

  async function openBrowse(kind: BrowseKind, entryText?: string): Promise<void> {
    if (searching) await searchCancel(requestId).catch(() => {});
    closeSuggestions();
    if (entryText) rememberQuery(entryText);
    requestId += 1;
    const current = requestId;
    browseKind = kind;
    browseFrom = 1;
    browseHasMore = false;
    kindFilter = "all";
    submitted = "";
    if (sortOrder !== "site" && sortOrder !== "largest") sortOrder = "site";
    searching = true;
    searched = true;
    error = null;
    cards = [];
    sources = [];
    resetPreviews();
    try {
      const event = await searchBrowse(kind, 1, current);
      if (current !== requestId) return;
      cards = event.cards;
      sources = event.sources;
      browseFrom = 3;
      browseHasMore = event.cards.length > 0;
    } catch (reason) {
      if (current === requestId) error = errorText(reason);
    } finally {
      if (current === requestId) searching = false;
    }
  }

  async function loadMore(): Promise<void> {
    if (!browseKind || !browseHasMore || searching) return;
    requestId += 1;
    const current = requestId;
    const before = cards.length;
    searching = true;
    error = null;
    try {
      const event = await searchBrowse(browseKind, browseFrom, current);
      if (current !== requestId) return;
      cards = event.cards;
      sources = event.sources;
      browseFrom += 2;
      browseHasMore = cards.length > before;
    } catch (reason) {
      if (current === requestId) error = errorText(reason);
    } finally {
      if (current === requestId) searching = false;
    }
  }

  function leaveBrowse() {
    browseKind = null;
    browseFrom = 0;
    browseHasMore = false;
    cards = [];
    sources = [];
    searched = false;
    submitted = "";
    query = "";
    error = null;
    resetPreviews();
    input?.focus();
  }

  function retryAll() {
    if (browseKind) void openBrowse(browseKind);
    else void runSearch();
  }

  async function retrySource(source: SearchSource): Promise<void> {
    if (retrying !== null || searching) return;
    const at = requestId;
    retrying = source.id;
    error = null;
    try {
      const event = await searchRetrySource(submitted, source.id, at);
      if (at === requestId) {
        cards = event.cards;
        sources = event.sources;
      }
    } catch (reason) {
      if (at === requestId) error = errorText(reason);
    } finally {
      // Новый поиск мог перехватить повтор — кнопку всё равно разблокируем.
      retrying = null;
    }
  }

  function openSuggestions() {
    if (searching) return;
    suggestionsOpen = true;
    activeSuggestion = -1;
  }

  function closeSuggestions() {
    suggestionsOpen = false;
    activeSuggestion = -1;
  }

  function chooseSuggestion(value: string) {
    query = value;
    closeSuggestions();
    void runSearch();
  }

  function onSearchKeydown(event: KeyboardEvent) {
    const list = suggestions;
    if (!suggestionsOpen || list.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      activeSuggestion = (activeSuggestion + 1) % list.length;
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      activeSuggestion = activeSuggestion <= 0 ? list.length - 1 : activeSuggestion - 1;
    } else if (event.key === "Enter" && activeSuggestion >= 0) {
      const item = list[activeSuggestion];
      if (item) {
        event.preventDefault();
        chooseSuggestion(item.value);
      }
    }
  }

  async function cancel() {
    const current = requestId;
    requestId += 1;
    searching = false;
    await searchCancel(current).catch(() => {});
  }

  function searchExample(example: string) {
    query = example;
    void runSearch();
  }

  function clear() {
    closeSuggestions();
    browseKind = null;
    browseFrom = 0;
    browseHasMore = false;
    query = "";
    submitted = "";
    searched = false;
    cards = [];
    sources = [];
    resetPreviews();
    resetFilters();
    error = null;
    input?.focus();
  }

  async function openLink(url: string) {
    await openExternal(url);
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
    const at = requestId;
    deciding = card.id;
    error = null;
    try {
      const event = await searchDecide(
        submitted,
        { source: a.source, sourceId: a.sourceId },
        { source: b.source, sourceId: b.sourceId },
        same,
      );
      // Пока шёл ответ, могли запустить новый поиск: его выдача новее.
      if (at !== requestId) return;
      cards = event.cards;
      sources = event.sources;
    } catch (reason) {
      if (at === requestId) error = errorText(reason);
    } finally {
      deciding = null;
    }
  }

  function priceLabel(price: SearchPriceSummary): string {
    switch (price) {
      case "free":
        return "Бесплатно";
      case "paid":
        return "Платный";
      case "mixed":
        return "Есть платные";
      default:
        return "Цена неизвестна";
    }
  }

  function versionText(listing: SearchListing): string {
    const parts: string[] = [];
    if (listing.modVersion) parts.push(listing.modVersion);
    if (listing.gameVersion) parts.push(`игра ${listing.gameVersion}`);
    return parts.join(" · ");
  }

  function metaText(card: SearchCard, listing: SearchListing): string {
    // Раздел сайта («mashiny», «Cars») уже показан меткой типа на фото.
    const category = card.kind ? null : listing.category;
    return [category, formatSize(listing.sizeBytes ?? 0)]
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

  async function downloadListing(card: SearchCard, listing: SearchListing): Promise<void> {
    try {
      await downloadOpen({
        modName: card.title,
        installedVersion: null,
        latestVersion: listing.modVersion,
        pageUrl: listing.url,
        previewUrl: null,
        role: card.kind,
      });
    } catch {
      // В браузере окна загрузки нет — открываем страницу как раньше.
      await openLink(downloadOffer(listing)?.url ?? listing.url);
    }
  }

  function matchHint(card: SearchCard, other: SearchCard): string {
    const sameAuthor = card.listings.some((a) =>
      other.listings.some((b) => a.author && b.author && a.author.toLowerCase() === b.author.toLowerCase()),
    );
    const sameVersion = card.listings.some((a) =>
      other.listings.some((b) => a.modVersion && b.modVersion && a.modVersion === b.modVersion),
    );
    const sameSize = card.listings.some((a) =>
      other.listings.some((b) => a.sizeBytes && b.sizeBytes && a.sizeBytes === b.sizeBytes),
    );
    const hints = [
      sameAuthor ? "автор" : null,
      sameVersion ? "версия" : null,
      sameSize ? "размер" : null,
    ].filter((value): value is string => value !== null);
    return hints.length > 0 ? `Совпали: ${hints.join(", ")}` : "Похожее название";
  }

  function previewOf(card: SearchCard): string | null {
    return (
      previews[card.id] ??
      card.listings.find((listing) => listing.previewUrl?.startsWith("data:"))
        ?.previewUrl ??
      null
    );
  }

  let kindFilter = $state<SearchKindFilter>("all");
  const kindFilters: { id: Exclude<SearchKindFilter, "all">; label: string }[] = [
    { id: "vehicle", label: "Машины" },
    { id: "map", label: "Карты" },
    { id: "addon", label: "Дополнения" },
  ];
  let priceFilter = $state<SearchPriceFilter>("all");
  let onlyInstalled = $state(false);
  let filtersLoaded = false;
  let sourceFilter = $state("all");
  let sortOrder = $state<SearchSortOrder>("relevance");
  let authorFilter = $state<string | null>(null);
  let onlyCompatible = $state(false);
  let gameVersion = $state<string | null>(null);
  let history = $state<string[]>([]);

  const suggestions = $derived(
    suggestionsOpen ? suggest(query, history, libraryNames) : [],
  );

  const browsing = $derived(browseKind !== null);

  const activeFilterCount = $derived(
    (priceFilter !== "all" ? 1 : 0) +
      (onlyInstalled ? 1 : 0) +
      (sourceFilter !== "all" ? 1 : 0) +
      (onlyCompatible ? 1 : 0) +
      (authorFilter ? 1 : 0),
  );

  const sortOptions = $derived(
    browseKind
      ? [
          { id: "site" as const, label: "Как на сайтах" },
          { id: "largest" as const, label: "Сначала крупные" },
        ]
      : [
          { id: "relevance" as const, label: "По релевантности" },
          { id: "newest" as const, label: "Сначала новые" },
          { id: "largest" as const, label: "Сначала крупные" },
        ],
  );

  function browseLabel(kind: BrowseKind): string {
    return kindFilters.find((item) => item.id === kind)?.label ?? "Раздел";
  }

  const kindCount = (id: SearchKindFilter) =>
    id === "all" ? cards.length : cards.filter((card) => card.kind === id).length;

  const shownCards = $derived(
    filterAndSort(cards, {
      kind: kindFilter,
      price: priceFilter,
      installedOnly: onlyInstalled,
      source: sourceFilter,
      sort: sortOrder,
      author: authorFilter,
      gameVersion,
      onlyCompatible,
    }),
  );

  $effect(() => {
    if (!filtersLoaded) return;
    try {
      localStorage.setItem(
        FILTERS_KEY,
        JSON.stringify({
          kind: kindFilter,
          price: priceFilter,
          installedOnly: onlyInstalled,
          source: sourceFilter,
          sort: sortOrder,
          onlyCompatible,
        }),
      );
    } catch {
      // Приватный режим — просто не запоминаем.
    }
  });

  // Новый поиск мог не включить выбранный сайт — не оставляем пустой фильтр.
  $effect(() => {
    if (
      sourceFilter !== "all" &&
      sources.length > 0 &&
      !sources.some((source) => source.id === sourceFilter)
    ) {
      sourceFilter = "all";
    }
  });

  function resetFilters() {
    kindFilter = "all";
    priceFilter = "all";
    onlyInstalled = false;
    sourceFilter = "all";
    sortOrder = browseKind ? "site" : "relevance";
    authorFilter = null;
    onlyCompatible = false;
  }

  function chooseKind(next: SearchKindFilter) {
    if (next === "all") {
      if (browseKind) {
        leaveBrowse();
        return;
      }
      if (kindFilter === "all") return;
      void motion.run(() => {
        kindFilter = "all";
      }, "flow-search");
      return;
    }
    if (browseKind === next) return;
    // В разделе или на стартовом экране чип открывает раздел сайтов,
    // а в текстовой выдаче — просто фильтрует её.
    if (!browseKind && kindFilter === next) {
      void motion.run(() => {
        kindFilter = "all";
      }, "flow-search");
      return;
    }
    if (browseKind || !submitted) {
      void openBrowse(next);
      return;
    }
    void motion.run(() => {
      kindFilter = next;
    }, "flow-search");
  }

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось выполнить поиск.";
  }

  const HISTORY_KEY = "bmm.search.history";
  const FILTERS_KEY = "bmm.search.filters";

  function loadFilters(): void {
    try {
      const stored = JSON.parse(localStorage.getItem(FILTERS_KEY) ?? "null") as {
        kind?: SearchKindFilter;
        price?: SearchPriceFilter;
        installedOnly?: boolean;
        source?: string;
        sort?: SearchSortOrder;
        onlyCompatible?: boolean;
      } | null;
      if (!stored) return;
      if (["all", "vehicle", "map", "addon"].includes(stored.kind ?? "")) {
        kindFilter = stored.kind as SearchKindFilter;
      }
      if (["all", "free", "paid"].includes(stored.price ?? "")) {
        priceFilter = stored.price as SearchPriceFilter;
      }
      if (["relevance", "newest", "largest", "site"].includes(stored.sort ?? "")) {
        sortOrder = stored.sort as SearchSortOrder;
      }
      if (typeof stored.installedOnly === "boolean") onlyInstalled = stored.installedOnly;
      if (typeof stored.onlyCompatible === "boolean") onlyCompatible = stored.onlyCompatible;
      if (typeof stored.source === "string") sourceFilter = stored.source;
    } catch {
      // Повреждённые настройки фильтров не мешают поиску.
    }
  }

  function loadHistory(): void {
    try {
      const stored = JSON.parse(localStorage.getItem(HISTORY_KEY) ?? "[]");
      if (Array.isArray(stored)) {
        history = stored.filter((item): item is string => typeof item === "string").slice(0, 8);
      }
    } catch {
      history = [];
    }
  }

  function rememberQuery(text: string): void {
    const value = text.trim();
    if (!value) return;
    history = [value, ...history.filter((item) => item.toLowerCase() !== value.toLowerCase())].slice(0, 8);
    try {
      localStorage.setItem(HISTORY_KEY, JSON.stringify(history));
    } catch {
      // Приватный режим — просто не запоминаем.
    }
  }

  function clearHistory(): void {
    history = [];
    try {
      localStorage.removeItem(HISTORY_KEY);
    } catch {
      // ignore
    }
  }

  function onkeydown(event: KeyboardEvent) {
    if (isShortcut(event, "f")) {
      event.preventDefault();
      input?.focus();
      input?.select();
      return;
    }
    if (event.key === "Escape") {
      if (suggestionsOpen) {
        closeSuggestions();
        return;
      }
      if (query) clear();
    }
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="search-page">
  <header class="top" data-tauri-drag-region>
    <div class="headline" data-tauri-drag-region>
    <div class="title" data-tauri-drag-region>
      <h1 class="display" data-tauri-drag-region>Поиск</h1>
      <p data-tauri-drag-region>
        {#if browsing && browseKind && searched}
          раздел «{browseLabel(browseKind)}» · {cards.length} {plural(cards.length, "мод", "мода", "модов")}
        {:else if searching}
          ищем на сайтах…
        {:else if searched}
          {shownCards.length}{#if shownCards.length !== cards.length} из {cards.length}{/if}
          {plural(shownCards.length, "мод", "мода", "модов")}
        {:else}
          сразу на нескольких сайтах
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
        role="combobox"
        aria-autocomplete="list"
        aria-expanded={suggestionsOpen && suggestions.length > 0}
        aria-controls="search-suggestions"
        aria-activedescendant={activeSuggestion >= 0 ? `search-suggestion-${activeSuggestion}` : undefined}
        onfocus={openSuggestions}
        oninput={openSuggestions}
        onkeydown={onSearchKeydown}
        onblur={closeSuggestions}
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
      {#if suggestionsOpen && suggestions.length > 0}
        <div class="suggest" id="search-suggestions" role="listbox" aria-label="Подсказки поиска">
          {#each suggestions as item, index (item.source + item.value)}
            <button
              id={`search-suggestion-${index}`}
              type="button"
              role="option"
              aria-selected={index === activeSuggestion}
              class:active={index === activeSuggestion}
              onmousedown={(event) => event.preventDefault()}
              onclick={() => chooseSuggestion(item.value)}
            >
              <span>{item.value}</span>
              <small>{item.source === "history" ? "недавнее" : "в библиотеке"}</small>
            </button>
          {/each}
        </div>
      {/if}
    </form>
    </div>

    <div class="toolbar">
      <div class="kinds" role="group" aria-label="Тип">
        <button
          class="chip"
          class:on={browseKind === null && kindFilter === "all"}
          type="button"
          aria-pressed={browseKind === null && kindFilter === "all"}
          onclick={() => chooseKind("all")}
        >
          Все{#if searched}<b>{kindCount("all")}</b>{/if}
        </button>
        {#each kindFilters as item (item.id)}
          <button
            class="chip"
            class:on={browseKind ? browseKind === item.id : kindFilter === item.id}
            type="button"
            aria-pressed={browseKind ? browseKind === item.id : kindFilter === item.id}
            onclick={() => chooseKind(item.id)}
          >{item.label}{#if searched}<b>{kindCount(item.id)}</b>{/if}</button>
        {/each}
      </div>
      <div class="tools">
        {#if browsing}
          <button class="chip leave" type="button" onclick={leaveBrowse}>К поиску</button>
        {/if}
        <button
          class="chip filter-toggle"
          class:on={filtersOpen || activeFilterCount > 0}
          type="button"
          aria-expanded={filtersOpen}
          onclick={() => (filtersOpen = !filtersOpen)}
        >
          <Funnel size={14} />
          Фильтры
          {#if activeFilterCount > 0}<b>{activeFilterCount}</b>{/if}
        </button>
        <select bind:value={sortOrder} aria-label="Сортировка">
          {#each sortOptions as item (item.id)}
            <option value={item.id}>{item.label}</option>
          {/each}
        </select>
      </div>
    </div>

    {#if filtersOpen}
      <div class="filters-panel" role="group" aria-label="Фильтры">
        <div class="filter-row">
          <span class="filter-label">Цена</span>
          <button
            class="chip"
            class:on={priceFilter === "all"}
            type="button"
            aria-pressed={priceFilter === "all"}
            onclick={() => (priceFilter = "all")}
          >Любая</button>
          <button
            class="chip"
            class:on={priceFilter === "free"}
            type="button"
            aria-pressed={priceFilter === "free"}
            onclick={() => (priceFilter = priceFilter === "free" ? "all" : "free")}
          >Бесплатные</button>
          <button
            class="chip"
            class:on={priceFilter === "paid"}
            type="button"
            aria-pressed={priceFilter === "paid"}
            onclick={() => (priceFilter = priceFilter === "paid" ? "all" : "paid")}
          >Платные</button>
        </div>
        <div class="filter-row">
          <span class="filter-label">Показывать</span>
          <button
            class="chip"
            class:on={onlyInstalled}
            type="button"
            aria-pressed={onlyInstalled}
            onclick={() => (onlyInstalled = !onlyInstalled)}
          >Установленные</button>
          {#if gameVersion}
            <button
              class="chip"
              class:on={onlyCompatible}
              type="button"
              aria-pressed={onlyCompatible}
              title={`Установлена игра ${gameVersion}`}
              onclick={() => (onlyCompatible = !onlyCompatible)}
            >Совместимо с игрой</button>
          {/if}
          {#if authorFilter}
            <button
              class="chip on"
              type="button"
              title="Убрать фильтр по автору"
              onclick={() => (authorFilter = null)}
            >Автор: {authorFilter} ×</button>
          {/if}
        </div>
        <div class="filter-row">
          <span class="filter-label">Сайт</span>
          <select bind:value={sourceFilter} aria-label="Сайт">
            <option value="all">Все сайты</option>
            {#each sources as source (source.id)}
              <option value={source.id}>{source.displayName}</option>
            {/each}
          </select>
          <button class="chip reset" type="button" onclick={resetFilters}>Сбросить всё</button>
        </div>
      </div>
    {:else if activeFilterCount > 0}
      <div class="active-filters" aria-label="Активные фильтры">
        <span>Фильтры:</span>
        {#if priceFilter !== "all"}
          <button class="chip" type="button" onclick={() => (priceFilter = "all")}>
            {priceFilter === "free" ? "Бесплатные" : "Платные"} <b>×</b>
          </button>
        {/if}
        {#if onlyInstalled}
          <button class="chip" type="button" onclick={() => (onlyInstalled = false)}>
            Установленные <b>×</b>
          </button>
        {/if}
        {#if onlyCompatible}
          <button class="chip" type="button" onclick={() => (onlyCompatible = false)}>
            Совместимо с игрой <b>×</b>
          </button>
        {/if}
        {#if sourceFilter !== "all"}
          <button class="chip" type="button" onclick={() => (sourceFilter = "all")}>
            {sources.find((source) => source.id === sourceFilter)?.displayName ?? sourceName(sourceFilter)} <b>×</b>
          </button>
        {/if}
        {#if authorFilter}
          <button class="chip" type="button" onclick={() => (authorFilter = null)}>
            Автор: {authorFilter} <b>×</b>
          </button>
        {/if}
        <button class="clear-filters" type="button" onclick={resetFilters}>Сбросить</button>
      </div>
    {/if}
  </header>

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
      {#if browseKind}
        <p class="display">В разделе пока ничего нет</p>
        <span>Попробуйте другой раздел или найдите мод по названию.</span>
        <button class="btn-glass" type="button" onclick={leaveBrowse}>К поиску</button>
      {:else}
        <p class="display">Ничего не нашли</p>
        <span>Попробуйте короче: название машины, карты или автора. Сайты ищут по-разному.</span>
        <a class="btn-glass" href="/settings?section=sites">Какие сайты включены</a>
      {/if}
    </div>
  {:else if cards.length > 0 && shownCards.length === 0}
    <div class="state">
      <p class="display">Под фильтр ничего не подошло</p>
      <span>Измените цену, сайт или тип мода — или сбросьте фильтры.</span>
      <button class="btn-glass" type="button" onclick={resetFilters}>Сбросить фильтры</button>
    </div>
  {:else if cards.length > 0}
    <div class="grid search-grid">
      {#each shownCards as card (card.id)}
        {@const preview = previewOf(card)}
        <article class="card search-card" style={`--flow:${flowName(card.id)}`}>
          <div class="cover">
            {#if preview}
              <img src={preview} alt="" loading="lazy" decoding="async" />
            {:else}
              <span class="glyph"><Car size={44} weight="thin" /></span>
            {/if}
            {#if card.price === "paid" || card.price === "mixed"}
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
                    <span class="meta">
                      {#if listing.author}
                        <button
                          class="author"
                          type="button"
                          title="Показать моды этого автора"
                          onclick={() => (authorFilter = listing.author)}
                        >{listing.author}</button>
                        {#if metaText(card, listing)} · {/if}
                      {/if}
                      {metaText(card, listing)}
                    </span>
                  </div>
                  <div class="actions">
                    {#if downloadOffer(listing)}
                      <button
                        class="action"
                        type="button"
                        title={`Скачать на ${downloadOffer(listing)?.host}`}
                        onclick={() => void downloadListing(card, listing)}
                      >
                        Скачать
                      </button>
                    {:else if !hasPaid(listing)}
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
                        Купить у автора
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
                      <span>
                        Возможно, тот же мод: <b>{other.title}</b>
                        <small>{matchHint(card, other)}</small>
                      </span>
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
    {#if browseKind}
      <div class="more">
        {#if browseHasMore}
          <button class="btn-glass" type="button" disabled={searching} onclick={() => void loadMore()}>
            {searching ? "Загружаем…" : "Показать ещё"}
          </button>
        {:else}
          <span>Больше в разделе нет</span>
        {/if}
      </div>
    {/if}
  {:else}
    <div class="start">
      <span class="start-icon rise-in" aria-hidden="true">
        <MagnifyingGlass size={26} weight="bold" />
      </span>
      <p class="start-title rise-in" style="animation-delay:60ms">Найдите мод сразу везде</p>
      <span class="start-text rise-in" style="animation-delay:100ms">
        Напишите название машины, карты или автора и нажмите «Найти». Один мод
        с разных сайтов — одна карточка.
      </span>
      <div class="examples rise-in" style="animation-delay:140ms" role="group" aria-label="Примеры запросов">
        <span>Например:</span>
        {#each EXAMPLES as example (example)}
          <button class="example" type="button" onclick={() => searchExample(example)}>
            {example}
          </button>
        {/each}
      </div>
      <span class="start-hint rise-in" style="animation-delay:160ms">
        Уточнить: автор:Flanje · тип:карта · сайт:stmods
      </span>
      {#if history.length > 0}
        <div class="history rise-in" style="animation-delay:180ms">
          <span>Недавние:</span>
          {#each history as item (item)}
            <button type="button" onclick={() => searchExample(item)}>{item}</button>
          {/each}
          <button class="history-clear" type="button" onclick={clearHistory}>Очистить</button>
        </div>
      {/if}
    </div>
  {/if}
  </section>

  <aside class="panel" aria-label="Сайты поиска">
    <h2 class="display">Сайты</h2>
    <p class="sub">Один мод с разных сайтов — одна карточка.</p>
    {#if sources.length > 0}
      <div class="site-list">
        {#each sources as source (source.id)}
          <div class="site-row" title={source.note ?? source.displayName}>
            <span
              class="dot"
              class:pending={source.pending}
              class:warn={source.status === "needsHumanCheck"}
              class:bad={source.status === "unavailable"}
              class:zero={source.status === "ok" && !source.pending && source.count === 0}
            ></span>
            <b>{source.displayName}</b>
            <span class="state-text">
              {#if source.pending}
                ищем…
              {:else if source.status === "needsHumanCheck"}
                просит проверку
              {:else if source.status === "unavailable"}
                не отвечает
              {:else if source.count === 0}
                ничего
              {:else}
                нашёл {source.count}
              {/if}
            </span>
            {#if !source.pending && (source.status !== "ok" || (source.note ?? "").includes("не загрузилась"))}
              <button
                class="site-retry"
                type="button"
                disabled={retrying !== null || searching}
                onclick={() => void retrySource(source)}
              >{retrying === source.id ? "…" : "Повторить"}</button>
            {/if}
          </div>
          {#if source.note && !source.pending}
            <p class="site-note">{source.note}</p>
          {/if}
          {#if source.id === "modland" && source.status === "needsHumanCheck"}
            <button class="btn-glass pass" type="button" disabled={passing} onclick={() => void openModland()}>
              {passing ? "Проходим проверку…" : "Пройти проверку Modland"}
            </button>
          {/if}
        {/each}
      </div>
    {:else}
      <p class="sub">Modland, STMods и ModsGaming — сразу, одним запросом.</p>
    {/if}
    {#if !searching && sources.some((source) => !source.pending && (source.status !== "ok" || (source.note ?? "").includes("не загрузилась")))}
      <button class="btn-glass pass" type="button" onclick={retryAll}>
        Повторить все сайты
      </button>
    {/if}
    <p class="note">
      Платные моды не скачиваются: вместо «Скачать» — «Купить у автора».
    </p>
    <p class="note">
      Репозиторий BeamNG используется для проверки обновлений, а не для поиска.
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
    flex-direction: column;
    gap: 9px;
    padding: 14px 150px 12px 24px;
    border-bottom: 1px solid var(--border);
  }

  .headline {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px 20px;
  }

  .title {
    flex: 0 0 auto;
  }

  h1 {
    margin: 0;
    font-size: 26px;
    line-height: 1.05;
  }

  .title p {
    margin: 2px 0 0;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .bar {
    position: relative;
    display: flex;
    flex: 1 1 420px;
    align-items: center;
    gap: 10px;
    max-width: 880px;
    height: 44px;
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
    height: 32px;
  }

  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 12px;
  }

  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .tools {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-left: auto;
  }

  .chip.reset {
    margin-left: auto;
  }

  .chip.leave {
    color: var(--muted-foreground);
  }

  .tools select,
  .filter-row select {
    height: 30px;
    max-width: 190px;
    padding: 0 28px 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--foreground);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
  }

  .suggest {
    position: absolute;
    top: calc(100% + 6px);
    left: 0;
    right: 0;
    z-index: 40;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface);
    box-shadow: 0 12px 30px color-mix(in oklab, var(--background) 60%, transparent);
  }

  .suggest button {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 12px;
    border: 0;
    background: transparent;
    color: var(--foreground);
    font: inherit;
    font-size: 13.5px;
    text-align: left;
    cursor: default;
  }

  .suggest button small {
    flex: none;
    color: var(--faint-foreground);
    font-size: 11px;
    font-weight: 400;
  }

  .suggest button:hover,
  .suggest button.active {
    background: var(--surface-3);
  }

  .filters-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
    animation: fade-in var(--dur-2) var(--ease-out) both;
  }

  .filter-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }

  .filter-label {
    min-width: 78px;
    color: var(--faint-foreground);
    font-size: 12px;
    font-weight: 600;
  }

  .active-filters {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    color: var(--faint-foreground);
    font-size: 12px;
  }

  .active-filters .chip {
    height: 26px;
    padding: 0 9px;
    font-size: 12px;
  }

  .clear-filters {
    border: 0;
    background: transparent;
    color: var(--accent-brand-text);
    font: inherit;
    font-size: 12px;
    font-weight: 700;
    cursor: default;
  }

  .clear-filters:hover {
    text-decoration: underline;
  }

  .more {
    display: flex;
    justify-content: center;
    padding: 18px 0 4px;
    color: var(--faint-foreground);
    font-size: 12.5px;
  }

  .start-hint {
    color: var(--faint-foreground);
    font-size: 12px;
    text-align: center;
  }

  .meta .author {
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    text-decoration: underline;
    text-decoration-color: var(--border-strong);
    text-underline-offset: 2px;
    cursor: default;
  }

  .meta .author:hover {
    color: var(--foreground);
    text-decoration-color: currentColor;
  }

  .maybe small {
    display: block;
    margin-top: 3px;
    color: var(--faint-foreground);
    font-size: 11.5px;
    font-weight: 400;
  }

  .history {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 6px;
    max-width: 560px;
    color: var(--faint-foreground);
    font-size: 12px;
  }

  .history button {
    padding: 3px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    font: inherit;
    cursor: default;
  }

  .history button:hover {
    color: var(--foreground);
    border-color: var(--border-strong);
  }

  .history .history-clear {
    border-color: transparent;
    color: var(--faint-foreground);
  }

  .site-retry {
    margin-left: auto;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 11px;
    cursor: default;
  }

  .site-retry:hover {
    color: var(--foreground);
    border-color: var(--border-strong);
  }

  .site-note {
    margin: -2px 0 4px 22px;
    color: var(--faint-foreground);
    font-size: 11.5px;
    line-height: 1.35;
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
    grid-template-columns: minmax(0, 1fr) 340px;
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

  /* Узкая карточка: кнопки уходят под название, а не сжимают его до «BM…». */
  .listing {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }

  .listing-main {
    display: flex;
    flex: 1 1 150px;
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
    margin-left: auto;
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

  .start {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 12px;
    min-height: 420px;
    padding: 40px 24px;
    color: var(--muted-foreground);
    text-align: center;
  }

  .start-icon {
    display: grid;
    place-items: center;
    width: 58px;
    height: 58px;
    margin-bottom: 2px;
    border-radius: 18px;
    background: var(--accent-brand-soft);
    color: var(--accent-brand-text);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--accent-brand) 24%, transparent);
  }

  .start-title {
    margin: 0;
    color: var(--foreground);
    font-family: var(--font-display);
    font-size: 28px;
    font-weight: 600;
    letter-spacing: 0.03em;
    text-transform: uppercase;
  }

  .start-text {
    max-width: 480px;
    font-size: 13.5px;
    line-height: 1.55;
  }

  .examples {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 6px;
    margin-top: 4px;
  }

  .examples > span {
    color: var(--faint-foreground);
    font-size: 12.5px;
  }

  .example {
    height: 30px;
    padding: 0 11px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    color: var(--muted-foreground);
    font-size: 12.5px;
    font-weight: 600;
    cursor: default;
    transition:
      transform var(--dur-1) var(--ease-out),
      border-color var(--dur-2) var(--ease-out),
      background-color var(--dur-2) var(--ease-out),
      color var(--dur-2) var(--ease-out);
  }

  .example:hover {
    transform: translateY(-1px);
    border-color: var(--border-strong);
    background: var(--surface-2);
    color: var(--foreground);
  }

  :global(.no-motion) .example {
    transition: none;
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
    font-size: 22px;
  }

  .state .btn-glass {
    margin-top: 6px;
    text-decoration: none;
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
    overflow: hidden;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
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
    background: var(--warning);
  }

  .dot.bad {
    background: var(--danger);
  }

  .dot.zero {
    background: var(--faint-foreground);
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

  /* Узкое окно: сайты уходят под результаты, и прокручивается вся страница,
     а не два независимых списка в половину высоты. */
  @media (max-width: 1100px) {
    .body {
      display: block;
      overflow-y: auto;
    }

    .results,
    .panel {
      overflow: visible;
    }

    .panel {
      border-left: 0;
      border-top: 1px solid var(--border);
    }
  }
</style>

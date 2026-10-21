<script lang="ts">
  import { onMount } from "svelte";
  import { RefreshCw } from "@lucide/svelte";
  import {
    libraryLoad,
    modDetails,
    modToggle,
    type LibrarySnapshot,
    type ModCardDto,
    type ModDetails,
  } from "$lib/ipc/library";
  import Inspector from "./Inspector.svelte";
  import ModGrid from "./ModGrid.svelte";
  import { headerNote, matchesQuery } from "./format";

  type Filter = "all" | "enabled" | "updates" | "unchecked";
  type Toast = { message: string; undo?: () => void; timer?: ReturnType<typeof setTimeout> };

  let snapshot = $state<LibrarySnapshot | null>(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  let filter = $state<Filter>("all");
  let query = $state("");
  let selectedId = $state<string | null>(null);
  let details = $state<ModDetails | null>(null);
  let detailsLoading = $state(false);
  let toast = $state<Toast | null>(null);
  let renderedCount = $state(0);
  let searchEl = $state<HTMLInputElement | null>(null);
  let detailsToken = 0;

  const filters: { id: Filter; label: string }[] = [
    { id: "all", label: "Все" },
    { id: "enabled", label: "Включённые" },
    { id: "updates", label: "Нужно обновить" },
    { id: "unchecked", label: "Не проверяются" },
  ];

  let counts = $derived(
    snapshot?.counts ?? { all: 0, enabled: 0, updates: 0, unchecked: 0, deleted: 0 },
  );
  let filtered = $derived.by(() => {
    if (!snapshot) return [];
    return snapshot.mods.filter((mod) => {
      if (filter === "enabled" && !mod.enabled) return false;
      if (filter === "updates" && mod.update === "none") return false;
      if (filter === "unchecked") return false;
      return matchesQuery(mod, query);
    });
  });
  let selected = $derived(snapshot?.mods.find((mod) => mod.id === selectedId) ?? null);
  let note = $derived(
    snapshot ? headerNote(snapshot.mods.length, snapshot.counts.updates) : "загружаем…",
  );

  onMount(() => {
    void load();
  });

  $effect(() => {
    if (selectedId && !filtered.some((mod) => mod.id === selectedId)) {
      selectedId = null;
    }
  });

  $effect(() => {
    if (!selectedId) {
      details = null;
      detailsLoading = false;
      return;
    }
    const token = ++detailsToken;
    detailsLoading = true;
    void modDetails(selectedId)
      .then((value) => {
        if (token === detailsToken) details = value;
      })
      .catch((reason: unknown) => {
        if (token === detailsToken) error = errorText(reason);
      })
      .finally(() => {
        if (token === detailsToken) detailsLoading = false;
      });
  });

  async function load(force = false) {
    loading = !force;
    refreshing = force;
    error = null;
    try {
      snapshot = await libraryLoad();
      if (selectedId && !snapshot.mods.some((mod) => mod.id === selectedId)) {
        selectedId = null;
      }
    } catch (reason) {
      error = errorText(reason);
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  function select(id: string) {
    selectedId = selectedId === id ? null : id;
  }

  async function toggle(mod: ModCardDto) {
    await applyToggle(mod, !mod.enabled, true);
  }

  async function applyToggle(mod: ModCardDto, target: boolean, announce: boolean) {
    try {
      const result = await modToggle(mod.id, target);
      if (result.outcome === "error" || result.outcome === "failed") {
        showToast(result.message ?? "Не удалось изменить мод");
        return;
      }
      mod.enabled = target;
      mod.pending = result.outcome === "queued" ? "queued" : "none";
      if (snapshot) {
        snapshot.counts.enabled = snapshot.mods.filter((item) => item.enabled).length;
      }
      if (details?.id === mod.id) {
        details = { ...details, enabled: target };
      }
      if (announce) {
        const message = target ? `Включён «${mod.name}»` : `Выключен «${mod.name}»`;
        showToast(
          result.message ?? message,
          result.outcome === "queued"
            ? undefined
            : () => {
                void applyToggle(mod, !target, false);
              },
        );
      }
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  function showToast(message: string, undo?: () => void) {
    if (toast?.timer) clearTimeout(toast.timer);
    const timer = setTimeout(() => {
      toast = null;
    }, undo ? 6000 : 3000);
    toast = { message, undo, timer };
  }

  function hideToast() {
    if (toast?.timer) clearTimeout(toast.timer);
    toast = null;
  }

  function isTyping(event: KeyboardEvent): boolean {
    const target = event.target as HTMLElement | null;
    if (!target) return false;
    return ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName) || target.isContentEditable;
  }

  function onkeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "f") {
      event.preventDefault();
      searchEl?.focus();
      searchEl?.select();
      return;
    }
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
      event.preventDefault();
      toast?.undo?.();
      hideToast();
      return;
    }
    if (event.key === "Escape") {
      if (query) {
        query = "";
      } else {
        selectedId = null;
      }
      return;
    }
    if (isTyping(event)) return;
    if (event.key === " " && selected) {
      event.preventDefault();
      void toggle(selected);
      return;
    }
    if (selected && ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) {
      event.preventDefault();
      const list = filtered;
      const index = list.findIndex((mod) => mod.id === selected.id);
      if (index < 0) return;
      const step =
        event.key === "ArrowLeft"
          ? -1
          : event.key === "ArrowRight"
            ? 1
            : event.key === "ArrowUp"
              ? -4
              : 4;
      const next = list[Math.max(0, Math.min(list.length - 1, index + step))];
      if (next) selectedId = next.id;
    }
  }

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось выполнить действие.";
  }

  function emptyTitle(): string {
    if (snapshot?.folderError) return "Не нашли папку с модами";
    if (query) return "Нет такого мода";
    if (filter === "updates") return "Всё свежее";
    if (filter === "unchecked") return "Все моды проверяются";
    if (filter === "enabled") return "Все моды выключены";
    return "Модов пока нет";
  }

  function emptyText(): string {
    if (snapshot?.folderError) {
      return snapshot.folderError;
    }
    if (query) return "Проверьте написание. Ищем только среди ваших модов.";
    if (filter === "updates") return "Проверка обновлений появится на следующем этапе.";
    if (filter === "unchecked") return "У всех модов есть страница.";
    if (filter === "enabled") return "Включите нужные в общем списке.";
    return "Найдите мод в поиске или перетащите архив на окно.";
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="garage">
  <header class="head">
    <div>
      <h1>Моды</h1>
      <p>{note}{snapshot?.gameVersion ? ` · игра ${snapshot.gameVersion}` : ""}</p>
    </div>
    <div class="head-actions">
      <button
        class="refresh glass liquid-edge"
        type="button"
        onclick={() => void load(true)}
        disabled={refreshing}
        aria-label="Обновить библиотеку"
      >
        <RefreshCw size={14} class={refreshing ? "spin" : ""} />
        Обновить
      </button>
    </div>
  </header>

  {#if snapshot?.warning}
    <div class="banner">{snapshot.warning}</div>
  {/if}

  <div class="body" class:with-inspector={selectedId !== null}>
    <section class="work">
      <div class="island glass liquid-edge">
        <div class="search">
          <input
            bind:this={searchEl}
            bind:value={query}
            id="garage-search"
            name="garage-search"
            type="search"
            placeholder="Найти мод"
            aria-label="Найти мод"
          />
          {#if query}
            <button class="clear" type="button" aria-label="Очистить" onclick={() => (query = "")}>×</button>
          {/if}
        </div>

        <div class="filters">
          {#each filters as item (item.id)}
            <button
              class="filter"
              class:active={filter === item.id}
              type="button"
              aria-pressed={filter === item.id}
              onclick={() => (filter = item.id)}
            >
              {item.label}
              {#if item.id === "all"}
                <span>{counts.all.toLocaleString("ru-RU")}</span>
              {:else if item.id === "enabled" && counts.enabled > 0}
                <span>{counts.enabled.toLocaleString("ru-RU")}</span>
              {:else if item.id === "updates" && counts.updates > 0}
                <span>{counts.updates.toLocaleString("ru-RU")}</span>
              {:else if item.id === "unchecked" && counts.unchecked > 0}
                <span>{counts.unchecked.toLocaleString("ru-RU")}</span>
              {/if}
            </button>
          {/each}
        </div>
      </div>

      <div class="grid-wrap">
        {#if loading}
          <div class="state"><p>Загружаем моды…</p></div>
        {:else if error}
          <div class="state error">
            <p>Не удалось загрузить моды</p>
            <span>{error}</span>
            <button type="button" onclick={() => void load(true)}>Попробовать снова</button>
          </div>
        {:else if snapshot && (snapshot.folderError || snapshot.mods.length === 0 || filtered.length === 0)}
          <div class="state">
            <p>{emptyTitle()}</p>
            <span>{emptyText()}</span>
            {#if snapshot.folderError}
              <a href="/settings">Открыть настройки</a>
            {:else if query}
              <button type="button" onclick={() => (query = "")}>Сбросить поиск</button>
            {:else if filter !== "all"}
              <button type="button" onclick={() => (filter = "all")}>Показать все</button>
            {/if}
          </div>
        {:else if snapshot}
          <ModGrid
            items={filtered}
            {selectedId}
            onselect={select}
            ontoggle={(mod) => void toggle(mod)}
            resetKey={`${filter}|${query}`}
            bind:renderedCount
          />
        {/if}
      </div>
    </section>

    {#if selectedId !== null}
      <Inspector
        {details}
        loading={detailsLoading}
        onToggle={() => {
          if (selected) void toggle(selected);
        }}
        onClose={() => (selectedId = null)}
      />
    {/if}
  </div>

  {#if toast}
    <div class="toast glass liquid-edge" role="status">
      <span>{toast.message}</span>
      {#if toast.undo}
        <button type="button" onclick={() => { toast?.undo?.(); hideToast(); }}>Отменить</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .garage {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 12px;
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

  .head-actions {
    display: flex;
    gap: 8px;
  }

  .refresh {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    color: var(--foreground);
    font-size: 10.5px;
    cursor: default;
  }

  .refresh:disabled {
    opacity: 0.6;
  }

  .banner {
    padding: 8px 12px;
    border: 1px solid color-mix(in oklab, oklch(0.82 0.15 85) 35%, transparent);
    border-radius: 12px;
    background: color-mix(in oklab, oklch(0.82 0.15 85) 10%, transparent);
    color: oklch(0.85 0.12 85);
    font-size: 10.5px;
  }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 12px;
    flex: 1;
    min-height: 0;
  }

  .body.with-inspector {
    grid-template-columns: minmax(0, 1fr) 320px;
  }

  .work {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
    min-height: 0;
  }

  .island {
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: 760px;
    margin: 0 auto;
    padding: 8px;
    border-radius: 18px;
  }

  .search {
    position: relative;
    flex: 1;
    min-width: 150px;
  }

  .search input {
    width: 100%;
    height: 30px;
    padding: 0 28px 0 12px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 7%, transparent);
    color: var(--foreground);
    font-size: 11px;
  }

  .search input::placeholder {
    color: var(--muted-foreground);
  }

  .search input:focus-visible {
    outline: 2px solid var(--ring);
  }

  .clear {
    position: absolute;
    top: 50%;
    right: 7px;
    width: 20px;
    height: 20px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 15px;
    transform: translateY(-50%);
    cursor: default;
  }

  .filters {
    display: flex;
    gap: 2px;
    overflow-x: auto;
  }

  .filter {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 30px;
    padding: 0 10px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 10.5px;
    white-space: nowrap;
    cursor: default;
  }

  .filter span {
    color: inherit;
    opacity: 0.65;
    font-variant-numeric: tabular-nums;
  }

  .filter:hover,
  .filter.active {
    background: color-mix(in oklab, var(--foreground) 9%, transparent);
    color: var(--foreground);
  }

  .filter.active {
    font-weight: 600;
  }

  .grid-wrap {
    flex: 1;
    min-height: 0;
  }

  .state {
    display: grid;
    place-content: center;
    gap: 6px;
    min-height: 280px;
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
    max-width: 420px;
    font-size: 11px;
    line-height: 1.45;
  }

  .state button,
  .state a {
    justify-self: center;
    margin-top: 4px;
    padding: 6px 12px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    color: var(--foreground);
    font-size: 10.5px;
    text-decoration: none;
    cursor: default;
  }

  .state.error p {
    color: oklch(0.85 0.12 25);
  }

  .toast {
    position: absolute;
    left: 50%;
    bottom: 14px;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 10px 14px;
    border-radius: 999px;
    color: var(--foreground);
    font-size: 11px;
    transform: translateX(-50%);
    z-index: 20;
  }

  .toast button {
    border: 0;
    background: transparent;
    color: var(--primary);
    font-size: 10.5px;
    font-weight: 600;
    cursor: default;
  }

  @media (max-width: 900px) {
    .body.with-inspector {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @media (max-width: 900px) {
    .island {
      align-items: stretch;
      flex-direction: column;
      gap: 6px;
    }
  }
</style>

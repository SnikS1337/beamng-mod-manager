<script lang="ts">
  import { groupMembers, isShown, visibleCards } from "./duplicates";
  import { onMount } from "svelte";
  import {
    listenUpdates,
    updatesOpenModland,
    updatesRefollow,
    updatesUnfollow,
  } from "$lib/ipc/updates";
  import { isTauri } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { RefreshCw, Upload } from "@lucide/svelte";
  import {
    libraryLoad,
    listenLibraryTitles,
    modBindPage,
    modFixPackaging,
    modDelete,
    modDetails,
    modForgetDeleted,
    modRestoreDeleted,
    modRestoreVersion,
    modToggle,
    modVersions,
    modUndelete,
    type LibrarySnapshot,
    type ModCardDto,
    type ModDetails,
    type VersionRow,
  } from "$lib/ipc/library";
  import { isArchivePath } from "$lib/ipc/incoming";
  import { incomingQueue } from "$lib/features/incoming/queue.svelte";
  import { toasts } from "$lib/shell/toasts.svelte";
  import { updatesBadge } from "$lib/shell/updatesBadge.svelte";
  import DeletedInspector from "./DeletedInspector.svelte";
  import Inspector from "./Inspector.svelte";
  import ModGrid from "./ModGrid.svelte";
  import { headerNote, matchesQuery } from "./format";

  type Filter = "all" | "enabled" | "updates" | "unchecked" | "deleted";

  let snapshot = $state<LibrarySnapshot | null>(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  let filter = $state<Filter>("all");
  let query = $state("");
  let selectedId = $state<string | null>(null);
  let details = $state<ModDetails | null>(null);
  let detailsLoading = $state(false);
  let renderedCount = $state(0);
  let bannerDismissed = $state(false);
  let searchEl = $state<HTMLInputElement | null>(null);
  let archiveInput = $state<HTMLInputElement | null>(null);
  let detailsToken = 0;

  const baseFilters: { id: Filter; label: string }[] = [
    { id: "all", label: "Все" },
    { id: "enabled", label: "Включённые" },
    { id: "updates", label: "Нужно обновить" },
    { id: "unchecked", label: "Не проверяются" },
  ];

  let counts = $derived(
    snapshot?.counts ?? { all: 0, enabled: 0, updates: 0, unchecked: 0, deleted: 0 },
  );
  let visibleFilters = $derived(
    counts.deleted > 0
      ? [...baseFilters, { id: "deleted" as const, label: "Удалённые" }]
      : baseFilters,
  );
  let allCards = $derived(
    snapshot ? [...snapshot.mods, ...snapshot.deleted] : [],
  );
  let filtered = $derived.by(() => {
    if (!snapshot) return [];
    // Копии прячутся за карточкой основного мода группы — их видно в его панели.
    const source = filter === "deleted" ? snapshot.deleted : visibleCards(snapshot.mods);
    return source.filter((mod) => {
      if (filter === "enabled" && !mod.enabled) return false;
      if (filter === "updates" && mod.update === "none") return false;
      if (filter === "unchecked" && !mod.unchecked) return false;
      return matchesQuery(mod, query);
    });
  });
  let selected = $derived(allCards.find((mod) => mod.id === selectedId) ?? null);
  let note = $derived(
    snapshot ? headerNote(snapshot.mods.length, snapshot.counts.updates) : "загружаем…",
  );

  onMount(() => {
    void load();
    const onLibraryChanged = (event: Event) => {
      const detail = (event as CustomEvent<{ fileNames?: string[] }>).detail;
      void refreshAndSelect(detail?.fileNames ?? []);
    };
    window.addEventListener("bmm:library-changed", onLibraryChanged);
    // Проверка обновлений закончилась (в фоне или в разделе) — метки «Есть обновление» и
    // счётчик «Нужно обновить» должны появиться без перезапуска.
    let stopUpdates: (() => void) | undefined;
    let disposed = false;
    void listenUpdates((event) => {
      if (event.phase === "auto" || event.phase === "done" || event.phase === "pages") {
        void load(true);
      }
    }).then((stop) => {
      if (disposed) stop();
      else stopUpdates = stop;
    });
    let stopTitles: (() => void) | undefined;
    void listenLibraryTitles(() => void load(true)).then((stop) => {
      if (disposed) stop();
      else stopTitles = stop;
    });
    return () => {
      disposed = true;
      stopUpdates?.();
      stopTitles?.();
      window.removeEventListener("bmm:library-changed", onLibraryChanged);
    };
  });

  $effect(() => {
    if (selectedId && !isShown(selectedId, filtered)) {
      selectedId = null;
    }
  });

  $effect(() => {
    if (filter === "deleted" && counts.deleted === 0) {
      filter = "all";
    }
  });

  $effect(() => {
    if (!selectedId) {
      details = null;
      detailsLoading = false;
      return;
    }
    const card = allCards.find((mod) => mod.id === selectedId);
    if (!card || card.deleted) {
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
      if (
        selectedId &&
        !snapshot.mods.some((mod) => mod.id === selectedId) &&
        !snapshot.deleted.some((mod) => mod.id === selectedId)
      ) {
        selectedId = null;
      }
    } catch (reason) {
      error = errorText(reason);
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  let fixing = $state(false);

  async function fixSelected() {
    if (!selected || fixing) return;
    fixing = true;
    try {
      await modFixPackaging(selected.id);
      updatesBadge.refresh();
      showToast("Архив переупакован: игра теперь его загрузит");
      await load(true);
    } catch (reason) {
      showToast(errorText(reason));
    } finally {
      fixing = false;
    }
  }

  /** «Это другой мод» из панели мода: программа следила не за той страницей. */
  async function unfollowSelected(source: string) {
    const card = selected;
    if (!card) return;
    try {
      await updatesUnfollow(card.id, source);
      updatesBadge.refresh();
      await load(true);
      showToast(`Больше не следим за этой страницей «${trimName(card.name)}»`, {
        undo: async () => {
          await updatesRefollow(card.id, source);
          updatesBadge.refresh();
          await load(true);
        },
      });
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  /** «Указать страницу» из панели мода. Возвращает причину отказа для поля ввода. */
  async function bindSelected(url: string): Promise<string | null> {
    const card = selected;
    if (!card) return "Мод не выбран.";
    try {
      await modBindPage(card.id, url);
      updatesBadge.refresh();
      await load(true);
      showToast(`Следим за страницей «${trimName(card.name)}»`);
      return null;
    } catch (reason) {
      return errorText(reason);
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
        const message = target ? `Включён «${trimName(mod.name)}»` : `Выключен «${trimName(mod.name)}»`;
        showToast(result.message ?? message, {
          undo:
            result.outcome === "queued"
              ? undefined
              : () => {
                  void applyToggle(mod, !target, false);
                },
        });
      }
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  async function deleteSelected() {
    if (selected) await deleteCard(selected);
  }

  /** «Оставить включённой только эту» в группе копий: остальные выключаются. */
  async function keepOnly(card: ModCardDto) {
    const main = snapshot?.mods.find((item) => item.id === card.id || item.group.includes(card.id));
    if (!main || !snapshot) return;
    const members = groupMembers(main, snapshot.mods);
    for (const member of members) {
      const target = member.id === card.id;
      if (member.enabled !== target) await applyToggle(member, target, false);
    }
    await load(true);
    showToast(`Включена только «${trimName(card.name)}» (${card.fileName})`);
  }

  async function deleteCard(card: ModCardDto) {
    if (card.deleted || card.fromGameRepo) return;
    try {
      // Удаляют копию из панели группы — панель основного остаётся открытой.
      if (selectedId === card.id) selectedId = null;
      const ticket = await modDelete(card.id);
      await load(true);
      showToast(`Удалён «${trimName(ticket.name)}»`, {
        undo: async () => {
          await modUndelete(card.id);
          await load(true);
        },
        commit: async () => {
          await modForgetDeleted(card.id);
          await load(true);
        },
      });
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  async function restoreDeleted(card: ModCardDto) {
    try {
      await modRestoreDeleted(card.fileName);
      selectedId = null;
      await load(true);
      showToast(`Возвращён «${trimName(card.name)}»`);
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  async function restoreVersion(version: VersionRow) {
    const card = selected;
    if (!card || card.deleted) return;
    const previousFile = card.fileName;
    try {
      await modRestoreVersion(card.id, version.backupId);
      await load(true);
      showToast(
        version.version
          ? `Возвращена версия ${version.version}`
          : "Возвращена прошлая копия",
        {
          undo: async () => {
            const rows = await modVersions(card.id);
            const previous = rows.find((row) => row.fileName === previousFile);
            if (previous) {
              await modRestoreVersion(card.id, previous.backupId);
              await load(true);
            }
          },
        },
      );
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  function showToast(
    message: string,
    options: {
      undo?: () => void | Promise<void>;
      commit?: () => void | Promise<void>;
      duration?: number;
    } = {},
  ) {
    toasts.show(message, options);
  }

  async function undoToast() {
    await toasts.undo();
  }

  async function refreshAndSelect(fileNames: string[]) {
    await load(true);
    if (!snapshot) return;
    const last = fileNames.at(-1);
    if (!last) return;
    const card = snapshot.mods.find(
      (mod) => mod.fileName.toLowerCase() === last.toLowerCase(),
    );
    if (!card) return;
    filter = "all";
    query = "";
    selectedId = card.id;
  }

  async function pickArchive() {
    if (!isTauri()) {
      archiveInput?.click();
      return;
    }
    try {
      const picked = await open({
        multiple: true,
        directory: false,
        title: "Установить моды из архива",
        filters: [{ name: "Архивы модов", extensions: ["zip", "rar", "7z"] }],
      });
      const paths = (Array.isArray(picked) ? picked : picked ? [picked] : [])
        .filter(isArchivePath)
        .slice(0, 8);
      if (paths.length > 0) incomingQueue.add(paths);
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  function onArchiveInput(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const names = Array.from(input.files ?? [])
      .map((file) => file.name)
      .filter(isArchivePath)
      .slice(0, 8);
    input.value = "";
    if (names.length > 0) incomingQueue.add(names);
  }

  function trimName(name: string): string {
    return name.length > 34 ? `${name.slice(0, 33)}…` : name;
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
      void undoToast();
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
    if (event.key === "Delete" && selected && !selected.deleted && !selected.fromGameRepo) {
      event.preventDefault();
      void deleteSelected();
      return;
    }
    if (event.key === " " && selected && !selected.deleted) {
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
    if (filter === "deleted") return "Удалённых модов нет";
    return "Модов пока нет";
  }

  function emptyText(): string {
    if (snapshot?.folderError) {
      return snapshot.folderError;
    }
    if (query) return "Проверьте написание. Ищем только среди ваших модов.";
    if (filter === "updates") return "Обновлений нет. Проверить снова можно на экране «Обновления».";
    if (filter === "unchecked") return "У всех модов есть подтверждённая страница.";
    if (filter === "enabled") return "Включите нужные в общем списке.";
    if (filter === "deleted") return "Удалённые моды появятся здесь после подтверждения.";
    return "Найдите мод в поиске или перетащите архив на окно.";
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="garage">
  <header class="top" data-tauri-drag-region>
    <div class="title" data-tauri-drag-region>
      <h1 data-tauri-drag-region>Моды</h1>
      <p data-tauri-drag-region>
        {note}{snapshot?.gameVersion ? ` · игра ${snapshot.gameVersion}` : ""}
      </p>
    </div>

    <div class="island glass liquid-edge">
      <div class="filters">
        {#each visibleFilters as item (item.id)}
          <button
            class="filter"
            class:active={filter === item.id}
            type="button"
            aria-pressed={filter === item.id}
            onclick={() => (filter = item.id)}
          >
            {item.label}
            {#if item.id === "enabled" && counts.enabled > 0}
              <span>{counts.enabled.toLocaleString("ru-RU")}</span>
            {:else if item.id === "updates" && counts.updates > 0}
              <span>{counts.updates.toLocaleString("ru-RU")}</span>
            {:else if item.id === "unchecked" && counts.unchecked > 0}
              <span>{counts.unchecked.toLocaleString("ru-RU")}</span>
            {:else if item.id === "deleted"}
              <span>{counts.deleted.toLocaleString("ru-RU")}</span>
            {/if}
          </button>
        {/each}
      </div>
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
    </div>

    <div class="actions">
      <button
        class="btn-glass"
        type="button"
        onclick={() => void pickArchive()}
        disabled={refreshing}
        title="Установить моды из архива"
      >
        <Upload size={15} />
        Из архива
      </button>
      <button
        class="btn-glass icon-only"
        type="button"
        onclick={() => void load(true)}
        disabled={refreshing}
        aria-label="Перечитать папку модов"
        title="Перечитать папку модов"
      >
        <RefreshCw size={15} class={refreshing ? "spin" : ""} />
      </button>
    </div>
  </header>

  {#if snapshot?.warning}
    <div class="banner">{snapshot.warning}</div>
  {/if}
  {#if snapshot?.manyEnabled && !bannerDismissed}
    <div class="banner many">
      <span>{snapshot.manyEnabled}</span>
      <button type="button" aria-label="Скрыть предупреждение" onclick={() => (bannerDismissed = true)}>×</button>
    </div>
  {/if}

  <div class="body" class:with-inspector={selectedId !== null}>
    <section class="work">
      <div class="grid-wrap">
        {#if loading}
          <div class="state"><p>Загружаем моды…</p></div>
        {:else if error}
          <div class="state error">
            <p>Не удалось загрузить моды</p>
            <span>{error}</span>
            <button type="button" onclick={() => void load(true)}>Попробовать снова</button>
          </div>
        {:else if snapshot && (snapshot.folderError || (snapshot.mods.length === 0 && snapshot.deleted.length === 0) || filtered.length === 0)}
          <div class="state">
            <p>{emptyTitle()}</p>
            <span>{emptyText()}</span>
            {#if snapshot.folderError}
              <a href="/settings?section=folders">Выбрать папку</a>
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
            ontoggle={(mod) => {
              if (!mod.deleted) void toggle(mod);
            }}
            resetKey={`${filter}|${query}`}
            bind:renderedCount
          />
        {/if}
      </div>
    </section>

    {#if selected?.deleted}
      <DeletedInspector
        card={selected}
        onRestore={(card) => void restoreDeleted(card)}
        onClose={() => (selectedId = null)}
      />
    {:else if selectedId !== null}
      <Inspector
        {details}
        loading={detailsLoading}
        canDelete={selected ? !selected.fromGameRepo : false}
        problem={selected?.problem ?? null}
        duplicateOf={selected?.duplicateOf ?? null}
        addon={selected?.role === "addon"}
        group={selected && snapshot && selected.group.length > 0 ? groupMembers(selected, snapshot.mods) : []}
        onShowMod={(id) => (selectedId = id)}
        onKeepOnly={(card) => void keepOnly(card)}
        onDeleteCard={(card) => void deleteCard(card)}
        update={selected?.update ?? "none"}
        updateVersion={selected?.updateVersion ?? null}
        {fixing}
        onFix={() => void fixSelected()}
        onUnfollow={(source) => void unfollowSelected(source)}
        onBindPage={bindSelected}
        onPassCheck={updatesOpenModland}
        onToggle={() => {
          if (selected) void toggle(selected);
        }}
        onDelete={() => void deleteSelected()}
        onRestoreVersion={(version) => void restoreVersion(version)}
        onClose={() => (selectedId = null)}
      />
    {/if}
  </div>

  <input
    class="archive-input"
    type="file"
    accept=".zip,.rar,.7z"
    multiple
    bind:this={archiveInput}
    onchange={onArchiveInput}
    tabindex="-1"
    aria-hidden="true"
  />
</div>

<style>
  .archive-input {
    display: none;
  }

  .garage {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    gap: 14px;
  }

  /* Ряд «островков» (legacy, DESIGN.md 4): заголовок на фоне, островок фильтров по центру,
     справа место под капсулу кнопок окна. */
  .top {
    display: grid;
    grid-template-columns: minmax(170px, 1fr) auto minmax(0, auto);
    align-items: center;
    gap: 16px;
    min-height: 52px;
    padding-right: 148px;
  }

  h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 700;
    line-height: 1.1;
    letter-spacing: -0.02em;
  }

  .title {
    min-width: 0;
  }

  .title p {
    margin: 3px 0 0;
    overflow: hidden;
    color: color-mix(in oklab, var(--foreground) 66%, transparent);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .icon-only {
    width: 36px;
    padding: 0;
  }

  .banner {
    padding: 9px 14px;
    border: 1px solid var(--warning-border);
    border-radius: 16px;
    background: var(--warning-soft);
    color: var(--warning);
    font-size: 13px;
  }

  .banner.many {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .banner.many button {
    border: 0;
    background: transparent;
    color: inherit;
    font-size: 17px;
    line-height: 1;
    cursor: default;
  }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 16px;
    flex: 1;
    min-height: 0;
  }

  .body.with-inspector {
    grid-template-columns: minmax(0, 1fr) 328px;
  }

  .work {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .island {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 52px;
    padding: 0 8px;
    border-radius: 26px;
  }

  .search {
    position: relative;
    width: 160px;
  }

  .search input {
    width: 100%;
    height: 35px;
    padding: 0 30px 0 14px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
    color: var(--foreground);
    font-size: 13px;
  }

  .search input::placeholder {
    color: color-mix(in oklab, var(--foreground) 55%, transparent);
  }

  .search input:focus-visible {
    outline: 2px solid var(--ring);
  }

  .clear {
    position: absolute;
    top: 50%;
    right: 8px;
    width: 22px;
    height: 22px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 17px;
    transform: translateY(-50%);
    cursor: default;
  }

  .filters {
    display: flex;
    gap: 2px;
    height: 35px;
    padding: 3px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 6%, transparent);
    overflow-x: auto;
    scrollbar-width: none;
  }

  .filter {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 29px;
    padding: 0 13px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: color-mix(in oklab, var(--foreground) 74%, transparent);
    font-size: 13px;
    font-weight: 550;
    white-space: nowrap;
    cursor: default;
    transition:
      background 220ms ease,
      color 220ms ease;
  }

  .filter span {
    color: inherit;
    opacity: 0.6;
    font-variant-numeric: tabular-nums;
  }

  .filter:hover {
    color: var(--foreground);
  }

  .filter.active {
    background: var(--drop-fill);
    color: var(--foreground);
    font-weight: 650;
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      inset 0 0 0 1px var(--glass-border),
      0 4px 14px var(--drop-glow);
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

  @media (max-width: 900px) {
    .body.with-inspector {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  @media (max-width: 1320px) {
    .top {
      grid-template-columns: minmax(0, 1fr) auto;
      padding-right: 148px;
    }

    .island {
      grid-column: 1 / -1;
      grid-row: 2;
      justify-self: center;
      max-width: 100%;
    }
  }
</style>

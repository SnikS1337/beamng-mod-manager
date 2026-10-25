<script lang="ts">
  import { openExternal } from "#lib/shell/openExternal.js";
  import { isShortcut } from "#lib/shell/keys.js";
  import { groupMembers, isShown, visibleCards } from "./duplicates";
  import { onMount, untrack } from "svelte";
  import {
    listenUpdates,
    updatesOpenModland,
    updatesRefollow,
    updatesUnfollow,
  } from "#lib/ipc/updates.js";
  import { isTauri } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import ArrowsClockwise from "phosphor-svelte/lib/ArrowsClockwise";
  import Plus from "phosphor-svelte/lib/Plus";
  import MagnifyingGlass from "phosphor-svelte/lib/MagnifyingGlass";
  import SquaresFour from "phosphor-svelte/lib/SquaresFour";
  import Rows from "phosphor-svelte/lib/Rows";
  import X from "phosphor-svelte/lib/X";
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
  } from "#lib/ipc/library.js";
  import { isArchivePath } from "#lib/ipc/incoming.js";
  import { incomingQueue } from "#lib/features/incoming/queue.svelte.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import { updatesBadge } from "#lib/shell/updatesBadge.svelte.js";
  import DeletedInspector from "./DeletedInspector.svelte";
  import Inspector from "./Inspector.svelte";
  import ModGrid from "./ModGrid.svelte";
  import ModList from "./ModList.svelte";
  import ModMenu from "./ModMenu.svelte";
  import { groupDigits, headerNote, matchesQuery } from "./format";
  import {
    loadViewMode,
    matchesType,
    saveViewMode,
    typeCounts,
    typeTag,
    type TypeGroup,
    type ViewMode,
  } from "./view";
  import { palette } from "#lib/shell/palette.svelte.js";
  import {
    EMPTY,
    bulkPlan,
    clickSelect,
    keepVisible,
    selectAll,
    selectOnly,
    single,
    type Selection,
  } from "./selection";
  import BulkBar from "./BulkBar.svelte";

  type Filter = "all" | "disabled" | "updates" | "unchecked" | "deleted";

  let snapshot = $state<LibrarySnapshot | null>(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  let filter = $state<Filter>("all");
  let typeFilter = $state<TypeGroup | "all">("all");
  let view = $state<ViewMode>(loadViewMode());
  let query = $state("");
  /** Выбранные моды; ровно один — его панель справа, несколько — панель действий. */
  let picked = $state<Selection>(EMPTY);
  let selectedId = $derived(single(picked));
  let details = $state<ModDetails | null>(null);
  let detailsLoading = $state(false);
  let renderedCount = $state(0);
  let bannerDismissed = $state(false);
  let searchEl = $state<HTMLInputElement | null>(null);
  let archiveInput = $state<HTMLInputElement | null>(null);
  let detailsToken = 0;

  const stateFilters: { id: Exclude<Filter, "all">; label: string }[] = [
    { id: "updates", label: "Есть обновление" },
    { id: "unchecked", label: "Без страницы" },
    { id: "disabled", label: "Выключены" },
  ];
  const typeFilters: { id: TypeGroup; label: string }[] = [
    { id: "vehicle", label: "Машины" },
    { id: "map", label: "Карты" },
    { id: "addon", label: "Дополнения" },
  ];

  let counts = $derived(
    snapshot?.counts ?? { all: 0, enabled: 0, updates: 0, unchecked: 0, deleted: 0 },
  );
  let visibleFilters = $derived(
    counts.deleted > 0
      ? [...stateFilters, { id: "deleted" as const, label: "Удалённые" }]
      : stateFilters,
  );
  let shownMods = $derived(snapshot ? visibleCards(snapshot.mods) : []);
  let kinds = $derived(typeCounts(shownMods));
  let stateCount = (id: Filter): number =>
    id === "disabled"
      ? counts.all - counts.enabled
      : id === "updates"
        ? counts.updates
        : id === "unchecked"
          ? counts.unchecked
          : id === "deleted"
            ? counts.deleted
            : counts.all;
  let allCards = $derived(
    snapshot ? [...snapshot.mods, ...snapshot.deleted] : [],
  );
  let filtered = $derived.by(() => {
    if (!snapshot) return [];
    // Копии прячутся за карточкой основного мода группы — их видно в его панели.
    const source = filter === "deleted" ? snapshot.deleted : visibleCards(snapshot.mods);
    return source.filter((mod) => {
      if (filter === "disabled" && mod.enabled) return false;
      if (filter !== "deleted" && !matchesType(mod, typeFilter)) return false;
      if (filter === "updates" && mod.update === "none") return false;
      if (filter === "unchecked" && !mod.unchecked) return false;
      return matchesQuery(mod, query);
    });
  });
  let selected = $derived(allCards.find((mod) => mod.id === selectedId) ?? null);
  let note = $derived(
    snapshot ? headerNote(shownMods.length, snapshot.counts.updates, snapshot.mods.length) : "загружаем…",
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

  // Смена фильтра или поиска: выбранными остаются только видимые (одиночный — и как копия
  // в группе видимого основного, его панель тогда не закрывается).
  $effect(() => {
    const shown = filtered;
    const current = untrack(() => picked);
    if (current.ids.length === 1) {
      if (!isShown(current.ids[0], shown)) picked = EMPTY;
    } else if (current.ids.length > 1) {
      picked = keepVisible(current, shown.map((mod) => mod.id));
    }
  });

  $effect(() => {
    if (filter === "deleted" && counts.deleted === 0) {
      filter = "all";
    }
  });

  // Палитра ищет по тем же карточкам, что показывает гараж.
  $effect(() => {
    if (snapshot) palette.mods = snapshot.mods;
  });

  // Палитра команд: «Найти мод» и выбор мода по имени.
  $effect(() => {
    const request = palette.garageRequest;
    if (!request) return;
    palette.garageRequest = null;
    if (request.kind === "select") {
      filter = "all";
      typeFilter = "all";
      query = "";
      picked = selectOnly(request.id);
    } else if (request.kind === "filter") {
      filter = request.filter;
    } else if (request.kind === "archive") {
      void pickArchive();
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
        picked = EMPTY;
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

  function select(id: string, keys: { ctrl: boolean; shift: boolean } = { ctrl: false, shift: false }) {
    picked = clickSelect(
      picked,
      id,
      filtered.map((mod) => mod.id),
      keys,
    );
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
      if (picked.ids.includes(card.id)) picked = EMPTY;
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

  // Действия над несколькими выбранными: одним тостом и одной отменой на всех.
  let bulkBusy = $state(false);
  let plan = $derived(bulkPlan(allCards, picked.ids));

  async function bulkToggle(target: boolean) {
    const mods = target ? plan.enable : plan.disable;
    if (bulkBusy || mods.length === 0) return;
    bulkBusy = true;
    const changed: ModCardDto[] = [];
    let failed = 0;
    let queued = 0;
    for (const mod of mods) {
      try {
        const result = await modToggle(mod.id, target);
        if (result.outcome === "error" || result.outcome === "failed") {
          failed += 1;
        } else {
          if (result.outcome === "queued") queued += 1;
          changed.push(mod);
        }
      } catch {
        failed += 1;
      }
    }
    await load(true);
    bulkBusy = false;
    const verb = target ? "Включено" : "Выключено";
    const parts = [`${verb}: ${changed.length}`];
    if (queued > 0) parts.push(`после игры: ${queued}`);
    if (failed > 0) parts.push(`не получилось: ${failed}`);
    showToast(parts.join(" · "), {
      undo:
        changed.length > 0 && queued === 0
          ? async () => {
              for (const mod of changed) await modToggle(mod.id, !target).catch(() => {});
              await load(true);
            }
          : undefined,
    });
  }

  async function bulkDelete() {
    const mods = plan.remove;
    if (bulkBusy || mods.length === 0) return;
    bulkBusy = true;
    const removed: ModCardDto[] = [];
    let failed = 0;
    for (const mod of mods) {
      try {
        await modDelete(mod.id);
        removed.push(mod);
      } catch {
        failed += 1;
      }
    }
    picked = EMPTY;
    await load(true);
    bulkBusy = false;
    const text = `Удалено: ${removed.length}${failed > 0 ? ` · не получилось: ${failed}` : ""}`;
    showToast(text, {
      undo: removed.length
        ? async () => {
            for (const mod of removed) await modUndelete(mod.id).catch(() => {});
            await load(true);
          }
        : undefined,
      commit: removed.length
        ? async () => {
            for (const mod of removed) await modForgetDeleted(mod.id).catch(() => {});
            await load(true);
          }
        : undefined,
    });
  }

  async function restoreDeleted(card: ModCardDto) {
    try {
      await modRestoreDeleted(card.fileName);
      picked = EMPTY;
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
    picked = selectOnly(card.id);
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
      const paths = (Array.isArray(picked) ? picked : picked ? [picked] : []).filter(
        isArchivePath,
      );
      if (paths.length > 0) incomingQueue.add(paths);
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  function onArchiveInput(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const names = Array.from(input.files ?? [])
      .map((file) => file.name)
      .filter(isArchivePath);
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
    if (isShortcut(event, "f")) {
      event.preventDefault();
      searchEl?.focus();
      searchEl?.select();
      return;
    }
    if (isShortcut(event, "z")) {
      event.preventDefault();
      void undoToast();
      return;
    }
    if (event.key === "Escape") {
      if (picked.ids.length > 0) {
        picked = EMPTY;
      } else if (query) {
        query = "";
      }
      return;
    }
    if (isTyping(event)) return;
    if (isShortcut(event, "a") && filtered.length > 0) {
      event.preventDefault();
      picked = selectAll(filtered.filter((mod) => !mod.deleted).map((mod) => mod.id));
      return;
    }
    if (picked.ids.length > 1) {
      if (event.key === "Delete") {
        event.preventDefault();
        void bulkDelete();
      }
      return;
    }
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
              ? -rowStep()
              : rowStep();
      const next = list[Math.max(0, Math.min(list.length - 1, index + step))];
      if (next) picked = selectOnly(next.id);
    }
  }

  /** Сколько карточек в ряду: в списке — одна. */
  function rowStep(): number {
    if (view === "list") return 1;
    const card = document.querySelector<HTMLElement>(".garage [data-mod-id]");
    const grid = card?.parentElement;
    if (!grid) return 4;
    return Math.max(1, getComputedStyle(grid).gridTemplateColumns.split(" ").length);
  }

  function setView(mode: ViewMode) {
    view = mode;
    saveViewMode(mode);
  }

  async function openPageOf(mod: ModCardDto) {
    try {
      const found = await modDetails(mod.id);
      if (found.binding?.url) {
        await openExternal(found.binding.url);
      } else {
        picked = selectOnly(mod.id);
        showToast("Страница мода не указана — её можно указать в панели мода");
      }
    } catch (reason) {
      showToast(errorText(reason));
    }
  }

  async function showFolderOf(mod: ModCardDto) {
    try {
      const found = await modDetails(mod.id);
      if (found.folder) await openPath(found.folder);
    } catch (reason) {
      showToast(errorText(reason));
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
    if (filter === "disabled") return "Выключенных нет";
    if (typeFilter !== "all") return "Таких модов нет";
    if (filter === "deleted") return "Удалённых модов нет";
    return "Модов пока нет";
  }

  function emptyText(): string {
    if (snapshot?.folderError) {
      return snapshot.folderError;
    }
    if (query) return "Проверьте написание. Ищем только среди ваших модов.";
    if (filter === "updates") return "Обновлений нет. Проверить снова можно на экране «Обновления».";
    if (filter === "unchecked") return "У всех модов есть страница — обновления проверяются.";
    if (filter === "disabled") return "Все моды включены.";
    if (typeFilter !== "all") return "Сбросьте фильтр типа, чтобы увидеть остальные.";
    if (filter === "deleted") return "Удалённые моды появятся здесь после подтверждения.";
    return "Найдите мод в поиске или перетащите архив на окно.";
  }
</script>

<svelte:window onkeydown={onkeydown} />

<div class="garage">
  <header class="top" data-tauri-drag-region>
    <div class="title" data-tauri-drag-region>
      <h1 class="display" data-tauri-drag-region>Гараж</h1>
      <p data-tauri-drag-region>
        {note}{snapshot?.gameVersion ? ` · игра ${snapshot.gameVersion}` : ""}
      </p>
    </div>

    <label class="search">
      <MagnifyingGlass size={16} />
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
        <button class="clear" type="button" aria-label="Очистить" onclick={() => (query = "")}>
          <X size={13} weight="bold" />
        </button>
      {:else}
        <button
          class="kbd-hint"
          type="button"
          title="Палитра команд"
          onclick={() => palette.show()}
        ><kbd>Ctrl K</kbd></button>
      {/if}
    </label>

    <div class="seg" role="group" aria-label="Вид">
      <button
        type="button"
        class:on={view === "grid"}
        aria-pressed={view === "grid"}
        aria-label="Сетка"
        title="Сетка"
        onclick={() => setView("grid")}
      ><SquaresFour size={17} weight={view === "grid" ? "fill" : "regular"} /></button>
      <button
        type="button"
        class:on={view === "list"}
        aria-pressed={view === "list"}
        aria-label="Список"
        title="Список"
        onclick={() => setView("list")}
      ><Rows size={17} weight={view === "list" ? "fill" : "regular"} /></button>
    </div>

    <button
      class="btn-glass icon-only"
      type="button"
      onclick={() => void load(true)}
      disabled={refreshing}
      aria-label="Перечитать папку модов"
      title="Перечитать папку модов"
    >
      <ArrowsClockwise size={16} class={refreshing ? "spin" : ""} />
    </button>
    <button
      class="btn-primary"
      type="button"
      onclick={() => void pickArchive()}
      disabled={refreshing}
      title="Установить моды из архива"
    >
      <Plus size={16} weight="bold" />
      Из архива
    </button>
  </header>

  <div class="filters" role="group" aria-label="Фильтры">
    <button
      class="chip"
      class:on={typeFilter === "all" && filter === "all"}
      type="button"
      onclick={() => {
        typeFilter = "all";
        filter = "all";
      }}
    >Все <b>{groupDigits(shownMods.length)}</b></button>
    {#each typeFilters as item (item.id)}
      <button
        class="chip"
        class:on={typeFilter === item.id}
        type="button"
        aria-pressed={typeFilter === item.id}
        onclick={() => (typeFilter = typeFilter === item.id ? "all" : item.id)}
      >{item.label} <b>{groupDigits(kinds[item.id])}</b></button>
    {/each}
    <span class="sep" aria-hidden="true"></span>
    {#each visibleFilters as item (item.id)}
      {@const count = stateCount(item.id)}
      <button
        class="chip"
        class:on={filter === item.id}
        class:hot={item.id === "updates" && count > 0}
        type="button"
        aria-pressed={filter === item.id}
        onclick={() => (filter = filter === item.id ? "all" : item.id)}
      >{item.label} <b>{groupDigits(count)}</b></button>
    {/each}
  </div>

  {#if snapshot?.warning}
    <div class="banner">{snapshot.warning}</div>
  {/if}
  {#if snapshot?.manyEnabled && !bannerDismissed}
    <div class="banner many">
      <span>{snapshot.manyEnabled}</span>
      <button type="button" aria-label="Скрыть предупреждение" onclick={() => (bannerDismissed = true)}>
        <X size={14} weight="bold" />
      </button>
    </div>
  {/if}

  <div class="body" class:with-inspector={selectedId !== null || picked.ids.length > 1}>
    <section class="work">
      {#if loading}
        <div class="loading" aria-label="Загружаем моды">
          {#each Array.from({ length: 8 }, (_, index) => index) as index (index)}
            <div class="skeleton ghost-card"></div>
          {/each}
        </div>
      {:else if error}
        <div class="state error">
          <p>Не удалось загрузить моды</p>
          <span>{error}</span>
          <button type="button" class="btn-glass" onclick={() => void load(true)}>Попробовать снова</button>
        </div>
      {:else if snapshot && (snapshot.folderError || (snapshot.mods.length === 0 && snapshot.deleted.length === 0) || filtered.length === 0)}
        <div class="state">
          <p class="display">{emptyTitle()}</p>
          <span>{emptyText()}</span>
          {#if snapshot.folderError}
            <a class="btn-primary" href="/settings?section=folders">Выбрать папку</a>
          {:else if query}
            <div class="state-actions">
              <a class="btn-primary" href={`/search?q=${encodeURIComponent(query)}`}>Искать на сайтах</a>
              <button type="button" class="btn-glass" onclick={() => (query = "")}>Сбросить поиск</button>
            </div>
          {:else if filter !== "all" || typeFilter !== "all"}
            <button
              type="button"
              class="btn-glass"
              onclick={() => {
                filter = "all";
                typeFilter = "all";
              }}>Показать все</button
            >
          {:else if snapshot.mods.length === 0}
            <a class="btn-primary" href="/search">Перейти в поиск</a>
          {/if}
        </div>
      {:else if snapshot}
        <ModMenu
          mods={allCards}
          onToggle={(mod) => void toggle(mod)}
          onOpenPage={(mod) => void openPageOf(mod)}
          onShowFolder={(mod) => void showFolderOf(mod)}
          onShowDetails={(mod) => (picked = selectOnly(mod.id))}
          onDelete={(mod) => void deleteCard(mod)}
          selectedIds={picked.ids}
          onBulk={(action) =>
            void (action === "delete" ? bulkDelete() : bulkToggle(action === "enable"))}
        >
          {#if view === "list"}
            <ModList
              items={filtered}
              selectedIds={picked.ids}
              onselect={select}
              ontoggle={(mod) => {
                if (!mod.deleted) void toggle(mod);
              }}
              resetKey={`${filter}|${typeFilter}|${query}`}
            />
          {:else}
            <ModGrid
              items={filtered}
              selectedIds={picked.ids}
              onselect={select}
              ontoggle={(mod) => {
                if (!mod.deleted) void toggle(mod);
              }}
              resetKey={`${filter}|${typeFilter}|${query}`}
              bind:renderedCount
            />
          {/if}
        </ModMenu>
      {/if}
    </section>
    {#if picked.ids.length > 1}
      <BulkBar
        count={picked.ids.length}
        {plan}
        busy={bulkBusy}
        onEnable={() => void bulkToggle(true)}
        onDisable={() => void bulkToggle(false)}
        onDelete={() => void bulkDelete()}
        onClear={() => (picked = EMPTY)}
      />
    {:else if selected?.deleted}
      <DeletedInspector
        card={selected}
        onRestore={(card) => void restoreDeleted(card)}
        onClose={() => (picked = EMPTY)}
      />
    {:else if selectedId !== null}
      <Inspector
        {details}
        loading={detailsLoading}
        canDelete={selected ? !selected.fromGameRepo : false}
        problem={selected?.problem ?? null}
        duplicateOf={selected?.duplicateOf ?? null}
        addon={selected?.role === "addon"}
        tag={selected ? typeTag(selected) : "mod"}
        group={selected && snapshot && selected.group.length > 0 ? groupMembers(selected, snapshot.mods) : []}
        onShowMod={(id) => (picked = selectOnly(id))}
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
        onClose={() => (picked = EMPTY)}
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
  }

  /* Шапка: заголовок, поиск, вид и действия; справа место под кнопки окна. */
  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px;
    padding: 18px 150px 14px 24px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    flex: 1 1 220px;
    min-width: 0;
  }

  h1 {
    margin: 0;
    font-size: 30px;
    line-height: 1.05;
  }

  .title p {
    margin: 3px 0 0;
    overflow: hidden;
    color: var(--muted-foreground);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 300px;
    height: 36px;
    padding: 0 6px 0 12px;
    box-sizing: border-box;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--muted-foreground);
    transition: border-color var(--dur-2) var(--ease-out);
  }

  .search:focus-within {
    border-color: var(--accent-brand);
  }

  .search input {
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--foreground);
    font-size: 14px;
    outline: none;
  }

  .search input::placeholder {
    color: var(--faint-foreground);
  }

  .search input::-webkit-search-cancel-button {
    display: none;
  }

  .clear,
  .kbd-hint {
    display: grid;
    place-items: center;
    min-width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 5px;
    background: transparent;
    color: var(--muted-foreground);
    cursor: default;
  }

  .clear:hover {
    background: var(--surface-3);
    color: var(--foreground);
  }

  .seg {
    display: flex;
    overflow: hidden;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
  }

  .seg button {
    display: grid;
    place-items: center;
    width: 38px;
    height: 34px;
    border: 0;
    background: var(--surface);
    color: var(--muted-foreground);
    cursor: default;
    transition:
      background-color var(--dur-2) var(--ease-out),
      color var(--dur-2) var(--ease-out);
  }

  .seg button.on {
    background: var(--surface-3);
    color: var(--foreground);
  }

  .icon-only {
    width: 36px;
    padding: 0;
  }

  .filters {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 12px 24px;
    border-bottom: 1px solid var(--border);
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
    white-space: nowrap;
    cursor: default;
    transition:
      background-color var(--dur-2) var(--ease-out),
      color var(--dur-2) var(--ease-out),
      border-color var(--dur-2) var(--ease-out);
  }

  .chip b {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 500;
    color: var(--faint-foreground);
  }

  .chip:hover {
    color: var(--foreground);
    border-color: var(--faint-foreground);
  }

  .chip.on {
    border-color: var(--foreground);
    background: var(--foreground);
    color: var(--background);
  }

  .chip.on b {
    color: color-mix(in oklab, var(--background) 70%, transparent);
  }

  .chip.hot:not(.on) b {
    color: var(--accent-brand-text);
  }

  .sep {
    width: 1px;
    height: 20px;
    margin: 0 6px;
    background: var(--border-strong);
  }

  .banner {
    margin: 12px 24px 0;
    padding: 9px 14px;
    border: 1px solid var(--warning-border);
    border-radius: 8px;
    background: var(--warning-soft);
    color: var(--warning);
    font-size: 13px;
    animation: rise-in var(--dur-3) var(--ease-out) both;
  }

  .banner.many {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  .banner.many button {
    display: grid;
    place-items: center;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: default;
  }

  .body {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    flex: 1;
    min-height: 0;
  }

  .body.with-inspector {
    grid-template-columns: minmax(0, 1fr) 360px;
  }

  .work {
    min-width: 0;
    min-height: 0;
  }

  .loading {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(228px, 1fr));
    gap: 16px;
    padding: 20px 24px;
  }

  .ghost-card {
    height: 206px;
    border-radius: 12px;
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

  .state span {
    max-width: 440px;
    font-size: 13.5px;
    line-height: 1.5;
  }

  .state .btn-primary,
  .state .btn-glass {
    margin-top: 6px;
    text-decoration: none;
  }

  .state-actions {
    display: flex;
    gap: 8px;
  }

  .state.error p {
    color: var(--danger);
  }

  /* Узкое окно: панель не прячется, а ложится поверх сетки справа. */
  @media (max-width: 980px) {
    .body.with-inspector {
      grid-template-columns: minmax(0, 1fr);
    }

    .body.with-inspector > :global(aside) {
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      z-index: 5;
      width: min(360px, 92%);
      box-shadow: -18px 0 40px rgb(0 0 0 / 0.35);
    }
  }
</style>

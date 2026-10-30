<script lang="ts">
  import { openExternal } from "#lib/shell/openExternal.js";
  import { page } from "$app/state";
  import Package from "phosphor-svelte/lib/Package";
  import PuzzlePiece from "phosphor-svelte/lib/PuzzlePiece";
  import DotsThree from "phosphor-svelte/lib/DotsThree";
  import { DropdownMenu } from "bits-ui";
  import { expectDownload } from "#lib/ipc/incoming.js";
  import { downloadOpen, downloadOpenBatch, type DownloadRequest } from "#lib/ipc/download.js";
  import { onDestroy, onMount } from "svelte";
  import { sourceName } from "#lib/ipc/search.js";
  import { formatAgo, formatSize, formatWhen, plural } from "#lib/features/garage/format.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import { checkSummary, failedSites, updateHeadline } from "#lib/features/updates/summary.js";
  import { motion } from "#lib/shell/motion.svelte.js";
  import { reason, sizeChange } from "#lib/features/updates/reason.js";
  import {
    listenUpdates,
    updatesCancel,
    updatesCheckMod,
    updatesConfirmPage,
    updatesDismiss,
    updatesFindPages,
    updatesOpenModland,
    updatesRefollow,
    updatesRejectPage,
    updatesUnfollow,
    updatesForgetLost,
    updatesRestore,
    updatesRetrySource,
    updatesSnapshot,
    updatesVerifyPage,
    updatesStart,
    updatesUndoPage,
    type PageSuggestion,
    type PageVerification,
    type LostPage,
    type UpdateFinding,
    type UpdateProgress,
    type UpdateSource,
    type UpdatesEvent,
    type UpdatesSnapshot,
  } from "#lib/ipc/updates.js";

  type Filter = "available" | "hidden" | "pages";

  let snapshot = $state<UpdatesSnapshot>({
    findings: [],
    hidden: [],
    suggestions: [],
    sites: [],
    lastChecked: null,
    lost: [],
  });
  let sources = $state<UpdateSource[]>([]);
  let loading = $state(true);
  let checking = $state(false);
  let passing = $state(false);
  let filter = $state<Filter>("available");
  let verifying = $state<string | null>(null);
  let verifications = $state<Record<string, PageVerification | null>>({});
  let verifyErrors = $state<Record<string, string | null>>({});

  function setTab(next: Filter) {
    if (next === filter) return;
    void motion.run(() => {
      filter = next;
    }, "flow-section");
  }
  let progress = $state<UpdateProgress | null>(null);
  /** Ход проверки при запуске (она идёт сама, без нажатия). */
  let autoProgress = $state<UpdateProgress | null>(null);
  /** Полоса проверки доходит до конца и только потом уступает кнопкам. */
  let finishing = $state(false);
  let finishTimer: ReturnType<typeof setTimeout> | undefined;
  /** Номер запроса проверки при запуске (`AUTO_REQUEST_ID` в окне). */
  const AUTO_REQUEST = 0;
  let error = $state<string | null>(null);
  // Обычная проверка не трогает моды, проверенные недавно (срок — в настройках). Если она
  // никуда не сходила, следующее нажатие проверяет всё заново, как в legacy.
  let offerForce = $state(false);
  let requestId = 0;
  let readToken = 0;
  let unlisten: (() => void) | undefined;
  let disposed = false;

  const counts = $derived({
    available: snapshot.findings.length,
    hidden: snapshot.hidden.length,
    pages: snapshot.suggestions.length + (snapshot.lost?.length ?? 0),
  });
  const headline = $derived(
    updateHeadline(counts.available, snapshot.sites, snapshot.lastChecked),
  );
  const failed = $derived(failedSites(snapshot.sites));

  // «Проверить обновления сейчас» из палитры команд: /updates?check=1
  let askedCheck = false;
  $effect(() => {
    if (page.url.searchParams.get("check") !== "1" || askedCheck || loading) return;
    askedCheck = true;
    void check();
  });

  onMount(() => {
    void load();
    void listenUpdates(onEvent)
      .then((stop) => {
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      })
      .catch(() => {});
    // Обновление поставили (из «Загрузок» или вручную) — пункт должен исчезнуть сразу.
    const onLibraryChanged = () => void refresh();
    window.addEventListener("bmm:library-changed", onLibraryChanged);
    return () => {
      disposed = true;
      unlisten?.();
      window.removeEventListener("bmm:library-changed", onLibraryChanged);
    };
  });

  /** Перечитать список без заставки загрузки. */
  async function refresh() {
    const token = ++readToken;
    try {
      const fresh = await updatesSnapshot();
      if (token === readToken) snapshot = fresh;
    } catch (reason) {
      if (token === readToken) error = errorText(reason);
    }
  }

  onDestroy(() => {
    disposed = true;
    unlisten?.();
    if (finishTimer) clearTimeout(finishTimer);
    if (checking) void updatesCancel();
  });

  function stopFinishing() {
    if (finishTimer) clearTimeout(finishTimer);
    finishTimer = undefined;
    finishing = false;
  }

  function finishProgress() {
    if (finishTimer) clearTimeout(finishTimer);
    finishing = true;
    finishTimer = setTimeout(() => {
      finishing = false;
      finishTimer = undefined;
    }, 420);
  }

  function onEvent(event: UpdatesEvent) {
    if (event.requestId === AUTO_REQUEST && event.phase === "progress") {
      stopFinishing();
      autoProgress = event.progress;
      return;
    }
    if (event.requestId === AUTO_REQUEST && event.phase === "error") {
      stopFinishing();
      autoProgress = null;
      return;
    }
    if (event.phase === "auto") {
      const hadProgress = autoProgress !== null;
      autoProgress = null;
      void refreshSites();
      // Фоновая проверка закончилась: показываем найденное, если сами сейчас не проверяем.
      if (!checking) {
        readToken += 1;
        loading = false;
        snapshot = {
          ...snapshot,
          findings: event.findings,
          hidden: event.hidden,
          suggestions: event.suggestions,
        };
        if (event.sources.length > 0) sources = event.sources;
        if (hadProgress) finishProgress();
      }
      return;
    }
    if (event.requestId !== requestId) return;
    if (event.phase === "progress") {
      stopFinishing();
      progress = event.progress;
      if (event.sources.length > 0) sources = event.sources;
      return;
    }
    if (event.phase === "error") {
      error = event.error ?? "Проверка не удалась.";
      checking = false;
      progress = null;
      stopFinishing();
      return;
    }
    const known = new Set(snapshot.suggestions.map((item) => item.modId));
    const found = event.suggestions.filter((item) => !known.has(item.modId)).length;
    readToken += 1;
    loading = false;
    snapshot = {
      ...snapshot,
      findings: event.findings,
      hidden: event.hidden,
      suggestions: event.suggestions,
    };
    if (event.sources.length > 0) sources = event.sources;
    checking = false;
    progress = null;
    finishProgress();
    if (event.phase === "done" || event.phase === "pages") void refreshSites();
    if (event.phase === "done") {
      const summary = checkSummary(event);
      offerForce = summary.offerForce;
      if (summary.text) toasts.show(summary.text);
    }
    if (event.phase === "pages") filter = "pages";
    const followed = event.phase === "pages" ? (event.followed ?? 0) : 0;
    if (followed > 0 && found > 0) {
      toasts.show(`Следим за новыми страницами: ${followed}; на проверку: ${found}`);
    } else if (followed > 0) {
      toasts.show(`Следим за новыми страницами: ${followed}`);
    } else if (found > 0) toasts.show(`Нашли новые страницы (${found})`);
    else if (event.phase === "pages" && !event.cancelled) {
      // Говорим, что именно искали: «не нашли» без числа звучало как «не искали».
      toasts.show(
        event.requests > 0
          ? `Искали страницы для ${event.requests} ${plural(event.requests, "мода", "модов", "модов")} — новых не нашли`
          : "Страницы не искали: сайты не ответили или искать нечего",
      );
    }
  }

  // Панель «Проверка»: состояние сайтов и время последней проверки — из снимка.
  async function refreshSites() {
    const token = readToken;
    try {
      const fresh = await updatesSnapshot();
      if (token === readToken) {
        snapshot = {
          ...snapshot,
          sites: fresh.sites,
          lastChecked: fresh.lastChecked,
          lost: fresh.lost,
        };
      }
    } catch {
      // Панель просто останется прежней.
    }
  }

  async function load() {
    const token = ++readToken;
    loading = true;
    error = null;
    try {
      const fresh = await updatesSnapshot();
      if (token === readToken) snapshot = fresh;
    } catch (reason) {
      if (token === readToken) error = errorText(reason);
    } finally {
      if (token === readToken) loading = false;
    }
  }

  async function check() {
    if (checking) return;
    stopFinishing();
    requestId += 1;
    checking = true;
    progress = null;
    error = null;
    const force = offerForce;
    offerForce = false;
    try {
      await updatesStart(force, requestId);
    } catch (reason) {
      error = errorText(reason);
      checking = false;
    }
  }

  async function cancel() {
    stopFinishing();
    requestId += 1;
    checking = false;
    progress = null;
    autoProgress = null;
    await updatesCancel().catch(() => {});
  }

  async function findPages() {
    if (checking) return;
    stopFinishing();
    checking = true;
    progress = null;
    error = null;
    requestId += 1;
    try {
      await updatesFindPages(requestId);
    } catch (reason) {
      error = errorText(reason);
      checking = false;
    }
  }

  /** Мутация возвращает новый снимок: применяем его, только если ответ не устарел. */
  async function applySnapshot(load: () => Promise<UpdatesSnapshot>) {
    const token = ++readToken;
    try {
      const next = await load();
      if (token === readToken) snapshot = next;
    } catch (reason) {
      if (token === readToken) error = errorText(reason);
    }
  }

  async function dismiss(finding: UpdateFinding) {
    await applySnapshot(() => updatesDismiss(finding.modId));
  }

  async function restore(finding: UpdateFinding) {
    await applySnapshot(() => updatesRestore(finding.modId));
  }

  async function confirmPage(suggestion: PageSuggestion) {
    await applySnapshot(() => updatesConfirmPage(suggestion.modId));
    toasts.show(`Следим за страницей «${suggestion.modName}»`, {
      undo: async () => {
        await applySnapshot(() => updatesUndoPage(suggestion));
      },
    });
    // Подтверждённую страницу проверяем сразу: иначе обновление появится
    // только после следующего запуска проверки.
    const token = readToken;
    void updatesCheckMod(suggestion.modId)
      .then((next) => {
        if (token === readToken) snapshot = next;
      })
      .catch(() => {});
  }

  async function verifyPage(suggestion: PageSuggestion) {
    // Проверка занимает десятки секунд и идёт через общий браузер страниц:
    // вторую строку запускать нельзя, даже если нажали на другую карточку.
    if (verifying !== null) return;
    verifying = suggestion.modId;
    verifyErrors = { ...verifyErrors, [suggestion.modId]: null };
    try {
      const result = await updatesVerifyPage(suggestion.modId);
      verifications = { ...verifications, [suggestion.modId]: result };
    } catch (reason) {
      verifyErrors = {
        ...verifyErrors,
        [suggestion.modId]: errorText(reason),
      };
    } finally {
      verifying = null;
    }
  }

  async function rejectPage(suggestion: PageSuggestion) {
    await applySnapshot(() => updatesRejectPage(suggestion.modId));
    toasts.show(`Страница «${suggestion.modName}» отклонена`, {
      undo: async () => {
        await applySnapshot(() => updatesUndoPage(suggestion));
      },
    });
  }

  async function unfollow(finding: UpdateFinding) {
    await applySnapshot(() => updatesUnfollow(finding.modId, finding.source));
    toasts.show(`Больше не следим за страницей «${finding.modName}»`, {
      undo: async () => {
        await applySnapshot(() =>
          updatesRefollow(finding.modId, finding.source),
        );
      },
    });
  }

  /** «Страница пропала»: искать новую (сразу запускаем поиск) или просто перестать следить. */
  async function forgetLost(page: LostPage, searchAgain: boolean) {
    await applySnapshot(() =>
      updatesForgetLost(page.modId, page.source, searchAgain),
    );
    if (searchAgain) {
      await findPages();
    } else {
      toasts.show(`Больше не следим за «${page.modName}»`, {
        undo: async () => {
          await applySnapshot(() =>
            updatesRefollow(page.modId, page.source),
          );
        },
      });
    }
  }

  async function openModland() {
    if (passing) return;
    passing = true;
    try {
      const passed = await updatesOpenModland();
      if (passed) {
        toasts.show("Modland пускает программу — проверяем снова");
        sources = sources.map((item) =>
          item.id === "modland" ? { ...item, status: "ok", note: null } : item,
        );
        await check();
      }
    } catch (reason) {
      error = errorText(reason);
    } finally {
      passing = false;
    }
  }

  async function openLink(url: string) {
    await openExternal(url);
  }

  function requestOf(finding: UpdateFinding): DownloadRequest {
    return {
      modName: finding.modName,
      installedVersion: finding.installedVersion,
      latestVersion: finding.latestVersion,
      pageUrl: finding.url,
      previewUrl: finding.previewUrl ?? null,
      role: finding.role ?? null,
    };
  }

  // «Обновить все» — только уверенные обновления: «возможно» сначала смотрит человек.
  const sure = $derived(snapshot.findings.filter((finding) => finding.status === "likely"));

  /** «Обновить все»: окно загрузки проводит по страницам, установка — одна в конце. */
  async function updateAll() {
    if (sure.length === 0) return;
    if (sure.length === 1) {
      await update(sure[0]);
      return;
    }
    try {
      await downloadOpenBatch(sure.map(requestOf));
    } catch (reason) {
      error = errorText(reason);
    }
  }

  /** «Обновить»: окно загрузки; если оно недоступно — браузер и наблюдение за «Загрузками». */
  async function update(finding: UpdateFinding) {
    try {
      await downloadOpen(requestOf(finding));
      return;
    } catch {
      // Ниже — прежний путь: системный браузер и «Загрузки».
    }
    if (!(await openExternal(finding.url))) return;
    await expectDownload();
    toasts.show(
      "Скачайте архив на странице — программа заметит его в «Загрузках» и предложит установить",
    );
  }

  function progressText(current: UpdateProgress | null = progress): string {
    const progress = current;
    if (!progress) return "Проверяем сайты…";
    if (progress.source === "страницы") {
      return progress.total > 0
        ? `Ищем страницы: ${progress.done} из ${progress.total}`
        : "Ищем страницы…";
    }
    const where = progress.source ? `${sourceName(progress.source)}` : "проверка";
    const stage =
      progress.stage === "лента"
        ? "лента"
        : progress.stage === "карточки"
          ? "карточки"
          : progress.stage === "страницы"
            ? "страницы"
            : "готово";
    return progress.total > 0
      ? `${where} · ${stage} ${progress.done} из ${progress.total}`
      : `${where} · ${stage}`;
  }

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось выполнить действие.";
  }

  function whereText(finding: UpdateFinding): string {
    const parts = [sourceName(finding.source)];
    if (finding.noticedAt) parts.push(`замечено ${formatAgo(finding.noticedAt)}`);
    const size = sizeChange(finding);
    if (finding.status === "likely" && size) parts.push(size);
    return parts.join(" · ");
  }

  function siteRows(): {
    id: string;
    name: string;
    tone: string;
    text: string;
    when: string;
    retry: boolean;
  }[] {
    const live = new Map(sources.map((item) => [item.id, item]));
    return (snapshot.sites ?? []).map((site) => {
      const now = live.get(site.id);
      const human = site.status === "human_check" || now?.status === "needsHumanCheck";
      const down = site.status === "unavailable" || now?.status === "unavailable";
      const when = site.lastChecked ? `проверен ${formatAgo(site.lastChecked)}` : "ещё не проверялся";
      if (site.status === "off") {
        return { id: site.id, name: site.displayName, tone: "off", text: "выключен", when: "", retry: false };
      }
      if (human) {
        return { id: site.id, name: site.displayName, tone: "warn", text: "просит проверку", when, retry: false };
      }
      if (down) {
        return {
          id: site.id,
          name: site.displayName,
          tone: "bad",
          text: now?.note ?? site.note ?? "не отвечает",
          when,
          retry: true,
        };
      }
      if (site.tracked === 0) {
        return { id: site.id, name: site.displayName, tone: "off", text: "ни одного мода", when, retry: false };
      }
      return {
        id: site.id,
        name: site.displayName,
        tone: "ok",
        text: `следим за ${site.tracked} ${plural(site.tracked, "модом", "модами", "модами")}`,
        when,
        retry: false,
      };
    });
  }

  async function retrySource(source: string) {
    if (checking) return;
    stopFinishing();
    requestId += 1;
    checking = true;
    progress = null;
    error = null;
    offerForce = false;
    try {
      await updatesRetrySource(source, requestId);
    } catch (reason) {
      error = errorText(reason);
      checking = false;
    }
  }
</script>

{#snippet moreActions(finding: UpdateFinding, dismissLabel: string)}
  <DropdownMenu.Root>
    <DropdownMenu.Trigger class="btn-ghost more" aria-label="Ещё действия">
      <span>Ещё</span>
      <DotsThree size={16} weight="bold" />
    </DropdownMenu.Trigger>
    <DropdownMenu.Portal>
      <DropdownMenu.Content class="updates-menu" sideOffset={6} align="end">
        <DropdownMenu.Item class="menu-item" onSelect={() => void dismiss(finding)}>
          {dismissLabel}
        </DropdownMenu.Item>
        <DropdownMenu.Separator class="menu-separator" />
        <DropdownMenu.Item class="menu-item" onSelect={() => void unfollow(finding)}>
          Это другой мод
        </DropdownMenu.Item>
      </DropdownMenu.Content>
    </DropdownMenu.Portal>
  </DropdownMenu.Root>
{/snippet}

<div class="updates">
  <header class="top" data-tauri-drag-region>
    <div class="title" data-tauri-drag-region>
      <h1 class="display" data-tauri-drag-region>Обновления</h1>
      <p data-tauri-drag-region class:warn={headline.tone === "warn"}>
        {headline.text}
      </p>
    </div>
    <div class="tabs" role="tablist" aria-label="Что показать">
      <button
        id="updates-tab-available"
        type="button"
        role="tab"
        class="tab"
        class:active={filter === "available"}
        aria-selected={filter === "available"}
        aria-controls="updates-panel"
        onclick={() => setTab("available")}
      >
        {#if filter === "available"}<span class="line" aria-hidden="true"></span>{/if}
        Доступные <b>{counts.available}</b>
      </button>
      <button
        id="updates-tab-hidden"
        type="button"
        role="tab"
        class="tab"
        class:active={filter === "hidden"}
        aria-selected={filter === "hidden"}
        aria-controls="updates-panel"
        onclick={() => setTab("hidden")}
      >
        {#if filter === "hidden"}<span class="line" aria-hidden="true"></span>{/if}
        Пропущенные <b>{counts.hidden}</b>
      </button>
      <button
        id="updates-tab-pages"
        type="button"
        role="tab"
        class="tab"
        class:active={filter === "pages"}
        aria-selected={filter === "pages"}
        aria-controls="updates-panel"
        onclick={() => setTab("pages")}
      >
        {#if filter === "pages"}<span class="line" aria-hidden="true"></span>{/if}
        Страницы <b>{counts.pages}</b>
      </button>
    </div>
  </header>

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  <div class="body">
    <div
      id="updates-panel"
      class="list scroll-area flow-section-body"
      role="tabpanel"
      aria-labelledby={`updates-tab-${filter}`}
    >
      {#if loading}
        <div class="state"><p class="display">Загружаем обновления…</p></div>
      {:else if filter === "available" && snapshot.findings.length === 0}
        <div class="state">
          <p class="display">Обновлений нет</p>
          <span>
            {snapshot.lastChecked
              ? `Последняя проверка ${formatWhen(snapshot.lastChecked)}. Проверить снова можно кнопкой «Проверить сейчас».`
              : "Нажмите «Проверить сейчас», чтобы сверить версии со всеми включёнными сайтами."}
          </span>
        </div>
      {:else if filter === "hidden" && snapshot.hidden.length === 0}
        <div class="state">
          <p class="display">Скрытых версий нет</p>
          <span>Пропущенные версии появятся здесь — их можно вернуть в список.</span>
        </div>
      {:else if filter === "pages" && counts.pages === 0}
        <div class="state">
          <p class="display">Страниц для проверки нет</p>
          <span>
            Если программа не уверена, что нашла страницу именно вашего мода, она спросит здесь.
          </span>
        </div>
      {:else if filter === "available" || filter === "hidden"}
        {#if filter === "available" && sure.length > 1}
          <div class="all">
            <div>
              <b>Обновить все — {sure.length} {plural(sure.length, "мод", "мода", "модов")}</b>
              <span>
                Окно загрузки откроет страницы по очереди: вы нажимаете Download, а в конце
                программа ставит всё одной кнопкой.
              </span>
            </div>
            <button class="btn-primary" type="button" onclick={() => void updateAll()}>
              Обновить все
            </button>
          </div>
        {/if}
        {#each filter === "available" ? snapshot.findings : snapshot.hidden as finding (finding.modId)}
          {@const possible = finding.status !== "likely"}
          <article class="card">
            <div class="photo">
              {#if finding.previewUrl && finding.role !== "addon"}
                <img src={finding.previewUrl} alt="" loading="lazy" />
              {:else if finding.role === "addon"}
                <PuzzlePiece size={48} weight="thin" />
              {:else}
                <Package size={48} weight="thin" />
              {/if}
            </div>
            <div class="content">
              <div class="head">
                <h2>{finding.modName}</h2>
                {#if filter === "hidden"}
                  <span class="chip muted">Пропущено</span>
                {:else if possible}
                  <span class="chip possible">Возможно, обновление</span>
                {:else}
                  <span class="chip update">Обновление</span>
                {/if}
              </div>
              {#if finding.installedVersion && finding.latestVersion && !possible}
                <p class="versions mono"><s>{finding.installedVersion}</s> → <b>{finding.latestVersion}</b></p>
              {:else if finding.latestVersion && !possible}
                <p class="versions mono"><b>→ {finding.latestVersion}</b></p>
              {/if}
              <p class="why">{reason(finding)}</p>
              <p class="where">{whereText(finding)}</p>
              <div class="actions">
                {#if filter === "hidden"}
                  <button class="btn-primary" type="button" onclick={() => void restore(finding)}>Вернуть в список</button>
                  <button class="btn-ghost" type="button" onclick={() => void openLink(finding.url)}>Открыть страницу</button>
                {:else if possible}
                  <button class="btn-primary" type="button" onclick={() => void update(finding)}>Открыть страницу</button>
                  {@render moreActions(finding, "Пропустить это изменение")}
                {:else}
                  <button class="btn-primary" type="button" onclick={() => void update(finding)}>Обновить</button>
                  <button class="btn-ghost" type="button" onclick={() => void openLink(finding.url)}>Открыть страницу</button>
                  {@render moreActions(finding, "Пропустить это обновление")}
                {/if}
              </div>
            </div>
          </article>
        {/each}
      {:else}
        {#each snapshot.lost ?? [] as page (page.modId + page.source)}
          <article class="card compact">
            <div class="photo">
              {#if page.previewUrl && page.role !== "addon"}
                <img src={page.previewUrl} alt="" loading="lazy" />
              {:else if page.role === "addon"}
                <PuzzlePiece size={48} weight="thin" />
              {:else}
                <Package size={48} weight="thin" />
              {/if}
            </div>
            <div class="content">
              <div class="head"><h2>{page.modName}</h2><span class="chip lost">Страница пропала</span></div>
              <p class="why">
                На сайте {sourceName(page.source)} этой страницы нет уже несколько проверок: мод удалили или перенесли.
                Обновлений по ней больше не будет.
              </p>
              <p class="where url" title={page.url}>{page.url}</p>
              <div class="actions">
                <button class="btn-primary" type="button" onclick={() => void forgetLost(page, true)}>Найти новую страницу</button>
                <button class="btn-ghost" type="button" onclick={() => void openLink(page.url)}>Открыть старую</button>
                <button class="btn-ghost" type="button" onclick={() => void forgetLost(page, false)}>Перестать следить</button>
              </div>
            </div>
          </article>
        {/each}
        {#each snapshot.suggestions as suggestion (suggestion.modId + suggestion.source)}
          {@const verification = verifications[suggestion.modId]}
          <article class="card compact">
            <div class="photo"><Package size={48} weight="thin" /></div>
            <div class="content">
              <div class="head"><h2>{suggestion.modName}</h2><span class="chip possible">Это его страница?</span></div>
              <p class="why">
                На сайте {sourceName(suggestion.source)} нашлась похожая страница, но уверенности нет: проверьте её.
              </p>
              <p class="where url" title={suggestion.url}>{suggestion.url}</p>
              <div class="actions">
                <button class="btn-primary" type="button" onclick={() => void confirmPage(suggestion)}>Следить</button>
                <button class="btn-ghost" type="button" onclick={() => void openLink(suggestion.url)}>Открыть страницу</button>
                <button
                  class="btn-ghost"
                  type="button"
                  disabled={verifying !== null}
                  onclick={() => void verifyPage(suggestion)}
                >
                  {verifying === suggestion.modId ? "Проверяем…" : "Проверить на других сайтах"}
                </button>
                <button class="btn-ghost" type="button" onclick={() => void rejectPage(suggestion)}>Это другой мод</button>
              </div>
              {#if verifying === suggestion.modId}
                <p class="verify-note">Ищем эту страницу на других сайтах…</p>
              {:else if verifyErrors[suggestion.modId]}
                <p class="verify-note error">{verifyErrors[suggestion.modId]}</p>
              {:else if verification}
                <div class="verify">
                  <div class="verify-head">
                    <b>Проверка страницы</b>
                    <span>
                      {sourceName(verification.pageSource)}
                      {#if verification.pageSize} · {formatSize(verification.pageSize)}{/if}
                    </span>
                  </div>
                  {#if verification.matches.length === 0}
                    <p class="verify-empty">
                      На других включённых сайтах совпадений нет — страница есть только на
                      {sourceName(verification.pageSource)}. Проверьте её глазами.
                    </p>
                  {:else}
                    {#each verification.matches as match (match.source + match.url)}
                      <div class="verify-row">
                        <div class="verify-title">
                          <b>{sourceName(match.source)}</b>
                          <span>{match.title}</span>
                        </div>
                        <small>
                          {match.version ? `версия ${match.version}` : "версия не указана"}
                          · {match.size ? formatSize(match.size) : "размер не указан"}
                          {#if match.sameSize} · размер совпал{/if}
                          {#if match.sameVersion} · версия совпала{/if}
                          {#if match.sameAuthor} · автор совпал{/if}
                        </small>
                        <button class="btn-ghost" type="button" onclick={() => void openLink(match.url)}>
                          Открыть
                        </button>
                      </div>
                    {/each}
                  {/if}
                </div>
              {/if}
            </div>
          </article>
        {/each}
      {/if}
    </div>

    <aside class="panel" aria-label="Проверка обновлений">
      <h2 class="display">Проверка</h2>
      <p class="sub">
        {#if checking}
          {progressText()}
        {:else if autoProgress}
          Проверка при запуске: {progressText(autoProgress)}
        {:else if finishing}
          Проверка завершена.
        {:else if snapshot.lastChecked}
          Последняя проверка {formatWhen(snapshot.lastChecked)}.
          {#if failed.length > 0}
            Не проверены: {failed.map((site) => site.displayName).join(", ")}.
          {/if}
          Окно можно закрыть: найденное сохранится.
        {:else}
          Ещё не проверяли. Проверим моды, за которыми следим на сайтах.
        {/if}
      </p>
      {#if checking || autoProgress || finishing}
        {@const shown = checking ? progress : autoProgress}
        <div class="track" class:complete={finishing}>
          <span
            class="bar"
            class:pulse={!finishing && !shown?.total}
            style={finishing
              ? "width:100%"
              : shown?.total
                ? `width:${Math.round((shown.done / shown.total) * 100)}%`
                : undefined}
          ></span>
        </div>
        {#if !finishing}
          <button class="btn-glass wide" type="button" onclick={() => void cancel()}>Остановить</button>
        {/if}
      {:else}
        <button class="btn-primary wide" type="button" onclick={() => void check()}>
          {offerForce ? "Всё равно проверить" : "Проверить сейчас"}
        </button>
        <button class="btn-glass wide" type="button" onclick={() => void findPages()}>
          Найти страницы для остальных модов
        </button>
      {/if}

      <h2 class="display sites-title">Сайты</h2>
      <div class="sites">
        {#each siteRows() as site (site.id)}
          <div class="site" title={site.when ? `${site.text} · ${site.when}` : site.text}>
            <span class="dot {site.tone}"></span>
            <b class="site-name">{site.name}</b>
            <span class="site-meta">
              <span class="state-text">{site.text}</span>
              {#if site.retry}
                <button
                  class="site-retry"
                  type="button"
                  disabled={checking}
                  onclick={() => void retrySource(site.id)}
                >
                  Повторить
                </button>
              {:else if site.when}
                <span class="site-when">{site.when}</span>
              {/if}
            </span>
          </div>
          <!-- Modland без единого мода иначе никогда не спросит проверку: без неё он
               не отвечает и поиску страниц, и мод на нём так и не найдётся. -->
          {#if site.id === "modland" && (site.tone === "warn" || site.text === "ни одного мода")}
            <button class="btn-glass pass" type="button" disabled={passing} onclick={() => void openModland()}>
              {passing ? "Проходим проверку…" : "Пройти проверку Modland"}
            </button>
          {/if}
        {/each}
      </div>
    </aside>
  </div>
</div>

<style>
  .updates {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 16px 24px;
    padding: 18px 150px 0 24px;
    border-bottom: 1px solid var(--border);
  }

  .title {
    flex: 1 1 240px;
    min-width: 0;
    padding-bottom: 14px;
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

  .title p.warn {
    color: var(--warning);
  }

  .tabs {
    display: flex;
    gap: 4px;
  }

  .tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 44px;
    padding: 0 14px;
    border: 0;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 14px;
    font-weight: 700;
    cursor: default;
    transition: color var(--dur-2) var(--ease-out);
  }

  .tab b {
    font-family: var(--font-mono);
    font-size: 11px;
    font-weight: 500;
    color: var(--faint-foreground);
  }

  /* Черта под вкладкой переезжает к новой (view-transition), а не гаснет. */
  .line {
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: -1px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent-brand);
    view-transition-name: tab-line;
  }

  .tab:hover {
    color: var(--foreground);
  }

  .tab.active {
    color: var(--foreground);
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

  .list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
    overflow-y: auto;
    padding: 20px 24px 24px;
  }

  .all {
    display: flex;
    flex: none;
    align-items: center;
    gap: 16px;
    padding: 14px 16px;
    border: 1px solid var(--accent-brand);
    border-radius: 12px;
    background: var(--accent-brand-soft);
  }

  .all > div {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }

  .all span {
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .all button {
    flex: none;
  }

  .card {
    display: flex;
    flex-wrap: wrap;
    flex: none;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    animation: rise-in var(--dur-3) var(--ease-out) both;
  }

  .photo {
    position: relative;
    display: grid;
    flex: 0 0 220px;
    place-items: center;
    min-height: 150px;
    overflow: hidden;
    background: var(--surface-2);
    color: var(--faint-foreground);
  }

  .card.compact .photo {
    flex-basis: 160px;
    min-height: 120px;
  }

  .photo img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .content {
    display: flex;
    flex: 1 1 300px;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    padding: 14px 18px 16px;
  }

  .head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 800;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    height: 22px;
    padding: 0 8px;
    border-radius: 5px;
    font-size: 11px;
    font-weight: 700;
    white-space: nowrap;
  }

  .chip.update {
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .chip.possible {
    border: 1px solid var(--warning-border);
    background: var(--warning-soft);
    color: var(--warning);
  }

  .chip.lost {
    border: 1px solid color-mix(in oklab, var(--kind-addon) 45%, transparent);
    color: var(--kind-addon);
  }

  .chip.muted {
    border: 1px solid var(--border-strong);
    color: var(--muted-foreground);
  }

  .versions {
    margin: 0;
    font-size: 14px;
  }

  .versions s {
    color: var(--faint-foreground);
  }

  .versions b {
    color: var(--accent-brand-text);
    font-weight: 500;
  }

  .why {
    margin: 0;
    color: var(--foreground);
    font-size: 13.5px;
    line-height: 1.45;
  }

  .where {
    margin: 0;
    color: var(--faint-foreground);
    font-size: 12.5px;
  }

  .where.url {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 6px;
  }

  .verify-note {
    margin: 2px 0 0;
    color: var(--muted-foreground);
    font-size: 12px;
  }

  .verify-note.error {
    color: var(--danger);
  }

  .verify {
    display: grid;
    gap: 6px;
    margin-top: 4px;
    padding: 9px 10px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--surface-2);
  }

  .verify-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }

  .verify-head b {
    font-size: 12.5px;
  }

  .verify-head span {
    color: var(--muted-foreground);
    font-size: 11.5px;
  }

  .verify-empty {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.4;
  }

  .verify-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 3px 8px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
  }

  .verify-title {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }

  .verify-title b {
    font-size: 11.5px;
    white-space: nowrap;
  }

  .verify-title span {
    overflow: hidden;
    color: var(--foreground);
    font-size: 11.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .verify-row small {
    grid-column: 1;
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .verify-row :global(.btn-ghost) {
    grid-column: 2;
    grid-row: 1 / span 2;
    align-self: center;
  }

  .more {
    gap: 5px;
    padding: 0 10px;
  }

  :global(.updates-menu) {
    z-index: 90;
    min-width: 216px;
    padding: 6px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface-2);
    box-shadow: var(--shadow-2);
    outline: none;
    transform-origin: var(--bits-dropdown-menu-content-transform-origin);
    transition:
      opacity var(--dur-2) var(--ease-out),
      transform var(--dur-2) var(--ease-out);
  }

  :global(.updates-menu[data-starting-style]),
  :global(.updates-menu[data-ending-style]) {
    opacity: 0;
    transform: translateY(-4px) scale(0.98);
  }

  :global(.updates-menu .menu-item) {
    display: flex;
    align-items: center;
    height: 34px;
    padding: 0 10px;
    border-radius: 6px;
    color: var(--foreground);
    font-size: 13.5px;
    cursor: default;
    outline: none;
  }

  :global(.updates-menu .menu-item[data-highlighted]) {
    background: var(--surface-3);
  }

  :global(.updates-menu .menu-item:focus-visible) {
    outline: none;
  }

  :global(.updates-menu .menu-separator) {
    height: 1px;
    margin: 5px 4px;
    background: var(--border);
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

  .sites-title {
    margin-top: 10px !important;
  }

  .sub {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 13px;
    line-height: 1.45;
  }

  .wide {
    width: 100%;
    height: 36px;
  }

  .track {
    height: 6px;
    overflow: hidden;
    border-radius: 3px;
    background: var(--surface-3);
  }

  .bar {
    display: block;
    width: 30%;
    height: 100%;
    border-radius: 3px;
    background: var(--accent-brand);
    transition: width var(--dur-3) var(--ease-out);
  }

  .bar.pulse {
    animation: indeterminate 1.3s var(--ease-in-out) infinite;
  }

  @keyframes indeterminate {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }

  .sites {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .site {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    font-size: 13.5px;
  }

  .site-name {
    min-width: 0;
    font-weight: 700;
    line-height: 1.2;
    overflow-wrap: anywhere;
  }

  .site-meta {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    min-width: 0;
    margin-left: auto;
  }

  .site + .site,
  .pass + .site {
    border-top: 1px solid var(--border);
  }

  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--faint-foreground);
  }

  .dot.ok {
    background: var(--ok);
  }

  .dot.warn {
    background: var(--warning);
  }

  .dot.bad {
    background: var(--danger);
  }

  .state-text {
    color: var(--muted-foreground);
    font-size: 12px;
    text-align: right;
  }

  .site-when {
    color: var(--faint-foreground);
    font-size: 11.5px;
    white-space: nowrap;
  }

  .site-retry {
    height: 24px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 11px;
    font-weight: 600;
    cursor: default;
  }

  .site-retry:hover:not(:disabled) {
    background: var(--surface-3);
    color: var(--foreground);
  }

  .site-retry:disabled {
    opacity: 0.5;
  }

  .pass {
    margin: 0 12px 10px;
    height: 32px;
  }

  /* Узкое окно: панель проверки уходит под список, прокручивается всё вместе. */
  @media (max-width: 1100px) {
    .body {
      display: block;
      overflow-y: auto;
    }

    .list,
    .panel {
      overflow: visible;
    }

    .panel {
      border-left: 0;
      border-top: 1px solid var(--border);
    }
  }
</style>

<script lang="ts">
  import { page } from "$app/state";
  import Car from "phosphor-svelte/lib/Car";
  import { expectDownload } from "#lib/ipc/incoming.js";
  import { onDestroy, onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { sourceName } from "#lib/ipc/search.js";
  import { formatAgo, formatSize, formatWhen, plural } from "#lib/features/garage/format.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import { checkSummary } from "#lib/features/updates/summary.js";
  import { reason, sizeChange } from "#lib/features/updates/reason.js";
  import {
    listenUpdates,
    updatesCancel,
    updatesConfirmPage,
    updatesDismiss,
    updatesFindPages,
    updatesOpenModland,
    updatesRefollow,
    updatesRejectPage,
    updatesUnfollow,
    updatesForgetLost,
    updatesRestore,
    updatesSnapshot,
    updatesStart,
    updatesUndoPage,
    type PageSuggestion,
    type LostPage,
    type UpdateFinding,
    type UpdateProgress,
    type UpdateSource,
    type UpdatesEvent,
    type UpdatesSnapshot,
  } from "#lib/ipc/updates.js";

  type Filter = "available" | "hidden" | "pages";

  let snapshot = $state<UpdatesSnapshot>({ findings: [], hidden: [], suggestions: [] });
  let sources = $state<UpdateSource[]>([]);
  let loading = $state(true);
  let checking = $state(false);
  let passing = $state(false);
  let filter = $state<Filter>("available");
  let progress = $state<UpdateProgress | null>(null);
  /** Ход проверки при запуске (она идёт сама, без нажатия). */
  let autoProgress = $state<UpdateProgress | null>(null);
  /** Номер запроса проверки при запуске (`AUTO_REQUEST_ID` в окне). */
  const AUTO_REQUEST = 0;
  let error = $state<string | null>(null);
  // Обычная проверка не трогает моды, проверенные недавно (срок — в настройках). Если она
  // никуда не сходила, следующее нажатие проверяет всё заново, как в legacy.
  let offerForce = $state(false);
  let requestId = 0;
  let unlisten: (() => void) | undefined;

  const counts = $derived({
    available: snapshot.findings.length,
    hidden: snapshot.hidden.length,
    pages: snapshot.suggestions.length + (snapshot.lost?.length ?? 0),
  });

  // «Проверить обновления сейчас» из палитры команд: /updates?check=1
  let askedCheck = false;
  $effect(() => {
    if (page.url.searchParams.get("check") !== "1" || askedCheck || loading) return;
    askedCheck = true;
    void check();
  });

  onMount(() => {
    void load();
    void listenUpdates(onEvent).then((stop) => {
      unlisten = stop;
    });
    // Обновление поставили (из «Загрузок» или вручную) — пункт должен исчезнуть сразу.
    const onLibraryChanged = () => void refresh();
    window.addEventListener("bmm:library-changed", onLibraryChanged);
    return () => {
      unlisten?.();
      window.removeEventListener("bmm:library-changed", onLibraryChanged);
    };
  });

  /** Перечитать список без заставки загрузки. */
  async function refresh() {
    try {
      snapshot = await updatesSnapshot();
    } catch (reason) {
      error = errorText(reason);
    }
  }

  onDestroy(() => {
    unlisten?.();
    if (checking) void updatesCancel();
  });

  function onEvent(event: UpdatesEvent) {
    if (event.requestId === AUTO_REQUEST && event.phase === "progress") {
      autoProgress = event.progress;
      return;
    }
    if (event.phase === "auto") {
      autoProgress = null;
      void refreshSites();
      // Фоновая проверка закончилась: показываем найденное, если сами сейчас не проверяем.
      if (!checking) {
        snapshot = {
          ...snapshot,
          findings: event.findings,
          hidden: event.hidden,
          suggestions: event.suggestions,
        };
        if (event.sources.length > 0) sources = event.sources;
      }
      return;
    }
    if (event.requestId !== requestId) return;
    if (event.phase === "progress") {
      progress = event.progress;
      if (event.sources.length > 0) sources = event.sources;
      return;
    }
    if (event.phase === "error") {
      error = event.error ?? "Проверка не удалась.";
      checking = false;
      progress = null;
      return;
    }
    const known = new Set(snapshot.suggestions.map((item) => item.modId));
    const found = event.suggestions.filter((item) => !known.has(item.modId)).length;
    snapshot = {
      ...snapshot,
      findings: event.findings,
      hidden: event.hidden,
      suggestions: event.suggestions,
    };
    if (event.sources.length > 0) sources = event.sources;
    checking = false;
    progress = null;
    if (event.phase === "done" || event.phase === "pages") void refreshSites();
    if (event.phase === "done") {
      const summary = checkSummary(event);
      offerForce = summary.offerForce;
      if (summary.text) toasts.show(summary.text);
    }
    if (event.phase === "pages") filter = "pages";
    if (found > 0) toasts.show(`Нашли новые страницы (${found})`);
    else if (event.phase === "pages" && !event.cancelled) toasts.show("Новых страниц не нашли");
  }

  // Панель «Проверка»: состояние сайтов и время последней проверки — из снимка.
  async function refreshSites() {
    try {
      const fresh = await updatesSnapshot();
      snapshot = { ...snapshot, sites: fresh.sites, lastChecked: fresh.lastChecked, lost: fresh.lost };
    } catch {
      // Панель просто останется прежней.
    }
  }

  async function load() {
    loading = true;
    error = null;
    try {
      snapshot = await updatesSnapshot();
    } catch (reason) {
      error = errorText(reason);
    } finally {
      loading = false;
    }
  }

  async function check() {
    if (checking) return;
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
    requestId += 1;
    checking = false;
    progress = null;
    autoProgress = null;
    await updatesCancel().catch(() => {});
  }

  async function findPages() {
    if (checking) return;
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

  async function dismiss(finding: UpdateFinding) {
    try {
      snapshot = await updatesDismiss(finding.modId);
    } catch (reason) {
      error = errorText(reason);
    }
  }

  async function restore(finding: UpdateFinding) {
    try {
      snapshot = await updatesRestore(finding.modId);
    } catch (reason) {
      error = errorText(reason);
    }
  }

  async function confirmPage(suggestion: PageSuggestion) {
    try {
      snapshot = await updatesConfirmPage(suggestion.modId);
      toasts.show(`Следим за страницей «${suggestion.modName}»`, {
        undo: async () => {
          snapshot = await updatesUndoPage(suggestion);
        },
      });
    } catch (reason) {
      error = errorText(reason);
    }
  }

  async function rejectPage(suggestion: PageSuggestion) {
    try {
      snapshot = await updatesRejectPage(suggestion.modId);
      toasts.show(`Страница «${suggestion.modName}» отклонена`, {
        undo: async () => {
          snapshot = await updatesUndoPage(suggestion);
        },
      });
    } catch (reason) {
      error = errorText(reason);
    }
  }

  async function unfollow(finding: UpdateFinding) {
    try {
      snapshot = await updatesUnfollow(finding.modId, finding.source);
      toasts.show(`Больше не следим за этой страницей «${finding.modName}»`, {
        undo: async () => {
          snapshot = await updatesRefollow(finding.modId, finding.source);
        },
      });
    } catch (reason) {
      error = errorText(reason);
    }
  }

  /** «Страница пропала»: искать новую (сразу запускаем поиск) или просто перестать следить. */
  async function forgetLost(page: LostPage, searchAgain: boolean) {
    try {
      snapshot = await updatesForgetLost(page.modId, page.source, searchAgain);
      if (searchAgain) {
        await findPages();
      } else {
        toasts.show(`Больше не следим за «${page.modName}»`, {
          undo: async () => {
            snapshot = await updatesRefollow(page.modId, page.source);
          },
        });
      }
    } catch (reason) {
      error = errorText(reason);
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
    try {
      await openUrl(url);
    } catch {
      window.open(url, "_blank", "noopener");
    }
  }

  /** «Обновить»: страница мода и полчаса наблюдения за «Загрузками». */
  async function update(finding: UpdateFinding) {
    await openLink(finding.url);
    await expectDownload();
    toasts.show(
      "Скачайте архив на странице — программа заметит его в «Загрузках» и предложит установить",
    );
  }

  function versionText(finding: UpdateFinding): string {
    if (finding.installedVersion && finding.latestVersion) {
      return `${finding.installedVersion} → ${finding.latestVersion}`;
    }
    if (finding.latestVersion) return `версия ${finding.latestVersion}`;
    return "файл изменился";
  }

  function statusText(finding: UpdateFinding): string {
    return finding.status === "likely" ? "Обновление" : "Возможно";
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
    if (finding.noticedAt) parts.push(`новую версию заметили ${formatAgo(finding.noticedAt)}`);
    const size = sizeChange(finding);
    if (finding.status === "likely" && size) parts.push(`размер ${size}`);
    return parts.join(", ") + ".";
  }

  function siteRows(): { id: string; name: string; tone: string; text: string }[] {
    const live = new Map(sources.map((item) => [item.id, item]));
    return (snapshot.sites ?? []).map((site) => {
      const now = live.get(site.id);
      const human = site.status === "human_check" || now?.status === "needsHumanCheck";
      const down = site.status === "unavailable" || now?.status === "unavailable";
      if (site.status === "off") return { id: site.id, name: site.displayName, tone: "off", text: "выключен" };
      if (human) return { id: site.id, name: site.displayName, tone: "warn", text: "просит проверку" };
      if (down) return { id: site.id, name: site.displayName, tone: "bad", text: now?.note ?? "не отвечает" };
      if (site.tracked === 0) return { id: site.id, name: site.displayName, tone: "off", text: "нет модов" };
      return {
        id: site.id,
        name: site.displayName,
        tone: "ok",
        text: `следим за ${site.tracked} ${plural(site.tracked, "модом", "модами", "модами")}`,
      };
    });
  }
</script>

<div class="updates">
  <header class="top" data-tauri-drag-region>
    <div class="title" data-tauri-drag-region>
      <h1 class="display" data-tauri-drag-region>Обновления</h1>
      <p data-tauri-drag-region>
        {#if counts.available === 0}
          Всё актуально
        {:else}
          {counts.available} {plural(counts.available, "мод ждёт", "мода ждут", "модов ждут")} решения
        {/if}
      </p>
    </div>
    <div class="tabs" role="tablist" aria-label="Что показать">
      <button type="button" role="tab" class="tab" class:active={filter === "available"} aria-selected={filter === "available"} onclick={() => (filter = "available")}>
        Доступные <b>{counts.available}</b>
      </button>
      <button type="button" role="tab" class="tab" class:active={filter === "hidden"} aria-selected={filter === "hidden"} onclick={() => (filter = "hidden")}>
        Пропущенные <b>{counts.hidden}</b>
      </button>
      <button type="button" role="tab" class="tab" class:active={filter === "pages"} aria-selected={filter === "pages"} onclick={() => (filter = "pages")}>
        Страницы <b>{counts.pages}</b>
      </button>
    </div>
  </header>

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  <div class="body">
    <section class="list scroll-area">
      {#if loading}
        <div class="state"><p>Загружаем обновления…</p></div>
      {:else if filter === "available" && snapshot.findings.length === 0}
        <div class="state">
          <p>Обновлений нет</p>
          <span>
            {snapshot.lastChecked
              ? `Последняя проверка ${formatWhen(snapshot.lastChecked)}. Программа проверяет сама при запуске.`
              : "Нажмите «Проверить сейчас», чтобы сверить версии со всеми включёнными сайтами."}
          </span>
        </div>
      {:else if filter === "hidden" && snapshot.hidden.length === 0}
        <div class="state">
          <p>Скрытых версий нет</p>
          <span>Пропущенные версии появятся здесь — их можно вернуть в список.</span>
        </div>
      {:else if filter === "pages" && counts.pages === 0}
        <div class="state">
          <p>Решать нечего</p>
          <span>
            Если программа не уверена, что нашла страницу именно вашего мода, она спросит здесь.
          </span>
        </div>
      {:else if filter === "available" || filter === "hidden"}
        {#each filter === "available" ? snapshot.findings : snapshot.hidden as finding (finding.modId)}
          {@const possible = finding.status !== "likely"}
          <article class="card">
            <div class="photo">{#if finding.previewUrl}<img src={finding.previewUrl} alt="" loading="lazy" />{:else}<Car size={48} weight="thin" />{/if}</div>
            <div class="content">
              <div class="head">
                <h2>{finding.modName}</h2>
                {#if filter === "hidden"}
                  <span class="chip muted">Пропущено</span>
                {:else if possible}
                  <span class="chip possible">Возможно, обновлён</span>
                {:else}
                  <span class="chip update">Обновление</span>
                {/if}
              </div>
              {#if finding.installedVersion && finding.latestVersion && !possible}
                <p class="versions mono"><s>{finding.installedVersion}</s> → <b>{finding.latestVersion}</b></p>
              {:else if finding.latestVersion && !possible}
                <p class="versions mono"><b>{finding.latestVersion}</b></p>
              {/if}
              <p class="why">{reason(finding)}</p>
              <p class="where">{whereText(finding)}</p>
              <div class="actions">
                {#if filter === "hidden"}
                  <button class="btn-primary" type="button" onclick={() => void restore(finding)}>Вернуть в список</button>
                  <button class="btn-ghost" type="button" onclick={() => void openLink(finding.url)}>Открыть страницу</button>
                {:else if possible}
                  <button class="btn-primary" type="button" onclick={() => void update(finding)}>Открыть страницу</button>
                  <button class="btn-ghost" type="button" onclick={() => void dismiss(finding)}>Версия не изменилась</button>
                  <button class="btn-ghost" type="button" onclick={() => void unfollow(finding)}>Это другой мод</button>
                {:else}
                  <button class="btn-primary" type="button" onclick={() => void update(finding)}>Обновить</button>
                  <button class="btn-ghost" type="button" onclick={() => void openLink(finding.url)}>Открыть страницу</button>
                  <button class="btn-ghost" type="button" onclick={() => void dismiss(finding)}>Пропустить эту версию</button>
                  <button class="btn-ghost" type="button" onclick={() => void unfollow(finding)}>Это другой мод</button>
                {/if}
              </div>
            </div>
          </article>
        {/each}
      {:else}
        {#each snapshot.lost ?? [] as page (page.modId + page.source)}
          <article class="card compact">
            <div class="photo">{#if page.previewUrl}<img src={page.previewUrl} alt="" loading="lazy" />{:else}<Car size={48} weight="thin" />{/if}</div>
            <div class="content">
              <div class="head"><h2>{page.modName}</h2><span class="chip lost">Страница пропала</span></div>
              <p class="why">
                На {sourceName(page.source)} этой страницы нет уже несколько проверок: мод удалили или перенесли.
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
          <article class="card compact">
            <div class="photo"><Car size={48} weight="thin" /></div>
            <div class="content">
              <div class="head"><h2>{suggestion.modName}</h2><span class="chip possible">Это его страница?</span></div>
              <p class="why">
                На {sourceName(suggestion.source)} нашлась похожая страница, но уверенности нет: проверьте её.
              </p>
              <p class="where url" title={suggestion.url}>{suggestion.url}</p>
              <div class="actions">
                <button class="btn-primary" type="button" onclick={() => void confirmPage(suggestion)}>Следить</button>
                <button class="btn-ghost" type="button" onclick={() => void openLink(suggestion.url)}>Открыть страницу</button>
                <button class="btn-ghost" type="button" onclick={() => void rejectPage(suggestion)}>Это другой мод</button>
              </div>
            </div>
          </article>
        {/each}
      {/if}
    </section>

    <aside class="panel">
      <h2 class="display">Проверка</h2>
      <p class="sub">
        {#if checking}
          {progressText()}
        {:else if autoProgress}
          Проверка при запуске: {progressText(autoProgress)}
        {:else if snapshot.lastChecked}
          Последняя {formatWhen(snapshot.lastChecked)}. Окно можно закрыть: найденное сохранится.
        {:else}
          Ещё не проверяли. Проверим моды, за которыми следим на сайтах.
        {/if}
      </p>
      {#if checking || autoProgress}
        {@const shown = checking ? progress : autoProgress}
        <div class="track"><span class="bar" class:pulse={!shown?.total} style={shown?.total ? `width:${Math.round((shown.done / shown.total) * 100)}%` : undefined}></span></div>
        <button class="btn-glass wide" type="button" onclick={() => void cancel()}>Остановить</button>
      {:else}
        <button class="btn-primary wide" type="button" onclick={() => void check()}>
          {offerForce ? "Всё равно проверить" : "Проверить сейчас"}
        </button>
        <button class="btn-glass wide" type="button" onclick={() => void findPages()}>
          Найти страницы модам без них
        </button>
      {/if}

      <h2 class="display sites-title">Сайты</h2>
      <div class="sites">
        {#each siteRows() as site (site.id)}
          <div class="site">
            <span class="dot {site.tone}"></span>
            <b>{site.name}</b>
            <span class="state-text">{site.text}</span>
          </div>
          {#if site.id === "modland" && site.tone === "warn"}
            <button class="btn-glass pass" type="button" disabled={passing} onclick={() => void openModland()}>
              {passing ? "Ждём…" : "Пройти проверку Modland"}
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

  .tab::after {
    content: "";
    position: absolute;
    left: 10px;
    right: 10px;
    bottom: -1px;
    height: 2px;
    border-radius: 2px;
    background: var(--accent-brand);
    transform: scaleX(0);
    transition: transform var(--dur-3) var(--ease-out);
  }

  .tab:hover {
    color: var(--foreground);
  }

  .tab.active {
    color: var(--foreground);
  }

  .tab.active::after {
    transform: scaleX(1);
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
    height: 38px;
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

  .site + .site,
  .pass + .site {
    border-top: 1px solid var(--border);
  }

  .site b {
    font-weight: 700;
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
    background: #f2b544;
  }

  .dot.bad {
    background: var(--danger);
  }

  .state-text {
    margin-left: auto;
    color: var(--muted-foreground);
    font-size: 12px;
    text-align: right;
  }

  .pass {
    margin: 0 12px 10px;
    height: 32px;
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

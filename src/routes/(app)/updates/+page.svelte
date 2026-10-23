<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { Check, ExternalLink, EyeOff, RefreshCw, Search } from "@lucide/svelte";
  import { sourceName } from "$lib/ipc/search";
  import { plural } from "$lib/features/garage/format";
  import { toasts } from "$lib/shell/toasts.svelte";
  import {
    listenUpdates,
    updatesCancel,
    updatesConfirmPage,
    updatesDismiss,
    updatesFindPages,
    updatesOpenModland,
    updatesRejectPage,
    updatesRestore,
    updatesSnapshot,
    updatesStart,
    updatesUndoPage,
    type PageSuggestion,
    type UpdateFinding,
    type UpdateProgress,
    type UpdateSource,
    type UpdatesEvent,
    type UpdatesSnapshot,
  } from "$lib/ipc/updates";

  type Filter = "available" | "hidden" | "pages";

  let snapshot = $state<UpdatesSnapshot>({ findings: [], hidden: [], suggestions: [] });
  let sources = $state<UpdateSource[]>([]);
  let loading = $state(true);
  let checking = $state(false);
  let passing = $state(false);
  let filter = $state<Filter>("available");
  let progress = $state<UpdateProgress | null>(null);
  let error = $state<string | null>(null);
  // Обычная проверка не трогает моды, проверенные недавно (срок — в настройках). Если она
  // никуда не сходила, следующее нажатие проверяет всё заново, как в legacy.
  let offerForce = $state(false);
  let requestId = 0;
  let unlisten: (() => void) | undefined;

  const counts = $derived({
    available: snapshot.findings.length,
    hidden: snapshot.hidden.length,
    pages: snapshot.suggestions.length,
  });

  onMount(() => {
    void load();
    void listenUpdates(onEvent).then((stop) => {
      unlisten = stop;
    });
    return () => unlisten?.();
  });

  onDestroy(() => {
    unlisten?.();
    if (checking) void updatesCancel();
  });

  function onEvent(event: UpdatesEvent) {
    if (event.phase === "auto") {
      // Фоновая проверка закончилась: показываем найденное, если сами сейчас не проверяем.
      if (!checking) {
        snapshot = { findings: event.findings, hidden: event.hidden, suggestions: event.suggestions };
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
      findings: event.findings,
      hidden: event.hidden,
      suggestions: event.suggestions,
    };
    if (event.sources.length > 0) sources = event.sources;
    checking = false;
    progress = null;
    if (event.phase === "done") {
      offerForce = !event.cancelled && event.requests === 0;
      if (offerForce) toasts.show("Всё проверено недавно. Нажмите ещё раз, чтобы проверить заново");
    }
    if (event.phase === "pages") filter = "pages";
    if (found > 0) toasts.show(`Нашли новые страницы (${found})`);
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

  function progressText(): string {
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
</script>

<div class="updates">
  <header class="head">
    <div>
      <h1>Обновления</h1>
      <p>
        {#if checking}
          {progressText()}
        {:else}
          {counts.available}
          доступно
          · {counts.hidden} скрыто · {counts.pages}
          {plural(counts.pages, "страница", "страницы", "страниц")}
          {plural(counts.pages, "ждёт", "ждут", "ждут")} решения
        {/if}
      </p>
    </div>
    <div class="head-actions">
      {#if checking}
        <button class="ghost glass liquid-edge" type="button" onclick={() => void cancel()}>
          Отменить
        </button>
      {:else}
        <button class="ghost glass liquid-edge" type="button" onclick={() => void findPages()}>
          <Search size={14} />
          Найти страницы
        </button>
        <button class="refresh glass liquid-edge" type="button" onclick={() => void check()}>
          <RefreshCw size={14} />
          {offerForce ? "Всё равно проверить" : "Проверить"}
        </button>
      {/if}
    </div>
  </header>

  {#if checking}
    <div class="progress glass liquid-edge">
      <div class="track"><span class="bar" class:pulse={!progress?.total}></span></div>
    </div>
  {/if}

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  {#if sources.length > 0}
    <div class="sites">
      {#each sources as source (source.id)}
        <div
          class="site"
          class:warn={source.status !== "ok"}
          class:bad={source.status === "unavailable"}
          title={source.note ?? source.displayName}
        >
          <span class="dot"></span>
          <b>{source.displayName}</b>
          <span>
            {#if source.status === "needsHumanCheck"}
              проверка
            {:else if source.status === "unavailable"}
              недоступен
            {:else}
              {source.cardsFetched} карточек
            {/if}
          </span>
          {#if source.id === "modland" && source.status === "needsHumanCheck"}
            <button type="button" disabled={passing} onclick={() => void openModland()}>
              {passing ? "Ждём…" : "Пройти проверку"}
            </button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  <div class="filters">
    <button
      type="button"
      class:active={filter === "available"}
      onclick={() => (filter = "available")}
    >
      Доступны <span>{counts.available}</span>
    </button>
    <button
      type="button"
      class:active={filter === "hidden"}
      onclick={() => (filter = "hidden")}
    >
      Скрытые <span>{counts.hidden}</span>
    </button>
    <button
      type="button"
      class:active={filter === "pages"}
      onclick={() => (filter = "pages")}
    >
      Страницы <span>{counts.pages}</span>
    </button>
  </div>

  {#if loading}
    <div class="state glass liquid-edge"><p>Загружаем обновления…</p></div>
  {:else if filter === "available" && snapshot.findings.length === 0}
    <div class="state glass liquid-edge">
      <p>Обновлений нет</p>
      <span>Нажмите «Проверить», чтобы сверить версии со всеми включёнными сайтами.</span>
    </div>
  {:else if filter === "hidden" && snapshot.hidden.length === 0}
    <div class="state glass liquid-edge">
      <p>Скрытых версий нет</p>
      <span>Всё, что вы пропустили, появится здесь — версию можно вернуть в список.</span>
    </div>
  {:else if filter === "pages" && snapshot.suggestions.length === 0}
    <div class="state glass liquid-edge">
      <p>Страницы не подобраны</p>
      <span>Нажмите «Найти страницы», чтобы поискать страницы модов, у которых их ещё нет.</span>
    </div>
  {:else if filter === "available"}
    <div class="list">
      {#each snapshot.findings as finding (finding.modId)}
        <article class="row glass liquid-edge">
          <div class="main">
            <div class="title-line">
              <h2>{finding.modName}</h2>
              <span class="status {finding.status}">{statusText(finding)}</span>
            </div>
            <p>
              <span class="source">{sourceName(finding.source)}</span>
              · {versionText(finding)}
            </p>
          </div>
          <div class="actions">
            <button type="button" onclick={() => void openLink(finding.url)}>
              <ExternalLink size={13} />
              Открыть
            </button>
            <button class="muted" type="button" onclick={() => void dismiss(finding)}>
              <EyeOff size={13} />
              Скрыть
            </button>
          </div>
        </article>
      {/each}
    </div>
  {:else if filter === "hidden"}
    <div class="list">
      {#each snapshot.hidden as finding (finding.modId)}
        <article class="row glass liquid-edge hidden">
          <div class="main">
            <div class="title-line">
              <h2>{finding.modName}</h2>
              <span class="status">Скрыто</span>
            </div>
            <p>
              <span class="source">{sourceName(finding.source)}</span>
              · {versionText(finding)}
            </p>
          </div>
          <div class="actions">
            <button type="button" onclick={() => void openLink(finding.url)}>
              <ExternalLink size={13} />
              Открыть
            </button>
            <button type="button" onclick={() => void restore(finding)}>
              <RefreshCw size={13} />
              Вернуть
            </button>
          </div>
        </article>
      {/each}
    </div>
  {:else}
    <div class="list">
      {#each snapshot.suggestions as suggestion (suggestion.modId + suggestion.source)}
        <article class="row glass liquid-edge">
          <div class="main">
            <div class="title-line">
              <h2>{suggestion.modName}</h2>
              <span class="status possible">Страница?</span>
            </div>
            <p>
              <span class="source">{sourceName(suggestion.source)}</span>
              · <span class="url">{suggestion.url}</span>
            </p>
          </div>
          <div class="actions">
            <button type="button" onclick={() => void openLink(suggestion.url)}>
              <ExternalLink size={13} />
              Проверить
            </button>
            <button class="ok" type="button" onclick={() => void confirmPage(suggestion)}>
              <Check size={13} />
              Следить
            </button>
            <button class="muted" type="button" onclick={() => void rejectPage(suggestion)}>
              Это другой мод
            </button>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .updates {
    display: flex;
    flex-direction: column;
    gap: 13px;
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

  .head-actions {
    display: flex;
    gap: 8px;
  }

  .refresh,
  .ghost {
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

  .refresh {
    background: color-mix(in oklab, var(--primary) 16%, transparent);
  }

  .ghost:disabled,
  .refresh:disabled {
    opacity: 0.55;
  }

  .progress {
    padding: 7px 10px;
    border-radius: 12px;
  }

  .track {
    height: 3px;
    overflow: hidden;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
  }

  .bar {
    display: block;
    width: 36%;
    height: 100%;
    border-radius: inherit;
    background: var(--primary);
    animation: slide 1.2s ease-in-out infinite;
  }

  .bar.pulse {
    width: 55%;
    opacity: 0.7;
  }

  .banner {
    padding: 9px 12px;
    border-radius: 12px;
    font-size: 10.5px;
  }

  .banner.error {
    border: 1px solid color-mix(in oklab, oklch(0.75 0.16 25) 35%, transparent);
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
    border-color: var(--warning-border);
  }

  .site.warn .dot {
    background: var(--warning);
  }

  .site.bad {
    border-color: color-mix(in oklab, oklch(0.75 0.16 25) 35%, transparent);
  }

  .site.bad .dot {
    background: oklch(0.75 0.16 25);
  }

  .site button {
    height: 19px;
    padding: 0 7px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: transparent;
    color: var(--foreground);
    font-size: 9px;
    cursor: default;
  }

  .site button:disabled {
    opacity: 0.55;
  }

  .filters {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }

  .filters button {
    height: 28px;
    padding: 0 11px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 10px;
    cursor: default;
  }

  .filters button.active {
    background: color-mix(in oklab, var(--primary) 14%, transparent);
    color: var(--foreground);
    border-color: color-mix(in oklab, var(--primary) 45%, transparent);
  }

  .filters span {
    opacity: 0.65;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 7px;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 14px;
    border-radius: 15px;
  }

  .row.hidden {
    opacity: 0.72;
  }

  .main {
    min-width: 0;
  }

  .title-line {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  h2 {
    margin: 0;
    font-size: 13.5px;
    font-weight: 620;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .status {
    flex: none;
    padding: 2px 7px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
    color: var(--muted-foreground);
    font-size: 8.5px;
  }

  .status.likely {
    color: var(--primary);
    background: color-mix(in oklab, var(--primary) 13%, transparent);
  }

  .status.possible {
    color: var(--warning);
    background: var(--warning-soft);
  }

  .row p {
    margin: 4px 0 0;
    color: var(--muted-foreground);
    font-size: 10px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .source {
    color: var(--primary);
  }

  .url {
    font-family: ui-monospace, monospace;
    opacity: 0.85;
  }

  .actions {
    display: flex;
    flex: none;
    align-items: center;
    gap: 5px;
  }

  .actions button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 25px;
    padding: 0 9px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: transparent;
    color: var(--foreground);
    font-size: 9px;
    cursor: default;
  }

  .actions button:hover {
    border-color: color-mix(in oklab, var(--foreground) 30%, transparent);
  }

  .actions .muted {
    color: var(--muted-foreground);
  }

  .actions .ok {
    color: oklch(0.83 0.14 150);
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

  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(300%);
    }
  }

  @media (max-width: 760px) {
    .row {
      flex-direction: column;
      align-items: stretch;
    }
    .actions {
      flex-wrap: wrap;
    }
  }
</style>

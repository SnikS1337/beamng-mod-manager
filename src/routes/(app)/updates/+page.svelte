<script lang="ts">
  import { expectDownload } from "$lib/ipc/incoming";
  import { onDestroy, onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { sourceName } from "$lib/ipc/search";
  import { formatAgo, formatSize, formatWhen, plural } from "$lib/features/garage/format";
  import { toasts } from "$lib/shell/toasts.svelte";
  import { checkSummary } from "$lib/features/updates/summary";
  import { reason, sizeChange } from "$lib/features/updates/reason";
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
    pages: snapshot.suggestions.length,
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
  }

  // Панель «Проверка»: состояние сайтов и время последней проверки — из снимка.
  async function refreshSites() {
    try {
      const fresh = await updatesSnapshot();
      snapshot = { ...snapshot, sites: fresh.sites, lastChecked: fresh.lastChecked };
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

  function placeholder(id: string): string {
    let hash = 0;
    for (let index = 0; index < id.length; index += 1) hash = (hash * 31 + id.charCodeAt(index)) % 360;
    return `radial-gradient(120% 90% at 18% 0%, hsl(${hash} 70% 60% / 0.7), transparent 62%), linear-gradient(135deg, hsl(${hash} 38% 28%), hsl(${(hash + 58) % 360} 42% 16%))`;
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
      <h1 data-tauri-drag-region>Обновления</h1>
      <p data-tauri-drag-region>
        {#if counts.available === 0}
          Всё актуально
        {:else}
          {counts.available} {plural(counts.available, "доступно", "доступно", "доступно")}
        {/if}
      </p>
    </div>
    <div class="island glass liquid-edge">
      <div class="filters">
        <button type="button" class="filter" class:active={filter === "available"} onclick={() => (filter = "available")}>
          Доступные {#if counts.available > 0}<span>{counts.available}</span>{/if}
        </button>
        <button type="button" class="filter" class:active={filter === "hidden"} onclick={() => (filter = "hidden")}>
          Скрытые {#if counts.hidden > 0}<span>{counts.hidden}</span>{/if}
        </button>
        <button type="button" class="filter" class:active={filter === "pages"} onclick={() => (filter = "pages")}>
          Страницы {#if counts.pages > 0}<span>{counts.pages}</span>{/if}
        </button>
      </div>
    </div>
    <div></div>
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
      {:else if filter === "pages" && snapshot.suggestions.length === 0}
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
            <div class="photo" style={finding.previewUrl ? `background-image:url('${finding.previewUrl}')` : `background:${placeholder(finding.modId)}`}></div>
            <div class="content">
              {#if possible}
                <span class="chip"><i></i>Возможно, обновлён</span>
              {/if}
              <h2>{finding.modName}</h2>
              {#if finding.installedVersion && finding.latestVersion && !possible}
                <p class="versions"><s>{finding.installedVersion}</s> → <b>{finding.latestVersion}</b></p>
              {:else if finding.latestVersion && !possible}
                <p class="versions"><b>{finding.latestVersion}</b></p>
              {/if}
              <p class="why">{reason(finding)}</p>
              <p class="where">{whereText(finding)}</p>
              <div class="actions">
                {#if filter === "hidden"}
                  <button class="btn-primary" type="button" onclick={() => void restore(finding)}>Вернуть в список</button>
                  <button class="link" type="button" onclick={() => void openLink(finding.url)}>Открыть страницу</button>
                {:else if possible}
                  <button class="btn-primary" type="button" onclick={() => void update(finding)}>Открыть страницу</button>
                  <button class="link" type="button" onclick={() => void dismiss(finding)}>Версия не изменилась</button>
                  <button class="link" type="button" onclick={() => void unfollow(finding)}>Это другой мод</button>
                {:else}
                  <button class="btn-primary" type="button" onclick={() => void update(finding)}>Обновить</button>
                  <button class="link" type="button" onclick={() => void openLink(finding.url)}>Открыть страницу</button>
                  <button class="link" type="button" onclick={() => void dismiss(finding)}>Пропустить эту версию</button>
                  <button class="link" type="button" onclick={() => void unfollow(finding)}>Это другой мод</button>
                {/if}
              </div>
            </div>
          </article>
        {/each}
      {:else}
        {#each snapshot.suggestions as suggestion (suggestion.modId + suggestion.source)}
          <article class="card compact">
            <div class="photo" style={`background:${placeholder(suggestion.modId)}`}></div>
            <div class="content">
              <span class="chip"><i></i>Это его страница?</span>
              <h2>{suggestion.modName}</h2>
              <p class="why">
                На {sourceName(suggestion.source)} нашлась похожая страница, но уверенности нет: проверьте её.
              </p>
              <p class="where url" title={suggestion.url}>{suggestion.url}</p>
              <div class="actions">
                <button class="btn-primary" type="button" onclick={() => void confirmPage(suggestion)}>Следить</button>
                <button class="link" type="button" onclick={() => void openLink(suggestion.url)}>Открыть страницу</button>
                <button class="link" type="button" onclick={() => void rejectPage(suggestion)}>Это другой мод</button>
              </div>
            </div>
          </article>
        {/each}
      {/if}
    </section>

    <aside class="panel glass liquid-edge">
      <h2>Проверка</h2>
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

      <h3>Сайты</h3>
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
    gap: 14px;
    height: 100%;
    min-height: 0;
  }

  .top {
    display: grid;
    grid-template-columns: minmax(170px, 1fr) auto minmax(0, 1fr);
    align-items: center;
    gap: 16px;
    min-height: 52px;
    padding-right: 148px;
  }

  .title {
    min-width: 0;
  }

  h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 700;
    line-height: 1.1;
    letter-spacing: -0.02em;
  }

  .title p {
    margin: 3px 0 0;
    color: color-mix(in oklab, var(--foreground) 66%, transparent);
    font-size: 13px;
    white-space: nowrap;
  }

  .island {
    display: flex;
    align-items: center;
    height: 52px;
    padding: 0 8px;
    border-radius: 26px;
  }

  .filters {
    display: flex;
    gap: 2px;
    height: 35px;
    padding: 3px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 6%, transparent);
  }

  .filter {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 29px;
    padding: 0 14px;
    border: 0;
    border-radius: 999px;
    background: transparent;
    color: color-mix(in oklab, var(--foreground) 74%, transparent);
    font-size: 13px;
    font-weight: 550;
    cursor: default;
    transition:
      background 220ms ease,
      color 220ms ease;
  }

  .filter span {
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

  .banner.error {
    padding: 9px 14px;
    border: 1px solid oklch(0.66 0.19 25 / 0.45);
    border-radius: 16px;
    background: oklch(0.66 0.19 25 / 0.1);
    font-size: 13px;
  }

  .body {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 328px;
    gap: 16px;
    flex: 1;
    min-height: 0;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 18px;
    min-height: 0;
    padding: 2px 4px 16px 2px;
    overflow-y: auto;
  }

  /* Карточка обновления legacy: фото слева, справа версия крупно, объяснение и действия. */
  .card {
    display: grid;
    grid-template-columns: minmax(200px, 38%) minmax(0, 1fr);
    min-height: 196px;
    border: 1px solid rgb(255 255 255 / 0.16);
    border-radius: 26px;
    background: rgb(12 16 30 / 0.86);
    color: #fff;
    box-shadow:
      inset 0 1px 0 rgb(255 255 255 / 0.22),
      0 16px 32px rgb(8 12 24 / 0.22);
    overflow: hidden;
  }

  .card.compact {
    min-height: 170px;
  }

  .photo {
    background-position: center;
    background-size: cover;
    mask-image: linear-gradient(90deg, #000 72%, transparent);
    -webkit-mask-image: linear-gradient(90deg, #000 72%, transparent);
  }

  .content {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    min-width: 0;
    padding: 22px 24px 20px 6px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 24px;
    padding: 0 10px;
    border: 1px solid rgb(255 255 255 / 0.16);
    border-radius: 999px;
    background: rgb(255 255 255 / 0.06);
    font-size: 12px;
    font-weight: 700;
  }

  .chip i {
    width: 7px;
    height: 7px;
    border-radius: 999px;
    background: #ffb340;
    box-shadow: 0 0 8px #ffb340;
  }

  .card h2 {
    margin: 0;
    max-width: 100%;
    overflow: hidden;
    font-size: 15px;
    font-weight: 620;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .versions {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
    letter-spacing: -0.01em;
    font-variant-numeric: tabular-nums;
  }

  .versions s {
    color: rgb(255 255 255 / 0.55);
    font-weight: 600;
  }

  .why,
  .where {
    margin: 0;
    max-width: 100%;
    font-size: 13px;
    line-height: 1.4;
  }

  .where {
    color: rgb(255 255 255 / 0.66);
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
    align-items: center;
    gap: 6px 18px;
    margin-top: auto;
    padding-top: 10px;
  }

  .actions .btn-primary {
    background: #fff;
    color: #111;
    box-shadow: 0 6px 22px rgb(170 205 255 / 0.35);
  }

  .link {
    padding: 0;
    border: 0;
    background: transparent;
    color: #fff;
    font-size: 13px;
    font-weight: 620;
    cursor: default;
  }

  .link:hover {
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .state {
    display: grid;
    place-content: center;
    gap: 6px;
    min-height: 260px;
    color: color-mix(in oklab, var(--foreground) 64%, transparent);
    text-align: center;
  }

  .state p {
    margin: 0;
    color: var(--foreground);
    font-size: 17px;
    font-weight: 650;
  }

  .state span {
    max-width: 440px;
    font-size: 13px;
    line-height: 1.45;
  }

  /* Панель «Проверка» справа — как в legacy. */
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
    letter-spacing: -0.01em;
  }

  .sub {
    margin: 0 0 6px;
    color: color-mix(in oklab, var(--foreground) 70%, transparent);
    font-size: 13px;
    line-height: 1.4;
  }

  .wide {
    width: 100%;
    height: 42px;
    font-size: 14px;
  }

  .panel h3 {
    margin: 10px 2px 0;
    font-size: 13px;
    font-weight: 650;
  }

  .sites {
    display: flex;
    flex-direction: column;
    padding: 4px 14px;
    border-radius: 16px;
    background: color-mix(in oklab, var(--foreground) 7%, transparent);
  }

  .site {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 0;
    font-size: 13px;
  }

  .site + .site {
    border-top: 1px solid color-mix(in oklab, var(--foreground) 7%, transparent);
  }

  .site b {
    flex: 1;
    font-weight: 600;
  }

  .state-text {
    color: color-mix(in oklab, var(--foreground) 62%, transparent);
    font-size: 12.5px;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 999px;
    background: #34c759;
  }

  .dot.warn {
    background: #ffb340;
  }

  .dot.bad {
    background: #ff6b5e;
  }

  .dot.off {
    background: color-mix(in oklab, var(--foreground) 30%, transparent);
  }

  .pass {
    width: 100%;
    height: 32px;
    margin: 2px 0 8px;
    font-size: 12.5px;
  }

  .track {
    height: 6px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 10%, transparent);
    overflow: hidden;
  }

  .bar {
    display: block;
    width: 30%;
    height: 100%;
    border-radius: 999px;
    background: var(--primary);
    transition: width 0.4s ease;
  }

  .bar.pulse {
    animation: slide 1.4s ease-in-out infinite;
  }

  @keyframes slide {
    0% {
      transform: translateX(-100%);
    }
    100% {
      transform: translateX(340%);
    }
  }

  @media (max-width: 1100px) {
    .body {
      grid-template-columns: minmax(0, 1fr);
    }

    .panel {
      order: -1;
    }
  }
</style>

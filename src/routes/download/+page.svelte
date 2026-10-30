<script lang="ts">
  import { onMount } from "svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { emit } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Check from "phosphor-svelte/lib/Check";
  import Lock from "phosphor-svelte/lib/Lock";
  import Package from "phosphor-svelte/lib/Package";
  import PuzzlePiece from "phosphor-svelte/lib/PuzzlePiece";
  import X from "phosphor-svelte/lib/X";
  import { expectDownload } from "#lib/ipc/incoming.js";
  import {
    archiveBaseName,
    incomingDiscard,
    incomingInspect,
    incomingInstall,
    incomingRemoveSource,
    type IncomingArchiveDto,
    type IncomingItemDto,
    type IncomingTicket,
  } from "#lib/ipc/incoming.js";
  import {
    installable,
    preselected,
    suspiciousFiles,
    suspiciousText,
  } from "#lib/features/incoming/selection.js";
  import { formatSize, plural } from "#lib/features/garage/format.js";
  import { safeExternalUrl } from "#lib/url.js";
  import {
    downloadClose,
    downloadPanelState,
    downloadReady,
    listenDownloadContext,
    listenDownloadProgress,
    listenDownloadReady,
    listenDownloadState,
    type DownloadProgress,
    type DownloadRequest,
  } from "#lib/ipc/download.js";
  import { progressView } from "#lib/features/download/progress.js";

  let context = $state<DownloadRequest | null>(null);
  let step = $state(1);
  let status = $state("Нажмите Download на странице");
  let detail = $state("Скачайте архив на странице — остальное программа сделает сама.");
  let error = $state<string | null>(null);
  let archive = $state<IncomingArchiveDto | null>(null);
  let selected = $state<boolean[]>([]);
  let inspecting = $state(false);
  let installing = $state(false);
  let installError = $state<string | null>(null);
  let handledPath = "";
  let progress = $state<DownloadProgress | null>(null);
  let changedAt = $state(0);
  let now = $state(Date.now());

  // Ход видно, пока файл качается; после него — проверка и установка.
  const loading = $derived(
    step === 2 && !error && progress !== null ? progressView(progress, changedAt, now) : null,
  );

  function onProgress(value: DownloadProgress): void {
    if (!progress || value.received !== progress.received) changedAt = Date.now();
    progress = value;
  }

  const steps = [
    {
      id: 1,
      title: "Нажмите Download",
      text: "Если сайт просит подождать или пройти проверку, сделайте это там же.",
    },
    {
      id: 2,
      title: "Файл придёт сам",
      text: "Менеджер заберёт готовую загрузку.",
    },
    {
      id: 3,
      title: "Проверка архива",
      text: "Структура, опасные пути, подозрительные файлы.",
    },
    {
      id: 4,
      title: "Установка",
      text: "Прошлая версия останется в копиях.",
    },
  ];

  const versionText = $derived.by(() => {
    const from = context?.installedVersion;
    const to = context?.latestVersion;
    if (from && to) return `обновление с ${from} до ${to}`;
    if (to) return `обновление до ${to}`;
    return "обновление мода";
  });

  const address = $derived.by(() => {
    try {
      const url = new URL(context?.pageUrl ?? "");
      return `${url.host}${url.pathname}`;
    } catch {
      return context?.pageUrl ?? "";
    }
  });

  const items = $derived(archive?.items ?? []);
  const archiveSize = $derived(items.reduce((sum, item) => sum + item.size, 0));
  const picked = $derived(
    items.filter((item, index) => selected[index] && installable(item)),
  );
  const showChecks = $derived(
    items.length > 1 || items.some((item) => suspiciousFiles(item).length > 0),
  );
  const archiveTitle = $derived(archive ? archiveBaseName(archive.source) : "");
  const installLabel = $derived.by(() => {
    if (picked.length > 1) {
      return `Установить ${picked.length} ${plural(picked.length, "мод", "мода", "модов")}`;
    }
    return picked.length === 1 && picked[0].kind === "update" ? "Обновить" : "Установить";
  });

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось выполнить действие.";
  }

  function resetArchive(discard = true): void {
    const work = archive?.workDir;
    if (discard && work) void incomingDiscard(work).catch(() => {});
    archive = null;
    selected = [];
    installError = null;
  }

  let prepareToken = 0;

  async function prepare(path: string): Promise<void> {
    if (!path || path === handledPath || installing) return;
    handledPath = path;
    const token = ++prepareToken;
    resetArchive();
    inspecting = true;
    installError = null;
    step = 3;
    status = "Проверяем архив";
    detail = "Смотрим структуру, опасные пути и подозрительные файлы.";
    try {
      const [found] = await incomingInspect([path]);
      if (token !== prepareToken) {
        if (found) void incomingDiscard(found.workDir).catch(() => {});
        return;
      }
      if (!found) throw new Error("Не удалось проверить архив.");
      archive = found;
      selected = found.items.map(preselected);
      if (found.problem) {
        status = "Архив не подошёл";
        detail = found.problem;
      } else if (!found.items.some(installable)) {
        status = "Устанавливать нечего";
        detail = "В архиве нет подходящих модов.";
      } else {
        step = 4;
        status = "Архив проверен";
        detail = "Можно установить — прошлая версия останется в копиях.";
      }
    } catch (reason) {
      if (token === prepareToken) {
        handledPath = "";
        installError = errorText(reason);
        status = "Не удалось проверить архив";
        detail = installError;
      }
    } finally {
      if (token === prepareToken) inspecting = false;
    }
  }

  function toggle(index: number, event: Event): void {
    selected[index] = (event.currentTarget as HTMLInputElement).checked;
  }

  function kindText(item: IncomingItemDto): string {
    if (item.kind === "alreadyInstalled") {
      return `Уже установлен (${item.replaces ?? item.fileName})`;
    }
    if (item.kind === "update") {
      return item.replaces &&
        item.replaces.toLowerCase() !== item.fileName.toLowerCase()
        ? `Обновление: заменит ${item.replaces}`
        : "Обновление";
    }
    return "Новый мод";
  }

  function itemChecks(item: IncomingItemDto): string[] {
    const checks: string[] = [];
    if (item.blocked) checks.push("Есть опасные файлы или пути");
    else if (item.warnings) checks.push("Есть предупреждения");
    else checks.push("Опасных файлов и путей нет");
    const files = suspiciousFiles(item);
    if (files.length > 0) checks.push(`Сомнительные файлы: ${suspiciousText(files)}`);
    return checks;
  }

  function successText(count: number): string {
    const name = context?.modName ?? "мод";
    if (count === 1) {
      return picked[0]?.kind === "update" ? `Обновлён «${name}»` : `Установлен «${name}»`;
    }
    return `Установлено ${count} ${plural(count, "мод", "мода", "модов")}`;
  }

  async function install(): Promise<void> {
    if (!archive || picked.length === 0 || installing || inspecting) return;
    installing = true;
    installError = null;
    step = 4;
    status = "Устанавливаем";
    detail = "Заменяем архив и сохраняем прошлую версию.";
    try {
      const results = await incomingInstall(
        picked.map((item) => ({
          zipPath: item.zipPath,
          fileName: item.fileName,
          replaces: item.replaces,
          allowWarnings: true,
        })),
      );
      const tickets: IncomingTicket[] = [];
      const failures: { title: string; message: string }[] = [];
      let warning: string | null = null;
      results.forEach((result, index) => {
        if (result.success) {
          if (result.warning) warning = result.warning;
          tickets.push({
            fileName: result.fileName,
            replaces: result.replaces,
            backupId: result.backupId,
            wasDisabled: result.wasDisabled,
            sameName: result.sameName,
          });
        } else {
          failures.push({
            title: picked[index]?.title ?? result.fileName,
            message: result.message ?? "установить не получилось",
          });
        }
      });
      const leftInArchive = archive.items.some(
        (item, index) => installable(item) && !selected[index],
      );
      if (tickets.length > 0 && failures.length === 0 && !leftInArchive) {
        void incomingRemoveSource(archive.source).catch(() => {});
      }
      if (tickets.length > 0) {
        await emit("download://installed", {
          tickets,
          fileNames: tickets.map((ticket) => ticket.fileName),
          message: successText(tickets.length),
          warning,
        }).catch(() => {});
      }
      if (failures.length > 0 || tickets.length === 0) {
        installError = failures[0]?.message ?? "Не удалось установить архив.";
        status = "Установка не удалась";
        detail = installError;
        installing = false;
        return;
      }
      status = "Установлено";
      detail = warning ?? "Библиотека обновлена, прошлая версия осталась в копиях.";
      await new Promise((resolve) => setTimeout(resolve, 700));
      installing = false;
      await close();
    } catch (reason) {
      installError = errorText(reason);
      status = "Установка не удалась";
      detail = installError;
      installing = false;
    }
  }

  async function close(): Promise<void> {
    if (installing) return;
    const work = archive?.workDir;
    archive = null;
    selected = [];
    handledPath = "";
    if (work) void incomingDiscard(work).catch(() => {});
    await downloadClose().catch(() => {});
  }

  async function openInBrowser(): Promise<void> {
    const url = safeExternalUrl(context?.pageUrl ?? "");
    if (!url) return;
    try {
      await openUrl(url);
    } catch {
      return;
    }
    await expectDownload();
    await close();
  }

  onMount(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    void downloadPanelState()
      .then((value) => {
        if (!disposed && value) context = value;
      })
      .catch(() => {});
    // Раз в секунду: заметить, что загрузка встала, даже если событий больше нет.
    const ticker = setInterval(() => {
      if (progress) now = Date.now();
    }, 1000);
    stops.push(() => clearInterval(ticker));
    void listenDownloadProgress((value) => {
      if (!disposed) onProgress(value);
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void listenDownloadContext((value) => {
      context = value;
      progress = null;
      handledPath = "";
      resetArchive();
      step = 1;
      status = "Нажмите Download на странице";
      detail = "Скачайте архив на странице — остальное программа сделает сама.";
      error = null;
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void listenDownloadState((value) => {
      // Новая загрузка начинается с нуля: прошлый ход не должен мелькнуть.
      if (value.step !== 2 || value.error) progress = null;
      if (value.step === 2 && step !== 2) progress = null;
      step = value.step;
      status = value.title;
      detail = value.detail;
      error = value.error ?? null;
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void listenDownloadReady((ready) => {
      if (!disposed) void prepare(ready.path);
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void downloadReady()
      .then((path) => {
        if (!disposed && path) void prepare(path);
      })
      .catch(() => {});
    return () => {
      disposed = true;
      for (const stop of stops) stop();
    };
  });
</script>

<svelte:head>
  <title>Скачивание</title>
</svelte:head>

<div class="page">
  <header class="head" data-tauri-drag-region>
    <div class="cover">
      {#if context?.previewUrl && context?.role !== "addon"}
        <img src={context.previewUrl} alt="" />
      {:else if context?.role === "addon"}
        <PuzzlePiece size={26} weight="thin" />
      {:else}
        <Package size={26} weight="thin" />
      {/if}
    </div>
    <div class="title">
      <h1>Скачивание</h1>
      <p>{context?.modName ?? "Обновление мода"}: {versionText}</p>
    </div>
    <button class="close" type="button" aria-label="Закрыть" onclick={() => void close()}>
      <span>Закрыть</span>
      <X size={16} />
    </button>
  </header>

  <div class="urlbar">
    <Lock size={13} />
    <span>{address}</span>
  </div>

  <div class="site-hole" aria-hidden="true">
    {#if !isTauri()}
      <div class="fake-site">
        <Package size={44} weight="thin" />
        <b>Страница мода</b>
        <small>Здесь откроется настоящая страница. Кнопку Download нажимаете вы.</small>
      </div>
    {/if}
  </div>

  <aside class="steps" aria-label="Этапы загрузки">
    <div class="steps-title">ЭТАПЫ</div>
    {#each steps as item (item.id)}
      {@const state = step > item.id ? "done" : step === item.id ? "active" : "waiting"}
      <div class="step {state}">
        <span class="marker">
          {#if state === "done"}
            <Check size={13} weight="bold" />
          {:else}
            <span class="dot"></span>
          {/if}
        </span>
        <div>
          <b>{item.title}</b>
          <p>{item.text}</p>
        </div>
      </div>
    {/each}
    <div class="status" class:error={!!error}>
      <b>{error ?? status}</b>
      {#if !error && detail && !loading}<span>{detail}</span>{/if}
      {#if loading}
        <div
          class="bar"
          class:indeterminate={loading.percent === null}
          role="progressbar"
          aria-label="Ход загрузки"
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={loading.percent ?? undefined}
        >
          <span style={`width:${loading.percent ?? 100}%`}></span>
        </div>
        <span class="mono progress-line">{loading.line}</span>
        {#if loading.stallText}
          <span class="stall">
            {loading.stallText}. Если сайт показал проверку или ошибку — посмотрите страницу.
          </span>
        {/if}
      {/if}
    </div>

    {#if inspecting}
      <div class="ready-state">Проверяем архив…</div>
    {:else if archive}
      <section class="ready" aria-label="Установка">
        <div class="ready-head">
          <b>{archiveTitle}</b>
          <span>{items.length} {plural(items.length, "мод", "мода", "модов")} · {formatSize(archiveSize)}</span>
        </div>
        {#if archive.problem}
          <p class="problem">{archive.problem}</p>
          <button class="primary" type="button" onclick={() => void close()}>Понятно</button>
        {:else if !items.some(installable)}
          <p class="problem">В архиве нет модов, которые можно установить.</p>
          <button class="primary" type="button" onclick={() => void close()}>Понятно</button>
        {:else}
          <div class="ready-items">
            {#each items as item, index (item.zipPath + index)}
              <article class="ready-item" class:blocked={item.blocked || item.locked}>
                <div class="ready-item-top">
                  {#if showChecks}
                    <label class="ready-check" class:disabled={!installable(item)}>
                      <input
                        type="checkbox"
                        checked={selected[index] ?? false}
                        disabled={!installable(item)}
                        onchange={(event) => toggle(index, event)}
                        aria-label={`Установить ${item.title}`}
                      />
                    </label>
                  {/if}
                  <div class="ready-text">
                    <b>{item.title}</b>
                    <small>{kindText(item)} · {formatSize(item.size)}</small>
                    <small class="file">{item.fileName}</small>
                  </div>
                </div>
                <ul class="ready-checks">
                  {#each itemChecks(item) as check}
                    <li>{check}</li>
                  {/each}
                </ul>
              </article>
            {/each}
          </div>
          {#if installError}<p class="problem">{installError}</p>{/if}
          <div class="ready-actions">
            <button
              class="primary"
              type="button"
              disabled={installing || picked.length === 0}
              onclick={() => void install()}
            >
              {installing ? "Устанавливаем…" : installLabel}
            </button>
            <button class="ghost" type="button" disabled={installing} onclick={() => void close()}>
              Отмена
            </button>
          </div>
        {/if}
      </section>
    {/if}
  </aside>

  <footer class="foot">
    <span>Менеджер сам ничего не нажимает на странице и не обходит ожидание и рекламу.</span>
    <button type="button" class="browser" onclick={() => void openInBrowser()}>
      Сайт не открывается? Открыть в браузере
    </button>
  </footer>
</div>

<style>
  :global(html),
  :global(body) {
    overflow: hidden;
    background: transparent;
  }

  .page {
    position: fixed;
    inset: 0;
    box-sizing: border-box;
    overflow: hidden;
    color: var(--foreground);
    background:
      radial-gradient(ellipse 70% 60% at 70% 0%, var(--accent-brand-soft), transparent 70%),
      linear-gradient(145deg, var(--surface), var(--background));
  }

  .head {
    position: absolute;
    top: 20px;
    left: 24px;
    right: 24px;
    display: flex;
    align-items: center;
    gap: 14px;
    height: 48px;
  }

  .cover {
    display: grid;
    place-items: center;
    width: 64px;
    height: 48px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
    color: var(--muted-foreground);
  }

  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .title {
    min-width: 0;
  }

  .title h1 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 26px;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .title p {
    margin: 3px 0 0;
    overflow: hidden;
    color: var(--muted-foreground);
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
    color: var(--muted-foreground);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  .close:hover {
    color: var(--foreground);
    border-color: var(--border-strong);
  }

  .urlbar {
    position: absolute;
    top: 80px;
    left: 24px;
    display: flex;
    align-items: center;
    gap: 9px;
    box-sizing: border-box;
    width: calc(100% - 448px);
    height: 28px;
    padding: 0 12px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--muted-foreground);
    font-family: var(--font-mono);
    font-size: 11.5px;
    white-space: nowrap;
  }

  .urlbar span {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .site-hole {
    position: absolute;
    top: 116px;
    left: 24px;
    box-sizing: border-box;
    width: calc(100% - 448px);
    height: calc(100% - 176px);
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: #fff;
    box-shadow: var(--shadow-2);
    pointer-events: none;
  }

  .fake-site {
    display: grid;
    place-content: center;
    height: 100%;
    padding: 24px;
    color: #5a606a;
    text-align: center;
  }

  .fake-site b {
    margin-top: 16px;
    color: #20242b;
    font-size: 17px;
  }

  .fake-site small {
    max-width: 360px;
    margin-top: 8px;
    color: #7a818c;
    line-height: 1.45;
  }

  .steps {
    position: absolute;
    top: 108px;
    right: 24px;
    bottom: 84px;
    width: 376px;
    padding: 8px 24px 8px 20px;
    box-sizing: border-box;
    border-left: 1px solid var(--border);
    overflow-y: auto;
    scrollbar-width: thin;
  }

  .steps-title {
    margin-bottom: 20px;
    color: var(--faint-foreground);
    font-family: var(--font-mono);
    font-size: 10px;
    letter-spacing: 0.16em;
  }

  .step {
    position: relative;
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr);
    gap: 12px;
    padding-bottom: 24px;
  }

  .step:not(:last-of-type)::before {
    position: absolute;
    top: 25px;
    left: 13px;
    width: 1px;
    height: calc(100% - 12px);
    background: var(--border-strong);
    content: "";
  }

  .marker {
    z-index: 1;
    display: grid;
    place-items: center;
    width: 27px;
    height: 27px;
    border: 1px solid var(--border-strong);
    border-radius: 50%;
    background: var(--surface);
    color: var(--muted-foreground);
  }

  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--faint-foreground);
  }

  .active .marker {
    border-color: var(--accent-brand);
    color: var(--accent-brand);
    box-shadow: 0 0 0 4px var(--accent-brand-soft);
  }

  .active .dot {
    background: var(--accent-brand);
  }

  .done .marker {
    border-color: var(--accent-brand);
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .step b {
    display: block;
    margin-top: 3px;
    font-size: 14px;
    font-weight: 600;
  }

  .step p {
    margin: 5px 0 0;
    color: var(--muted-foreground);
    font-size: 12px;
    line-height: 1.45;
  }

  .waiting b,
  .waiting p {
    opacity: 0.55;
  }

  .status {
    display: grid;
    gap: 3px;
    margin-top: 8px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--muted-foreground);
    font-size: 12px;
    line-height: 1.4;
  }

  .status b {
    color: var(--foreground);
    font-weight: 600;
  }

  .status span {
    color: var(--muted-foreground);
  }

  .bar {
    position: relative;
    height: 6px;
    margin-top: 6px;
    overflow: hidden;
    border-radius: 3px;
    background: var(--surface-3);
  }

  .bar span {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: inherit;
    background: var(--accent-brand);
    transition: width 250ms linear;
  }

  /* Размер неизвестен: полоса бежит, показывая, что загрузка идёт. */
  .bar.indeterminate span {
    width: 35% !important;
    animation: bar-run 1.4s var(--ease-in-out) infinite;
  }

  @keyframes bar-run {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(290%);
    }
  }

  .status .progress-line {
    color: var(--foreground);
    font-size: 11.5px;
  }

  .status .stall {
    color: var(--warning);
  }

  .status.error {
    border-color: var(--danger-soft);
    color: var(--danger);
  }

  .status.error b {
    color: var(--danger);
  }

  .ready-state {
    margin-top: 10px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--muted-foreground);
    font-size: 12px;
  }

  .ready {
    display: grid;
    gap: 8px;
    margin-top: 10px;
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }

  .ready-head {
    display: grid;
    gap: 2px;
  }

  .ready-head b {
    overflow: hidden;
    font-size: 13px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ready-head span {
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .ready-items {
    display: grid;
    gap: 6px;
    max-height: 230px;
    overflow-y: auto;
    scrollbar-width: thin;
  }

  .ready-item {
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }

  .ready-item.blocked {
    border-color: var(--danger-soft);
  }

  .ready-item-top {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  .ready-check {
    display: grid;
    flex: none;
    place-items: center;
    padding-top: 1px;
  }

  .ready-check input {
    width: 15px;
    height: 15px;
    margin: 0;
    accent-color: var(--accent-brand);
  }

  .ready-text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .ready-text b {
    font-size: 12.5px;
    line-height: 1.25;
  }

  .ready-text small {
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .ready-text .file {
    overflow: hidden;
    font-family: var(--font-mono);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .ready-checks {
    margin: 6px 0 0;
    padding-left: 13px;
    color: var(--muted-foreground);
    font-size: 11px;
    line-height: 1.4;
  }

  .ready-checks li::marker {
    color: var(--accent-brand);
  }

  .problem {
    margin: 0;
    color: var(--danger);
    font-size: 12px;
    line-height: 1.4;
  }

  .ready-actions {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 6px;
  }

  .ready button {
    height: 34px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }

  .ready .primary {
    border-color: var(--accent-brand);
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .ready .ghost {
    background: var(--surface-2);
    color: var(--foreground);
  }

  .ready button:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .ready > .primary {
    justify-self: start;
  }

  .foot {
    position: absolute;
    right: 24px;
    bottom: 24px;
    display: grid;
    gap: 7px;
    width: 376px;
    color: var(--faint-foreground);
    font-size: 11px;
    line-height: 1.45;
  }

  .foot .browser {
    justify-self: start;
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-brand-text);
    font: inherit;
    font-size: 11.5px;
    text-align: left;
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
  }

  .foot .browser:hover {
    color: var(--foreground);
  }

  @media (max-width: 1000px) {
    .urlbar,
    .site-hole {
      width: calc(100% - 360px);
    }

    .steps {
      width: 288px;
      padding: 8px 12px 8px 16px;
    }

    .foot {
      width: 288px;
    }
  }
</style>

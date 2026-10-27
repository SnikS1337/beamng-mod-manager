<script lang="ts">
  import { onMount } from "svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import Check from "phosphor-svelte/lib/Check";
  import Lock from "phosphor-svelte/lib/Lock";
  import Package from "phosphor-svelte/lib/Package";
  import X from "phosphor-svelte/lib/X";
  import {
    downloadClose,
    downloadPanelState,
    listenDownloadContext,
    listenDownloadState,
    type DownloadRequest,
  } from "#lib/ipc/download.js";

  let context = $state<DownloadRequest | null>(null);
  let step = $state(1);
  let status = $state("Нажмите Download на странице");
  let detail = $state("Скачайте архив на странице — остальное программа сделает сама.");
  let error = $state<string | null>(null);

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

  async function close(): Promise<void> {
    await downloadClose().catch(() => {});
  }

  onMount(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    void downloadPanelState().then((value) => {
      if (!disposed && value) context = value;
    });
    void listenDownloadContext((value) => {
      context = value;
      step = 1;
      status = "Нажмите Download на странице";
      detail = "Скачайте архив на странице — остальное программа сделает сама.";
      error = null;
    }).then((stop) => {
      if (disposed) stop();
      else stops.push(stop);
    });
    void listenDownloadState((value) => {
      step = value.step;
      status = value.title;
      detail = value.detail;
      error = value.error ?? null;
    }).then((stop) => {
      if (disposed) stop();
      else stops.push(stop);
    });
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
      {#if context?.previewUrl}
        <img src={context.previewUrl} alt="" />
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
      {#if !error && detail}<span>{detail}</span>{/if}
    </div>
  </aside>

  <footer class="foot">
    Менеджер сам ничего не нажимает на странице и не обходит ожидание и рекламу.
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

  .status.error {
    border-color: var(--danger-soft);
    color: var(--danger);
  }

  .status.error b {
    color: var(--danger);
  }

  .foot {
    position: absolute;
    right: 24px;
    bottom: 24px;
    width: 376px;
    color: var(--faint-foreground);
    font-size: 11px;
    line-height: 1.45;
  }
</style>

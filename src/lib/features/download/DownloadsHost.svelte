<script lang="ts">
  import { onMount } from "svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import DownloadSimple from "phosphor-svelte/lib/DownloadSimple";
  import {
    downloadCancelAll,
    downloadFocus,
    downloadJobs,
    listenDownloadJobs,
    listenDownloadProgress,
  } from "#lib/ipc/download.js";
  import { chipText, closeQuestion, downloading, track } from "./jobs.js";
  import { liveDownloads } from "./live.svelte.js";
  import { runCloseHooks } from "#lib/shell/closeGuard.js";
  import { trapFocus } from "#lib/shell/focus.js";
  import { enterOverlay, takeEscape } from "#lib/shell/overlays.js";

  // Главное окно должно видеть, что в окне скачивания идёт загрузка: было — файл
  // качался в «Загрузки», а в программе об этом ни слова.
  const chip = $derived(chipText(liveDownloads.jobs, liveDownloads.tracked, liveDownloads.now));

  /** Выход при идущей загрузке: вопрос «Прервать и выйти?». */
  let exitAsk = $state<string | null>(null);
  let exitConfirmed = false;
  let overlay = $state<symbol | null>(null);

  $effect(() => {
    if (!exitAsk) return;
    const entered = enterOverlay();
    overlay = entered.id;
    return () => {
      entered.leave();
      overlay = null;
    };
  });

  function onkeydown(event: KeyboardEvent) {
    if (exitAsk && takeEscape(event, overlay)) exitAsk = null;
  }

  async function exitAnyway(): Promise<void> {
    exitConfirmed = true;
    exitAsk = null;
    await downloadCancelAll().catch(() => {});
    await getCurrentWindow()
      .close()
      .catch(() => {});
  }

  onMount(() => {
    if (!isTauri()) return;
    let disposed = false;
    const stops: (() => void)[] = [];
    const keep = (promise: Promise<() => void>) => {
      void promise
        .then((stop) => {
          if (disposed) stop();
          else stops.push(stop);
        })
        .catch(() => {});
    };
    void downloadJobs()
      .then((value) => {
        if (!disposed && liveDownloads.jobs.length === 0) liveDownloads.jobs = value;
      })
      .catch(() => {});
    keep(
      listenDownloadJobs((value) => {
        liveDownloads.jobs = value;
        if (downloading(value).length === 0) liveDownloads.tracked = {};
      }),
    );
    keep(
      listenDownloadProgress((value) => {
        liveDownloads.tracked = track(liveDownloads.tracked, value, Date.now());
        liveDownloads.now = Date.now();
      }),
    );
    // Единственный обработчик закрытия окна: дела разделов (сохранить настройки)
    // идут через closeGuard, иначе каждый закрывал окно сам.
    keep(
      getCurrentWindow().onCloseRequested(async (event) => {
        if (!exitConfirmed) {
          const list = await downloadJobs().catch(() => liveDownloads.jobs);
          const question = closeQuestion(list, []);
          if (question) {
            event.preventDefault();
            exitAsk = question;
            return;
          }
        }
        await runCloseHooks();
      }),
    );
    return () => {
      disposed = true;
      for (const stop of stops) stop();
    };
  });
</script>

<svelte:window {onkeydown} />

{#if chip}
  <button class="chip" type="button" title="Показать окно скачивания" onclick={() => void downloadFocus()}>
    <DownloadSimple size={14} aria-hidden="true" />
    <span>{chip}</span>
  </button>
{/if}

{#if exitAsk}
  <div class="backdrop">
    <div
      class="ask glass"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="exit-title"
      aria-describedby="exit-text"
      use:trapFocus={".keep"}
    >
      <h2 id="exit-title">Выйти из программы?</h2>
      <p id="exit-text">{exitAsk}</p>
      <footer>
        <button class="danger" type="button" onclick={() => void exitAnyway()}>Прервать и выйти</button>
        <button class="keep" type="button" onclick={() => (exitAsk = null)}>Не выходить</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .chip {
    position: fixed;
    top: 5px;
    right: 150px;
    z-index: 30;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    max-width: min(420px, 45vw);
    height: 24px;
    padding: 0 10px;
    border: 1px solid var(--accent-brand);
    border-radius: 999px;
    background: var(--accent-brand-soft);
    color: var(--foreground);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }

  .chip span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--overlay);
  }

  .ask {
    width: min(440px, calc(100vw - 48px));
    padding: 22px 24px 18px;
    border-radius: 14px;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  p {
    margin: 8px 0 16px;
    color: var(--muted-foreground);
    font-size: 13.5px;
    line-height: 1.5;
  }

  footer {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  footer button {
    height: 34px;
    padding: 0 14px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--foreground);
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }

  footer .danger {
    border-color: var(--danger);
    background: var(--danger);
    color: #fff;
  }
</style>

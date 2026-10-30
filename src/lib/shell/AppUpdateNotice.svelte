<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import X from "phosphor-svelte/lib/X";
  import {
    appUpdateDownload,
    appUpdateRestart,
    appUpdateState,
    listenAppUpdateProgress,
    listenAppUpdateState,
    type AppUpdateSnapshot,
  } from "#lib/ipc/appUpdate.js";
  import { noticeFor, notesPreview } from "./appUpdateNotice";
  import { pop, softFade } from "./transitions";

  // Лёгкая плашка «Доступна новая версия»: проверка при запуске идёт в фоне,
  // а без плашки о ней знал только раздел «О программе».
  let snapshot = $state<AppUpdateSnapshot | null>(null);
  let dismissed = $state<string | null>(null);
  let busy = $state(false);

  const view = $derived(noticeFor(snapshot, dismissed));
  const preview = $derived(notesPreview(snapshot?.state.manifest?.notes));

  onMount(() => {
    let disposed = false;
    const stops: (() => void)[] = [];
    void appUpdateState()
      .then((value) => {
        if (!disposed) snapshot = value;
      })
      .catch(() => {});
    void listenAppUpdateState((value) => {
      if (!disposed) snapshot = value;
    })
      .then((stop) => (disposed ? stop() : stops.push(stop)))
      .catch(() => {});
    void listenAppUpdateProgress((progress) => {
      if (disposed || !snapshot) return;
      snapshot = { ...snapshot, state: { ...snapshot.state, progress } };
    })
      .then((stop) => (disposed ? stop() : stops.push(stop)))
      .catch(() => {});
    return () => {
      disposed = true;
      for (const stop of stops) stop();
    };
  });

  async function update() {
    if (busy || !snapshot) return;
    busy = true;
    snapshot = { ...snapshot, state: { ...snapshot.state, stage: "downloading", progress: 0 } };
    try {
      snapshot = await appUpdateDownload();
    } catch (reason) {
      const message =
        reason && typeof reason === "object" && "message" in reason
          ? String((reason as { message: unknown }).message)
          : "Не удалось скачать обновление.";
      snapshot = { ...snapshot, state: { ...snapshot.state, stage: "failed", error: message } };
    } finally {
      busy = false;
    }
  }

  function later() {
    dismissed = view?.version ?? null;
  }

  function details() {
    void goto("/settings?section=program");
  }
</script>

{#if view}
  <aside
    class="notice"
    aria-label="Обновление программы"
    in:pop={{ duration: 260, y: 12 }}
    out:softFade={{ duration: 160 }}
  >
    <div class="head">
      <b>
        {view.stage === "ready"
          ? `Версия ${view.version} скачана`
          : `Доступна версия ${view.version}`}
      </b>
      {#if view.stage === "available" || view.stage === "failed"}
        <button class="close" type="button" aria-label="Позже" onclick={later}>
          <X size={14} />
        </button>
      {/if}
    </div>

    {#if view.stage === "available" && preview.length > 0}
      <ul>
        {#each preview as line}<li>{line}</li>{/each}
      </ul>
    {:else if view.stage === "downloading"}
      <div class="bar" role="progressbar" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(view.progress * 100)}>
        <span style={`width:${Math.max(3, Math.round(view.progress * 100))}%`}></span>
      </div>
      <p>Скачиваем — {Math.round(view.progress * 100)} %</p>
    {:else if view.stage === "ready"}
      <p>Перезапуск займёт пару секунд. Прошлая версия останется для отката.</p>
    {:else if view.stage === "failed"}
      <p class="error">{view.error ?? "Не удалось скачать обновление."}</p>
    {/if}

    <div class="actions">
      {#if view.stage === "available" || view.stage === "failed"}
        <button class="primary" type="button" disabled={busy} onclick={() => void update()}>
          {view.stage === "failed" ? "Ещё раз" : "Обновить"}
        </button>
        <button class="ghost" type="button" onclick={details}>Что нового</button>
      {:else if view.stage === "ready"}
        <button class="primary" type="button" onclick={() => void appUpdateRestart()}>
          Перезапустить
        </button>
        <button class="ghost" type="button" onclick={later}>Позже</button>
      {/if}
    </div>
  </aside>
{/if}

<style>
  .notice {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 65;
    display: grid;
    gap: 8px;
    width: min(320px, calc(100vw - 120px));
    padding: 14px 14px 12px 16px;
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    background: var(--surface-2);
    box-shadow: var(--shadow-2);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .head b {
    font-size: 13.5px;
  }

  .close {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--muted-foreground);
  }

  .close:hover {
    background: var(--surface-3);
    color: var(--foreground);
  }

  ul {
    display: grid;
    gap: 2px;
    margin: 0;
    padding-left: 16px;
    list-style: disc;
    color: var(--muted-foreground);
    overflow-wrap: anywhere;
  }

  ul li {
    display: -webkit-box;
    overflow: hidden;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
  }

  p {
    margin: 0;
    color: var(--muted-foreground);
  }

  p.error {
    color: var(--danger);
  }

  .bar {
    height: 6px;
    overflow: hidden;
    border-radius: 3px;
    background: var(--surface-3);
  }

  .bar span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent-brand);
    transition: width 250ms linear;
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 2px;
  }

  .actions button {
    height: 30px;
    padding: 0 12px;
    border-radius: 8px;
    font-size: 12.5px;
    font-weight: 600;
  }

  .primary {
    border: 0;
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .primary:disabled {
    opacity: 0.6;
  }

  .ghost {
    border: 1px solid var(--border);
    background: transparent;
    color: var(--foreground);
  }

  .ghost:hover {
    background: var(--surface-3);
  }
</style>

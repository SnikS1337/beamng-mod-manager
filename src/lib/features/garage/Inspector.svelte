<script lang="ts">
  import { openPath } from "@tauri-apps/plugin-opener";
  import { Button } from "$lib/components/ui/button";
  import {
    modPreview,
    type ModDetails,
    type PackagingProblem,
    type VersionRow,
  } from "$lib/ipc/library";
  import { formatDate, formatSize, kindLabel, plural } from "./format";
  import VersionsBlock from "./VersionsBlock.svelte";

  let {
    details,
    loading,
    canDelete,
    problem = null,
    fixing = false,
    onFix,
    onToggle,
    onDelete,
    onRestoreVersion,
    onClose,
  }: {
    details: ModDetails | null;
    loading: boolean;
    canDelete: boolean;
    problem?: PackagingProblem | null;
    fixing?: boolean;
    onFix?: () => void;
    onToggle: () => void;
    onDelete: () => void;
    onRestoreVersion: (version: VersionRow) => void;
    onClose: () => void;
  } = $props();

  let preview = $state<string | null>(null);

  const problemText: Record<PackagingProblem, string> = {
    wrapped:
      "Всё лежит в лишней папке внутри архива, и игра мод не видит. Программа уберёт эту папку, прежний файл останется в копиях.",
    loose:
      "Машина упакована без папки vehicles, и игра мод не видит. Программа переупакует архив, прежний файл останется в копиях.",
    not_mod:
      "Внутри нет ничего, что загружает игра: это может быть установщик, архив в архиве или картинки. Распакуйте и поставьте мод через «Из архива».",
  };

  $effect(() => {
    const id = details?.id;
    preview = details?.previewUrl ?? null;
    if (!id || preview) return;
    void modPreview(id)
      .then((url) => {
        if (details?.id === id) preview = url;
      })
      .catch(() => {});
  });

  const hue = $derived(details ? hashHue(details.id) : 210);
  const placeholder = $derived(
    `radial-gradient(120% 90% at 18% 0%, hsl(${hue} 85% 68% / 0.8), transparent 62%), linear-gradient(135deg, hsl(${hue} 38% 28%), hsl(${(hue + 58) % 360} 42% 16%))`,
  );

  function hashHue(id: string): number {
    let hash = 0;
    for (let index = 0; index < id.length; index += 1) {
      hash = (hash * 31 + id.charCodeAt(index)) % 360;
    }
    return hash;
  }

  async function openFolder() {
    if (!details?.folder) return;
    try {
      await openPath(details.folder);
    } catch {
      // В браузере и без прав просто ничего не делаем.
    }
  }
</script>

<aside class="inspector glass liquid-edge scroll-area">
  <div class="top">
    <span class="eyebrow">Мод</span>
    <button class="close" type="button" aria-label="Закрыть панель" onclick={onClose}>×</button>
  </div>

  {#if loading && !details}
    <div class="skeleton"></div>
  {:else if details}
    <div class="hero" style={preview ? undefined : `background:${placeholder}`}>
      {#if preview}
        <img src={preview} alt="" />
      {:else}
        <span>{kindLabel(details.kind)}</span>
      {/if}
    </div>

    <h2>{details.name}</h2>
    <p class="file">{details.fileName}</p>

    <div class="facts">
      <div><span>Размер</span><b>{formatSize(details.sizeBytes)}</b></div>
      <div><span>Установлен</span><b>{formatDate(details.installedAt)}</b></div>
      {#if details.version}
        <div><span>Версия</span><b>{details.version}</b></div>
      {/if}
      {#if details.compat.text}
        <div class:warning={details.compat.differs}>
          <span>Для игры</span><b>{details.compat.text}</b>
        </div>
      {/if}
      {#if details.binding}
        <div><span>Следим на</span><b>{details.binding.source}</b></div>
      {/if}
    </div>

    {#if problem}
      <section class="problem">
        <h3>Игра не загрузит этот мод</h3>
        <p>{problemText[problem]}</p>
        {#if problem !== "not_mod" && onFix}
          <Button class="primary" onclick={onFix} disabled={fixing}>
            {fixing ? "Исправляем…" : "Исправить"}
          </Button>
        {/if}
      </section>
    {/if}

    {#if details.archive}
      <section>
        <h3>Внутри архива</h3>
        {#if details.archive.unreadable}
          <p class="muted">Архив не читается: возможно, он повреждён.</p>
        {:else}
          {#if details.archive.vehicles.length > 0}
            <p class="line"><span>Машины</span><b>{details.archive.vehicles.slice(0, 3).join(", ")}{details.archive.vehicles.length > 3 ? ` и ещё ${details.archive.vehicles.length - 3}` : ""}</b></p>
          {/if}
          {#if details.archive.levels.length > 0}
            <p class="line"><span>Карты</span><b>{details.archive.levels.slice(0, 3).join(", ")}{details.archive.levels.length > 3 ? ` и ещё ${details.archive.levels.length - 3}` : ""}</b></p>
          {/if}
          <p class="line"><span>Скрипты</span><b>{details.archive.hasScripts ? `есть, ${details.archive.luaFiles} ${plural(details.archive.luaFiles, "файл", "файла", "файлов")} Lua` : "нет"}</b></p>
          <p class="line"><span>Файлов</span><b>{details.archive.files} · {formatSize(details.archive.unpackedBytes)} после распаковки</b></p>
          {#each details.archive.warnings as warning}
            <p class="warn">{warning}</p>
          {/each}
        {/if}
      </section>
    {/if}

    {#if canDelete}
      <VersionsBlock modId={details.id} onRestore={onRestoreVersion} />
    {/if}

    <div class="actions">
      <Button class="primary" onclick={onToggle}>
        {details.enabled ? "Выключить" : "Включить"}
      </Button>
      <Button variant="outline" onclick={() => void openFolder()} disabled={!details.folder}>
        Открыть папку
      </Button>
      {#if canDelete}
        <Button variant="destructive" onclick={onDelete}>Удалить</Button>
        <p class="hint">Можно вернуть в течение нескольких секунд.</p>
      {/if}
    </div>
  {:else}
    <p class="muted">Выберите мод слева.</p>
  {/if}
</aside>

<style>
  .problem {
    border: 1px solid oklch(0.66 0.19 25 / 0.45);
    border-radius: 14px;
    padding: 12px;
    background: oklch(0.66 0.19 25 / 0.08);
  }

  .problem p {
    margin: 6px 0 10px;
    font-size: 12px;
    line-height: 1.45;
  }

  .inspector {
    min-width: 0;
    height: 100%;
    overflow-y: auto;
    border-radius: 20px;
    padding: 16px;
  }

  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .eyebrow {
    color: var(--muted-foreground);
    font-size: 9.5px;
    font-weight: 650;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .close {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
    color: var(--muted-foreground);
    font-size: 18px;
    line-height: 1;
    cursor: default;
  }

  .close:hover {
    color: var(--foreground);
  }

  .hero {
    position: relative;
    height: 132px;
    margin: 10px 0 14px;
    border-radius: 16px;
    overflow: hidden;
  }

  .hero img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .hero span {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: rgb(255 255 255 / 0.2);
    font-size: 18px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h2 {
    margin: 0;
    overflow: hidden;
    font-size: 20px;
    font-weight: 650;
    line-height: 1.2;
    text-overflow: ellipsis;
  }

  .file {
    margin: 4px 0 12px;
    overflow: hidden;
    color: var(--muted-foreground);
    font-family: "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
    font-size: 9.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .facts {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 12px 0;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }

  .facts div,
  .line {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin: 0;
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .facts b,
  .line b {
    color: var(--foreground);
    font-weight: 500;
    text-align: right;
  }

  .facts .warning b {
    color: var(--warning);
  }

  section {
    padding-top: 12px;
  }

  h3 {
    margin: 0 0 8px;
    color: var(--muted-foreground);
    font-size: 9.5px;
    font-weight: 650;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .line {
    margin-bottom: 5px;
  }

  .warn {
    margin: 6px 0 0;
    color: var(--warning);
    font-size: 10px;
    line-height: 1.4;
  }

  .muted {
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding-top: 14px;
  }

  .hint {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 9.5px;
    text-align: center;
  }

  .skeleton {
    height: 260px;
    margin: 10px 0;
    border-radius: 16px;
    background: color-mix(in oklab, var(--foreground) 6%, transparent);
    animation: pulse 1.6s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }

  @media (max-width: 900px) {
    .inspector {
      display: none;
    }
  }
</style>

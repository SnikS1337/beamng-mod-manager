<script lang="ts">
  import { openExternal } from "#lib/shell/openExternal.js";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { sourceName } from "#lib/ipc/search.js";
  import {
    modPreview,
    type ModDetails,
    type DuplicateOf,
    type ModCardDto,
    type PackagingProblem,
    type VersionRow,
  } from "#lib/ipc/library.js";
  import { expectDownload } from "#lib/ipc/incoming.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import X from "phosphor-svelte/lib/X";
  import Car from "phosphor-svelte/lib/Car";
  import { formatDate, formatSize, groupDigits, plural } from "./format";
  import { typeLabel, type TypeTag } from "./view";
  import VersionsBlock from "./VersionsBlock.svelte";
  import { duplicateExplain } from "./duplicates";

  let {
    details,
    loading,
    canDelete,
    problem = null,
    duplicateOf = null,
    group = [],
    addon = false,
    tag = "mod",
    fixing = false,
    update = "none",
    updateVersion = null,
    onFix,
    onShowMod,
    onKeepOnly,
    onDeleteCard,
    onToggle,
    onDelete,
    onRestoreVersion,
    onUnfollow,
    onBindPage,
    onPassCheck,
    onClose,
  }: {
    details: ModDetails | null;
    loading: boolean;
    canDelete: boolean;
    problem?: PackagingProblem | null;
    /** Повторяет другой мод (основной в группе). */
    duplicateOf?: DuplicateOf | null;
    /** Основной и его копии, если это основной мод группы. */
    group?: ModCardDto[];
    /** Дополнение (не машина и не карта): без картинки, как в списке. */
    addon?: boolean;
    /** Метка типа — та же, что на карточке. */
    tag?: TypeTag;
    fixing?: boolean;
    update?: string;
    updateVersion?: string | null;
    onFix?: () => void;
    /** Открыть другой мод из группы копий. */
    onShowMod?: (id: string) => void;
    /** Оставить включённой только эту копию. */
    onKeepOnly?: (card: ModCardDto) => void;
    /** Удалить копию (с «Отменить»). */
    onDeleteCard?: (card: ModCardDto) => void;
    onToggle: () => void;
    onDelete: () => void;
    onRestoreVersion: (version: VersionRow) => void;
    /** «Это другой мод»: программа следит не за той страницей. */
    onUnfollow?: (source: string) => void;
    /** «Указать страницу»: `null` — получилось, иначе причина, почему нет. */
    onBindPage?: (url: string) => Promise<string | null>;
    /** Сайт просит подтвердить, что вы человек: открыть его проверку. `true` — пройдена. */
    onPassCheck?: () => Promise<boolean>;
    onClose: () => void;
  } = $props();

  let preview = $state<string | null>(null);
  let binding = $state(false);
  let pageUrl = $state("");
  let bindError = $state<string | null>(null);
  let bindBusy = $state(false);

  // Поле сбрасывается только при переходе к другому моду: панель перечитывает данные
  // того же мода (после проверки, названий из архивов) и не должна закрывать форму.
  let formFor: string | undefined;
  $effect(() => {
    const id = details?.id;
    if (id === formFor) return;
    formFor = id;
    binding = false;
    pageUrl = "";
    bindError = null;
  });

  const needsCheck = $derived(!!bindError && bindError.includes("человек"));

  async function bindPage(event?: SubmitEvent) {
    event?.preventDefault();
    if (!onBindPage || bindBusy || !pageUrl.trim()) return;
    bindBusy = true;
    bindError = await onBindPage(pageUrl.trim());
    bindBusy = false;
    if (!bindError) binding = false;
  }

  async function passCheck() {
    if (!onPassCheck || bindBusy) return;
    bindBusy = true;
    const passed = await onPassCheck().catch(() => false);
    bindBusy = false;
    // Проверку прошли — сразу пробуем ещё раз, вставлять ссылку заново не нужно.
    if (passed) await bindPage();
  }

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
    preview = addon ? null : (details?.previewUrl ?? null);
    if (!id || preview || addon) return;
    void modPreview(id)
      .then((url) => {
        if (details?.id === id) preview = url;
      })
      .catch(() => {});
  });

  const hasUpdate = $derived(update !== "none" && !!details?.binding);

  async function openPage() {
    if (!details?.binding) return;
    if (!(await openExternal(details.binding.url))) return;
    if (hasUpdate) {
      await expectDownload();
      toasts.show(
        "Скачайте архив на странице — программа заметит его в «Загрузках» и предложит установить",
      );
    }
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

<aside class="inspector scroll-area" aria-label="Выбранный мод">
  <button class="close" type="button" aria-label="Закрыть панель" onclick={onClose}>
    <X size={15} weight="bold" />
  </button>

  {#if loading && !details}
    <div class="skeleton hero-skeleton"></div>
    <div class="skeleton line-skeleton"></div>
  {:else if details}
    {#key details.id}
      <div class="hero" class:addon>
        {#if preview}
          <img src={preview} alt="" />
        {:else}
          <span class="hero-icon"><Car size={56} weight="thin" /></span>
        {/if}
        <span class="kind-tag {tag}">{typeLabel(tag)}</span>
      </div>
    {/key}

    <h2 class="display">{details.name}</h2>
    <p class="file" title={details.fileName}>{details.fileName}</p>

    {#if group.length > 1}
      {@const enabledCount = group.filter((item) => item.enabled).length}
      <section class="copies" class:conflict={enabledCount > 1}>
        <h3>В группе {group.length} {plural(group.length, "файл", "файла", "файлов")}</h3>
        <p>
          {enabledCount > 1
            ? `Включено ${enabledCount} — игра загрузит все, и чьи файлы победят, неизвестно. Оставьте одну.`
            : "В списке показан самый свежий. Остальные — здесь."}
        </p>
        <ul>
          {#each group as item (item.id)}
            <li class:current={item.id === details.id}>
              <div class="copy-text">
                <b>{item.fileName}</b>
                <span>
                  {item.version ? `${item.version} · ` : ""}{formatSize(item.sizeBytes)} · {item.enabled ? "включён" : "выключен"}
                </span>
              </div>
              <div class="copy-actions">
                {#if onKeepOnly && !(item.enabled && enabledCount === 1)}
                  <button class="link" type="button" onclick={() => onKeepOnly?.(item)}>Только эта</button>
                {/if}
                {#if onShowMod && item.id !== details.id}
                  <button class="link" type="button" onclick={() => onShowMod?.(item.id)}>Открыть</button>
                {/if}
                {#if onDeleteCard && !item.fromGameRepo && item.id !== details.id}
                  <button class="link danger" type="button" onclick={() => onDeleteCard?.(item)}>Удалить</button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <!-- Одно главное действие по состоянию (legacy): исправить, обновить или включить. -->
    {#if problem}
      <section class="problem">
        <h3>Игра не загрузит этот мод</h3>
        <p>{problemText[problem]}</p>
        {#if problem !== "not_mod" && onFix}
          <button class="btn-primary wide" type="button" onclick={onFix} disabled={fixing}>
            {fixing ? "Исправляем…" : "Исправить"}
          </button>
        {/if}
      </section>
    {:else if duplicateOf}
      {@const explain = duplicateExplain(duplicateOf)}
      <section class="problem">
        <h3>{explain.title}</h3>
        <p>{explain.text}</p>
        {#if onShowMod}
          <button class="btn-glass wide" type="button" onclick={() => onShowMod?.(duplicateOf.id)}>
            К основной карточке
          </button>
        {/if}
      </section>
    {:else if hasUpdate}
      <button class="btn-primary wide" type="button" onclick={() => void openPage()}>
        {updateVersion ? `Обновить до ${updateVersion}` : "Открыть страницу обновления"}
      </button>
      <p class="explain">
        Откроем страницу мода. Скачанный архив программа поставит сама, прошлая версия сохранится.
      </p>
    {:else if !details.enabled}
      <button class="btn-primary wide" type="button" onclick={onToggle}>Включить</button>
    {/if}

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
        <div><span>Следим на</span><b>{sourceName(details.binding.source)}</b></div>
      {:else}
        <div><span>Обновления</span><b>не проверяются — нет страницы</b></div>
      {/if}
    </div>

    {#if details.archive}
      <section>
        <h3>Внутри архива</h3>
        <div class="facts">
          {#if details.archive.unreadable}
            <p class="muted">Архив не читается: возможно, он повреждён.</p>
          {:else}
            {#if details.archive.vehicles.length > 0}
              <div><span>{details.archive.vehicles.length > 1 ? "Машины" : "Машина"}</span><b>{details.archive.vehicles.slice(0, 3).join(", ")}{details.archive.vehicles.length > 3 ? ` и ещё ${details.archive.vehicles.length - 3}` : ""}</b></div>
            {/if}
            {#if details.archive.levels.length > 0}
              <div><span>{details.archive.levels.length > 1 ? "Карты" : "Карта"}</span><b>{details.archive.levels.slice(0, 3).join(", ")}{details.archive.levels.length > 3 ? ` и ещё ${details.archive.levels.length - 3}` : ""}</b></div>
            {/if}
            <div><span>Скрипты</span><b>{details.archive.hasScripts ? `есть, ${details.archive.luaFiles} ${plural(details.archive.luaFiles, "файл", "файла", "файлов")} Lua` : "нет"}</b></div>
            <div><span>Файлов</span><b>{groupDigits(details.archive.files)}, {formatSize(details.archive.unpackedBytes)} после распаковки</b></div>
            {#each details.archive.warnings as warning}
              <p class="warn">{warning}</p>
            {/each}
          {/if}
        </div>
      </section>
    {/if}

    {#if canDelete}
      <VersionsBlock modId={details.id} onRestore={onRestoreVersion} />
    {/if}

    {#if binding}
      <form class="bind" onsubmit={bindPage}>
        <span>Ссылка на страницу мода</span>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          type="text"
          inputmode="url"
          spellcheck="false"
          bind:value={pageUrl}
          placeholder="https://stmods.org/beamng_drive/mods/…"
          autofocus
          disabled={bindBusy}
        />
        {#if bindError}
          <p class="bind-error">{bindError}</p>
          {#if needsCheck && onPassCheck}
            <button class="btn-glass" type="button" onclick={() => void passCheck()} disabled={bindBusy}>
              Пройти проверку сайта
            </button>
          {/if}
        {/if}
        <div class="bind-actions">
          <button class="btn-primary" type="submit" disabled={bindBusy || !pageUrl.trim()}>
            {bindBusy ? "Открываем страницу…" : "Следить"}
          </button>
          <button class="btn-glass" type="button" onclick={() => (binding = false)} disabled={bindBusy}>
            Отмена
          </button>
        </div>
      </form>
    {/if}

    <div class="links">
      {#if onBindPage && !details.binding?.confirmed && !binding}
        <button class="btn-glass" type="button" onclick={() => (binding = true)}>Указать страницу</button>
      {/if}
      {#if details.binding}
        <button class="btn-glass" type="button" onclick={() => void openPage()}>Открыть страницу</button>
        {#if details.binding.confirmed && onUnfollow}
          <button class="btn-glass" type="button" onclick={() => onUnfollow?.(details!.binding!.source)}>
            Это другой мод
          </button>
        {/if}
      {/if}
      {#if details.enabled || hasUpdate || problem}
        <button class="btn-glass" type="button" onclick={onToggle}>
          {details.enabled ? "Выключить" : "Включить"}
        </button>
      {/if}
      <button class="btn-glass" type="button" onclick={() => void openFolder()} disabled={!details.folder}>
        Открыть папку
      </button>
    </div>
    {#if canDelete}
      <p class="delete">
        <button type="button" onclick={onDelete}>Удалить</button>
        <span>Можно вернуть позже</span>
      </p>
    {/if}
  {:else}
    <p class="muted">Выберите мод слева.</p>
  {/if}
</aside>

<style>
  /* Панель справа: на всю высоту, плоская, отделена кромкой — как в макете. */
  .inspector {
    position: relative;
    min-width: 0;
    height: 100%;
    box-sizing: border-box;
    overflow-y: auto;
    padding: 20px;
    border-left: 1px solid var(--border);
    background: var(--surface);
    animation: panel-in var(--dur-3) var(--ease-out) both;
  }

  @keyframes panel-in {
    from {
      opacity: 0;
      transform: translateX(12px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }

  .close {
    position: absolute;
    top: 28px;
    right: 28px;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: color-mix(in oklab, var(--background) 80%, transparent);
    color: var(--foreground);
    cursor: default;
  }

  .close:hover {
    background: var(--surface-3);
  }

  .hero {
    position: relative;
    height: 190px;
    margin-bottom: 16px;
    overflow: hidden;
    border-radius: 10px;
    background: var(--surface-2);
    animation: fade-in var(--dur-3) var(--ease-out) both;
  }

  .hero.addon {
    background: repeating-linear-gradient(
      135deg,
      var(--surface-2) 0 12px,
      color-mix(in oklab, var(--surface-2) 70%, var(--surface-3)) 12px 24px
    );
  }

  .hero img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .hero-icon {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--faint-foreground);
    opacity: 0.6;
  }

  .hero .kind-tag {
    position: absolute;
    top: 12px;
    left: 12px;
  }

  h2 {
    margin: 0;
    font-size: 25px;
    line-height: 1.1;
  }

  .file {
    margin: 4px 0 16px;
    overflow: hidden;
    color: var(--faint-foreground);
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .wide {
    width: 100%;
    height: 40px;
  }

  .explain {
    margin: 8px 2px 0;
    color: var(--muted-foreground);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .facts {
    display: flex;
    flex-direction: column;
    margin-top: 16px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  section .facts {
    margin-top: 0;
  }

  .facts div {
    display: flex;
    justify-content: space-between;
    gap: 14px;
    padding: 9px 12px;
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .facts div + div {
    border-top: 1px solid var(--border);
  }

  .facts b {
    color: var(--foreground);
    font-weight: 600;
    text-align: right;
  }

  .facts .warning b {
    color: var(--warning);
  }

  .facts p {
    margin: 0;
    padding: 9px 12px;
  }

  section {
    padding-top: 18px;
  }

  h3 {
    margin: 0 0 8px;
    color: var(--faint-foreground);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .copies {
    display: grid;
    gap: 8px;
    margin-bottom: 4px;
    padding: 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .copies.conflict {
    border-color: color-mix(in oklab, var(--danger) 50%, transparent);
    background: var(--danger-soft);
  }

  .copies h3 {
    margin: 0;
  }

  .copies p {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .copies ul {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .copies li {
    display: grid;
    gap: 4px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }

  .copies li.current {
    border-color: var(--accent-brand);
  }

  .copy-text {
    display: grid;
    min-width: 0;
  }

  .copy-text b {
    overflow: hidden;
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .copy-text span {
    color: var(--muted-foreground);
    font-size: 11.5px;
  }

  .copy-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .copy-actions .link {
    padding: 0;
    border: 0;
    background: none;
    color: var(--accent-brand-text);
    font-size: 12.5px;
    font-weight: 700;
    cursor: default;
  }

  .copy-actions .link:hover {
    text-decoration: underline;
  }

  .copy-actions .danger {
    color: var(--danger);
  }

  .problem {
    padding: 14px;
    border: 1px solid var(--warning-border);
    border-radius: 10px;
    background: var(--warning-soft);
  }

  .problem h3 {
    margin: 0;
    color: var(--warning);
  }

  .problem p {
    margin: 6px 0 12px;
    font-size: 13px;
    line-height: 1.45;
  }

  .warn {
    color: var(--warning);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .muted {
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding-top: 18px;
  }

  .links :global(.btn-glass) {
    flex: 1 1 auto;
    height: 34px;
    font-size: 13px;
  }

  .bind {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 18px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
  }

  .bind span {
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .bind input {
    height: 36px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--foreground);
    font-size: 13px;
    outline: none;
    user-select: text;
  }

  .bind input:focus {
    border-color: var(--accent-brand);
  }

  .bind-error {
    margin: 0;
    color: var(--danger);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .bind-actions {
    display: flex;
    gap: 8px;
  }

  .bind-actions :global(button) {
    flex: 1 1 auto;
    height: 34px;
  }

  .delete {
    display: flex;
    align-items: baseline;
    gap: 10px;
    margin: 16px 0 0;
    color: var(--faint-foreground);
    font-size: 12.5px;
  }

  .delete button {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--danger);
    font-size: 13px;
    font-weight: 700;
    cursor: default;
  }

  .delete button:hover {
    text-decoration: underline;
  }

  .hero-skeleton {
    height: 190px;
    border-radius: 10px;
  }

  .line-skeleton {
    height: 28px;
    width: 70%;
    margin-top: 16px;
    border-radius: 6px;
  }
</style>

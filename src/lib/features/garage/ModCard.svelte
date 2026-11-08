<script lang="ts">
  import { kindIcon, statusIcon } from "./tagIcons";
  import { arrival } from "#lib/shell/arrival.svelte.js";
  import { toggleLabel } from "./a11y";
  import type { ClickKeys } from "./selection";
  import Check from "phosphor-svelte/lib/Check";
  import Copy from "phosphor-svelte/lib/Copy";
  import { Switch } from "#lib/components/ui/switch/index.js";
  import { modPreview, type ModCardDto } from "#lib/ipc/library.js";
  import { copiesBadge } from "./duplicates";
  import { addonCaption, cardActions, compatNote, versionLine } from "./cardText";
  import ArrowSquareOut from "phosphor-svelte/lib/ArrowSquareOut";
  import DownloadSimple from "phosphor-svelte/lib/DownloadSimple";
  import { formatSize, plural } from "./format";
  import { typeLabel, typeTag } from "./view";
  import { modStatus } from "./status";
  import { flowName } from "#lib/shell/motion.svelte.js";
  import { rollIn } from "#lib/shell/transitions.js";
  import { coverFlight } from "./coverFlight.svelte";
  import { conflictFlash, installedFlash } from "./conflictFlash.svelte";
  import DrawCheck from "#lib/shell/DrawCheck.svelte";
  import { liveDownloads } from "#lib/features/download/live.svelte.js";
  import { modProgress } from "#lib/features/download/jobs.js";

  let {
    mod,
    selected,
    selecting = false,
    index = 0,
    appear = false,
    onselect,
    ontoggle,
    onupdate,
    onopenpage,
  }: {
    mod: ModCardDto;
    selected: boolean;
    /** Что-то уже выбрано: галочки видны у всех карточек, не только под курсором. */
    selecting?: boolean;
    /** Порядок в видимой части — для небольшой задержки появления. */
    index?: number;
    /** Появиться с анимацией (первый показ списка, смена фильтра). */
    appear?: boolean;
    onselect: (id: string, keys: ClickKeys) => void;
    ontoggle: (mod: ModCardDto) => void;
    /** «Обновить» на карточке — как кнопка в панели мода. */
    onupdate?: (mod: ModCardDto) => void;
    /** «Открыть сайт» на карточке. */
    onopenpage?: (mod: ModCardDto) => void;
  } = $props();

  const actions = $derived(cardActions(mod));

  let preview = $state<string | null>(null);
  let requested = $state("");
  let loaded = $state(false);
  /** Для этого мода качается обновление: полоска на обложке. */
  const download = $derived(modProgress(liveDownloads.jobs, liveDownloads.tracked, mod.id));
  /** Обновление уже качается — второй раз страницу не открываем. */
  const showUpdate = $derived(actions.update && download === null);
  /** Картинка не открылась (битый файл превью): показываем значок, а не пустое место. */
  let brokenPreview = $state<string | null>(null);

  // Дополнение (детали для машин игры, погода, скрипт): картинки у таких модов
  // случайные — текстуры, шум, — поэтому карточка без картинки, с подписью.
  const addon = $derived(mod.role === "addon");
  const tag = $derived(typeTag(mod));
  const copies = $derived(copiesBadge(mod));
  const status = $derived(modStatus(mod));

  $effect(() => {
    const id = mod.id;
    // Ключ включает версию файла: после замены архива старая картинка не должна
    // оставаться на карточке, даже если превью у нового файла нет.
    const key = `${id}|${mod.fileName}|${mod.sizeBytes}|${mod.version ?? ""}|${mod.role ?? ""}`;
    if (addon || mod.deleted) {
      preview = null;
      return;
    }
    if (mod.previewUrl) {
      preview = mod.previewUrl;
      return;
    }
    if (requested === key) return;
    requested = key;
    preview = null;
    loaded = false;
    void modPreview(id, key)
      .then((url) => {
        if (requested === key) preview = url;
      })
      .catch(() => {});
  });

  // Панель мода покажет эту же обложку сразу, пока грузятся данные мода.
  $effect(() => {
    coverFlight.remember(mod.id, preview && preview !== brokenPreview ? preview : null);
  });

  const icon = $derived(kindIcon(tag));
  const KindIcon = $derived(icon);

  function select(event: MouseEvent) {
    onselect(mod.id, { ctrl: event.ctrlKey || event.metaKey, shift: event.shiftKey });
  }

  const version = $derived(versionLine(mod));
  const compat = $derived(compatNote(mod));
  const caption = $derived(addonCaption(mod));
</script>

<div
  class="card flow-card"
  class:arrive-target={arrival.isLanded(mod.fileName)}
  class:selected
  class:selecting
  class:appear
  class:sleeping={!mod.enabled && !mod.deleted}
  class:deleted={mod.deleted}
  style={`--i:${Math.min(index, 14)}; --flow:${flowName(mod.id)}`}
  data-mod-id={mod.id}
>
  <!-- Выбор — отдельной кнопкой поверх карточки: переключатель не вложен в кнопку. -->
  <button
    class="select"
    type="button"
    aria-pressed={selected}
    aria-label={mod.name}
    title={`${mod.name} — ${mod.fileName}`}
    onclick={select}
  ></button>
  <button
    class="check"
    type="button"
    role="checkbox"
    aria-checked={selected}
    aria-label={selected ? `Снять выбор с «${mod.name}»` : `Выбрать «${mod.name}»`}
    title={selected ? "Снять выбор" : "Выбрать для действий"}
    onclick={(event) => {
      event.stopPropagation();
      onselect(mod.id, { check: true });
    }}
  >
    {#if selected}<Check size={12} weight="bold" />{/if}
  </button>
  <div class="thumb" class:addon class:cover-from={coverFlight.from === mod.id}>
    {#if addon}
      {@const Icon = icon}
      <span class="addon-label">
        <Icon size={34} weight="thin" />
        {#if caption}<span>{caption}</span>{/if}
      </span>
    {:else if preview && preview !== brokenPreview}
      <img
        src={preview}
        alt=""
        loading="lazy"
        decoding="async"
        class:loaded
        onload={() => (loaded = true)}
        onerror={() => (brokenPreview = preview)}
      />
    {:else}
      {@const Icon = icon}
      <span class="placeholder"><Icon size={40} weight="thin" /></span>
    {/if}

    <!-- Слева только галочка выбора, справа — что это за мод и его состояние.
         Тип не прячется ни при наведении, ни пока выбрано несколько: выбирая
         галочками, видно, машина это или карта. -->
    <span class="tags">
      <span class="kind-tag {tag}"><KindIcon size={12} weight="bold" />{typeLabel(tag)}</span>
      {#if status}
        {@const StatusIcon = statusIcon(status.tone)}
        <span class="status {status.tone}"><StatusIcon size={12} weight="bold" />{status.text}</span>
      {/if}
    </span>
    {#if download}
      <span
        class="loading"
        class:unknown={download.percent === null}
        role="progressbar"
        aria-label="Скачивается обновление"
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={download.percent ?? undefined}
        title={download.percent === null ? "Скачивается обновление" : `Скачивается: ${download.percent} %`}
      >
        <span style={`--p:${(download.percent ?? 0) / 100}`}></span>
      </span>
    {/if}
    {#if (showUpdate && onupdate) || (actions.page && onopenpage)}
      <!-- Быстрые действия под курсором. С клавиатуры то же есть в меню карточки
           (Shift+F10), поэтому в порядок Tab кнопки не встают. -->
      <span class="quick">
        {#if showUpdate && onupdate}
          <button
            class="quick-button primary"
            type="button"
            tabindex="-1"
            title={mod.updateVersion ? `Обновить до ${mod.updateVersion}` : "Открыть страницу обновления"}
            onclick={() => onupdate(mod)}
          >
            <DownloadSimple size={13} weight="bold" />
            Обновить
          </button>
        {/if}
        {#if actions.page && onopenpage}
          <button
            class="quick-button"
            type="button"
            tabindex="-1"
            aria-label="Открыть сайт мода"
            title="Открыть сайт мода"
            onclick={() => onopenpage(mod)}
          >
            <ArrowSquareOut size={13} weight="bold" />
          </button>
        {/if}
      </span>
    {/if}
    {#if installedFlash.id === mod.id}
      <!-- Только что поставлен: отметка сама тает. Читалкам экрана об установке
           уже сказал тост. -->
      <span class="installed installed-life" aria-hidden="true"><DrawCheck size={15} />Установлен</span>
    {/if}
    {#if copies}
      <span
        class="copies"
        class:conflict={mod.groupConflict}
        class:flash={conflictFlash.id === mod.id}
        title={mod.groupConflict
          ? "Включено несколько копий — игра загрузит все"
          : "Остальные копии — в панели мода"}
      >
        <Copy size={12} weight="bold" />{copies}
      </span>
    {/if}
  </div>

  <div class="body">
    <div class="text">
      <p class="name">{mod.name}</p>
      <p class="meta">
        {#if version}
          <!-- Версия сменилась (нашли обновление или поставили его) — новая
               прокручивается снизу. При первом показе карточки не играет. -->
          {#key version.text}
            <span class="mono version" class:fresh={version.fresh} in:rollIn>{version.text}</span>
          {/key}
        {/if}
        <span>{formatSize(mod.sizeBytes)}</span>
        {#if mod.deleted}
          <span>{mod.copies > 1 ? `${mod.copies} ${plural(mod.copies, "копия", "копии", "копий")}` : "копия"}</span>
        {:else if compat}
          <span class="compat" title={compat.title}>{compat.text}</span>
        {/if}
      </p>
    </div>
    {#if !mod.deleted}
      <div class="toggle">
        <Switch
          checked={mod.enabled}
          onCheckedChange={() => ontoggle(mod)}
          aria-label={toggleLabel(mod.enabled, mod.name)}
        />
      </div>
    {/if}
  </div>
</div>

<style>
  .card {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 206px;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    cursor: default;
    transition:
      transform var(--dur-2) var(--ease-out),
      border-color var(--dur-2) var(--ease-out),
      box-shadow var(--dur-2) var(--ease-out);
  }

  .card.appear {
    animation: rise-in var(--dur-3) var(--ease-out) both;
    animation-delay: calc(var(--i) * 18ms);
  }

  /* Карточка, въезжающая в экран при прокрутке, мягко проявляется. Ведёт
     прокрутка, а не таймер: без перерисовки, на композиторе. Свои свойства
     (translate, scale), а не transform: нажатие (transform) и анимация не спорят. */
  @supports (animation-timeline: view()) {
    :global(html:not(.no-motion)) .card:not(.appear) {
      animation: card-enter linear both;
      animation-timeline: view();
      animation-range: entry 0% entry 80%;
    }
  }

  @keyframes card-enter {
    from {
      opacity: 0.3;
      translate: 0 16px;
      scale: 0.97;
    }
  }

  /* Без сдвига при наведении: карточка под краем курсора начинала дрожать. */
  .card:hover {
    border-color: var(--border-strong);
    box-shadow: var(--shadow-1);
  }

  /* Нажатие по карточке (её кнопка выбора .select на всю площадь): она чуть
     проседает, как кнопка. Не тогда, когда жмут галочку или переключатель на ней:
     у них свой отклик. */
  .card:has(.select:active) {
    transform: scale(0.985);
    transition-duration: var(--dur-1);
  }

  .card.selected {
    border-color: var(--accent-brand);
    box-shadow: 0 0 0 1px var(--accent-brand);
  }

  .select {
    position: absolute;
    inset: 0;
    z-index: 1;
    padding: 0;
    border: 0;
    border-radius: inherit;
    background: transparent;
    cursor: default;
  }

  .select:focus-visible {
    outline: 2px solid var(--ring);
    outline-offset: 2px;
  }

  /* Галочка выбора: появляется в левом углу при наведении, с клавиатуры и пока
     что-то выбрано. Скрыта прозрачностью — фокус и клик доступны всегда. */
  .check {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    /* Без размытия: на сетке из тысяч карточек blur — заметная нагрузка. */
    background: color-mix(in oklab, var(--surface) 92%, transparent);
    color: var(--foreground);
    opacity: 0;
    cursor: pointer;
    transition:
      opacity var(--dur-1) var(--ease-out),
      background-color var(--dur-1) var(--ease-out),
      border-color var(--dur-1) var(--ease-out);
  }

  .card:hover .check,
  .card.selecting .check,
  .check:focus-visible,
  .check[aria-checked="true"] {
    opacity: 1;
  }

  /* Тип и состояние — столбиком в правом углу, левый угол свободен для галочки. */
  .tags {
    position: absolute;
    top: 10px;
    right: 10px;
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 4px;
    max-width: calc(100% - 50px);
  }

  .check:focus-visible {
    outline: 2px solid var(--ring);
    outline-offset: 2px;
  }

  .check[aria-checked="true"] {
    border-color: var(--accent-brand);
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .thumb {
    position: relative;
    flex: none;
    height: 140px;
    overflow: hidden;
    background: var(--surface-2);
  }

  /* Быстрые действия выезжают снизу обложки при наведении. Над кнопкой выбора
     карточки (.select), иначе клик выбирал бы карточку. */
  .quick {
    position: absolute;
    right: 8px;
    bottom: 8px;
    z-index: 2;
    display: flex;
    gap: 6px;
    opacity: 0;
    transform: translateY(6px);
    transition:
      opacity var(--dur-2) var(--ease-out),
      transform var(--dur-2) var(--ease-spring);
    pointer-events: none;
  }

  /* Только под курсором: с клавиатуры кнопки недоступны (tabindex -1), их
     действия — в меню карточки, и показывать их по фокусу значило бы обещать
     то, до чего Tab не дойдёт. */
  .card:hover .quick {
    opacity: 1;
    transform: none;
    pointer-events: auto;
  }

  /* Пометка «N копии» в том же углу — уступает место кнопкам. */
  .copies {
    transition: opacity var(--dur-2) var(--ease-out);
  }

  .card:hover:has(.quick) .copies {
    opacity: 0;
  }

  /* Идёт выбор нескольких — карточка работает как галочка, действия не мешают. */
  .card.selecting .quick {
    display: none;
  }

  .quick-button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 26px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 7px;
    background: color-mix(in oklab, var(--surface-2) 88%, transparent);
    color: var(--foreground);
    font-size: 12px;
    font-weight: 700;
    cursor: default;
    box-shadow: var(--shadow-1);
  }

  .quick-button:hover {
    border-color: var(--accent-brand);
  }

  .quick-button:active {
    transform: translateY(1px);
  }

  .quick-button.primary {
    border-color: transparent;
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  /* Полоска хода обновления по низу обложки. Двигается только transform —
     перерисовки нет, пока файл качается. */
  .loading {
    position: absolute;
    inset: auto 0 0;
    height: 3px;
    overflow: hidden;
    background: color-mix(in oklab, var(--background) 55%, transparent);
    pointer-events: none;
  }

  .loading > span {
    position: absolute;
    inset: 0;
    background: var(--accent-brand);
    transform: scaleX(var(--p));
    transform-origin: left;
    transition: transform var(--dur-3) var(--ease-out);
  }

  .loading.unknown > span {
    width: 35%;
    transform: none;
    animation: loading-run 1.2s var(--ease-in-out) infinite;
  }

  /* «Быстро» оборвал бы бегущую полоску на последнем кадре — за краем обложки.
     Без движения она стоит во всю ширину, бледнее: видно, что идёт загрузка. */
  :global(.no-motion) .loading.unknown > span {
    width: 100%;
    animation: none;
    opacity: 0.45;
  }

  @keyframes loading-run {
    from {
      translate: -100% 0;
    }
    to {
      translate: 290% 0;
    }
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0;
    transition:
      opacity var(--dur-3) var(--ease-out),
      transform var(--dur-3) var(--ease-out),
      filter var(--dur-2) var(--ease-out);
  }

  .thumb img.loaded {
    opacity: 1;
  }

  .card:hover .thumb img {
    transform: scale(1.04);
  }

  .placeholder {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    color: var(--faint-foreground);
    opacity: 0.6;
  }

  .thumb.addon {
    background: repeating-linear-gradient(
      135deg,
      var(--surface-2) 0 12px,
      color-mix(in oklab, var(--surface-2) 70%, var(--surface-3)) 12px 24px
    );
  }

  .addon-label {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    color: var(--muted-foreground);
    font-size: 12px;
    font-weight: 700;
  }


  .status {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    max-width: 100%;
    overflow: hidden;
    padding: 0 8px;
    border-radius: 5px;
    font-size: 11px;
    font-weight: 700;
    text-overflow: ellipsis;
    white-space: nowrap;
    animation: pop-in var(--dur-3) var(--ease-out) both;
  }

  .status.update {
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .status.possible {
    border: 1px solid var(--warning-border);
    background: color-mix(in oklab, var(--background) 82%, transparent);
    color: var(--warning);
  }

  .status.problem {
    border: 1px solid color-mix(in oklab, var(--danger) 45%, transparent);
    background: color-mix(in oklab, var(--background) 82%, transparent);
    color: var(--danger);
  }

  .status.off,
  .status.deleted,
  .status.pending {
    border: 1px solid var(--border-strong);
    background: color-mix(in oklab, var(--background) 82%, transparent);
    color: var(--muted-foreground);
  }

  .installed {
    position: absolute;
    left: 50%;
    top: 50%;
    z-index: 3;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    margin: -14px 0 0;
    padding: 0 12px 0 8px;
    border: 1px solid color-mix(in oklab, var(--ok) 45%, transparent);
    border-radius: 999px;
    background: var(--background);
    color: var(--ok);
    font-size: 12px;
    font-weight: 700;
    white-space: nowrap;
    translate: -50% 0;
    pointer-events: none;
    box-shadow: 0 6px 18px -8px color-mix(in oklab, var(--ok) 60%, transparent);
  }

  .copies {
    position: absolute;
    right: 10px;
    bottom: 10px;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 22px;
    padding: 0 8px;
    border: 1px solid var(--border-strong);
    border-radius: 5px;
    background: var(--background);
    color: var(--foreground);
    font-size: 11px;
    font-weight: 700;
  }

  .copies.conflict {
    border-color: color-mix(in oklab, var(--danger) 60%, transparent);
    color: var(--danger);
  }

  /* Только что включились две копии: метка вздрагивает и вспыхивает. Под
     курсором кнопки её прячут — вспышку показываем поверх них. */
  .copies.flash {
    z-index: 3;
    opacity: 1 !important;
    animation: copies-flash 900ms var(--ease-out);
  }

  @keyframes copies-flash {
    0%,
    100% {
      transform: none;
      box-shadow: 0 0 0 0 transparent;
    }
    15% {
      transform: translateX(-3px);
      box-shadow: 0 0 0 4px color-mix(in oklab, var(--danger) 35%, transparent);
    }
    30% {
      transform: translateX(3px);
    }
    45% {
      transform: translateX(-2px);
    }
    60% {
      transform: translateX(1px);
      box-shadow: 0 0 0 6px transparent;
    }
  }

  .body {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 0;
    padding: 0 12px;
  }

  .text {
    flex: 1;
    min-width: 0;
  }

  .name {
    display: -webkit-box;
    margin: 0;
    overflow: hidden;
    font-size: 14.5px;
    font-weight: 700;
    line-height: 1.25;
    overflow-wrap: anywhere;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    transition: color var(--dur-2) var(--ease-out);
  }

  /* По базовой линии: моноширинная версия мельче и без этого «висела» выше «3 МБ». */
  .meta {
    display: flex;
    align-items: baseline;
    gap: 8px;
    margin: 2px 0 0;
    overflow: hidden;
    color: var(--muted-foreground);
    font-size: 12px;
    white-space: nowrap;
  }

  .version {
    color: var(--foreground);
  }

  .version.fresh {
    color: var(--accent-brand-text);
  }

  .compat {
    color: var(--warning);
  }

  .toggle {
    position: relative;
    z-index: 2;
    flex: none;
  }

  .sleeping .thumb > img,
  .sleeping .thumb .placeholder {
    filter: grayscale(0.9) brightness(0.65);
  }

  .sleeping .name {
    color: var(--muted-foreground);
  }

  .deleted {
    opacity: 0.6;
  }
</style>

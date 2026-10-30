<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWebview } from "@tauri-apps/api/webview";
  import {
    archiveBaseName,
    incomingDiscard,
    incomingInspect,
    incomingInstall,
    incomingRemoveSource,
    incomingUndo,
    isArchivePath,
    type IncomingArchiveDto,
    type IncomingItemDto,
    type IncomingTicket,
  } from "#lib/ipc/incoming.js";
  import { formatSize, plural } from "#lib/features/garage/format.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import { incomingQueue } from "./queue.svelte";
  import { trapFocus } from "#lib/shell/focus.js";
  import { pop, softFade } from "#lib/shell/transitions.js";
  import { installable, preselected, suspiciousFiles, suspiciousText } from "./selection";

  type CheckLevel = "ok" | "attention" | "blocked";
  interface Check {
    level: CheckLevel;
    text: string;
  }
  interface PanelInstalled {
    tickets: IncomingTicket[];
    fileNames: string[];
    message: string;
    warning?: string | null;
  }

  let current = $state<IncomingArchiveDto | null>(null);
  let currentDownloaded = $state(false);
  let busy = $state(false);
  let busyName = $state("");
  let selected = $state<boolean[]>([]);
  let installing = $state(false);
  let dragActive = $state(false);
  let installButton = $state<HTMLButtonElement | null>(null);
  let session = 0;
  let pumping = false;

  const items = $derived(current?.items ?? []);
  const dialogOpen = $derived(busy || current !== null);
  const picked = $derived(items.filter((item, index) => selected[index] && installable(item)));
  const several = $derived(
    items.length > 1 || items.some((item) => suspiciousFiles(item).length > 0),
  );
  const hasUpdate = $derived(picked.some((item) => item.kind === "update"));
  const canInstall = $derived(
    !busy &&
      !installing &&
      current !== null &&
      current.problem === null &&
      picked.length > 0,
  );
  const totalSize = $derived(items.reduce((sum, item) => sum + item.size, 0));

  const installLabel = $derived.by(() => {
    if (picked.length > 1) {
      return `Установить ${picked.length} ${plural(picked.length, "мод", "мода", "модов")}`;
    }
    if (picked.length === 1 && picked[0].kind === "update") return "Обновить";
    return "Установить";
  });

  const title = $derived.by(() => {
    if (busy) return `Проверяем «${shortName(busyName)}»…`;
    if (!current) return "";
    const name = shortName(archiveBaseName(current.source));
    if (current.problem) return `В «${name}» нечего установить`;
    if (current.items.length === 1) return `Архив «${name}» проверен`;
    return `В архиве «${name}» ${current.items.length} ${plural(current.items.length, "мод", "мода", "модов")}`;
  });

  $effect(() => {
    incomingQueue.revision;
    if (!current && !busy) void pump();
  });

  // Фокус на «Установить» — только когда пришёл новый архив. Раньше эффект срабатывал
  // и на галочку (от неё зависит canInstall): фокус уезжал с чекбокса, и следующий
  // пробел запускал установку.
  let focusedArchive: unknown = null;
  $effect(() => {
    const archive = current;
    if (busy || !archive || archive === focusedArchive) return;
    focusedArchive = archive;
    if (untrack(() => canInstall)) void tick().then(() => installButton?.focus());
  });

  onMount(() => {
    if (!isTauri()) {
      window.addEventListener("dragenter", onHtmlDragEnter);
      window.addEventListener("dragover", onHtmlDragOver);
      window.addEventListener("dragleave", onHtmlDragLeave);
      window.addEventListener("drop", onHtmlDrop);
      return () => {
        window.removeEventListener("dragenter", onHtmlDragEnter);
        window.removeEventListener("dragover", onHtmlDragOver);
        window.removeEventListener("dragleave", onHtmlDragLeave);
        window.removeEventListener("drop", onHtmlDrop);
      };
    }

    let unlisten: (() => void) | undefined;
    let unlistenDownload: (() => void) | undefined;
    let unlistenPanel: (() => void) | undefined;
    let disposed = false;
    void listen<{ path: string }>("incoming://download", (event) => {
      const path = event.payload.path;
      if (isArchivePath(path)) incomingQueue.add([path], true);
    })
      .then((stop) => {
        if (disposed) stop();
        else unlistenDownload = stop;
      })
      .catch(() => {});
    void listen<PanelInstalled>("download://installed", (event) => {
      if (!disposed) panelInstalled(event.payload);
    })
      .then((stop) => {
        if (disposed) stop();
        else unlistenPanel = stop;
      })
      .catch(() => {});
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "enter") {
          dragActive = payload.paths.some(isArchivePath) && !dialogOpen;
        } else if (payload.type === "leave") {
          dragActive = false;
        } else if (payload.type === "drop") {
          dragActive = false;
          // Всё брошенное — в очередь, даже поверх открытого окна: ничего не теряем.
          const archives = payload.paths.filter(isArchivePath);
          if (archives.length > 0) incomingQueue.add(archives);
        }
      })
      .then((stop) => {
        if (disposed) stop();
        else unlisten = stop;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
      unlistenDownload?.();
      unlistenPanel?.();
    };
  });

  async function pump(): Promise<void> {
    // Не забираем следующий архив, пока открыт текущий или идёт установка:
    // иначе окно подменится, а workDir неудачного архива потеряется.
    if (pumping || installing || current || busy) return;
    const item = incomingQueue.shift();
    if (!item) return;
    const path = item.path;
    const downloaded = item.downloaded;
    pumping = true;
    const token = ++session;
    busy = true;
    busyName = archiveBaseName(path);
    try {
      const [archive] = await incomingInspect([path]);
      if (token !== session) {
        if (archive) void incomingDiscard(archive.workDir).catch(() => {});
        return;
      }
      if (!archive) throw new Error("Не удалось проверить архив.");
      openArchive(archive, downloaded);
    } catch (reason) {
      if (token === session) {
        openArchive(
          {
            source: path,
            problem: errorText(reason),
            workDir: "",
            items: [],
          },
          downloaded,
        );
      }
    } finally {
      if (token === session) busy = false;
      pumping = false;
      if (!current && !busy) void pump();
    }
  }

  function openArchive(archive: IncomingArchiveDto, downloaded = false): void {
    current = archive;
    currentDownloaded = downloaded;
    selected = archive.items.map(preselected);
  }

  function close(): void {
    if (installing) return;
    if (busy) {
      session += 1;
      busy = false;
      busyName = "";
    }
    const archive = current;
    current = null;
    currentDownloaded = false;
    selected = [];
    if (archive?.workDir) void incomingDiscard(archive.workDir).catch(() => {});
    void pump();
  }

  function onkeydown(event: KeyboardEvent): void {
    if (event.key === "Escape" && dialogOpen) {
      event.preventDefault();
      close();
    }
  }

  async function install(): Promise<void> {
    if (!canInstall || !current) return;
    const archive = current;
    const chosen = archive.items
      .map((item, index) => ({ item, index }))
      .filter(({ item, index }) => selected[index] && installable(item))
      .map(({ item }) => item);
    if (chosen.length === 0) return;
    installing = true;
    try {
      const results = await incomingInstall(
        chosen.map((item) => ({
          zipPath: item.zipPath,
          fileName: item.fileName,
          replaces: item.replaces,
          allowWarnings: true,
        })),
      );
      const tickets: IncomingTicket[] = [];
      const failures: { title: string; message: string }[] = [];
      const warnings: string[] = [];
      results.forEach((result, index) => {
        if (result.success) {
          if (result.warning) warnings.push(result.warning);
          tickets.push({
            fileName: result.fileName,
            replaces: result.replaces,
            backupId: result.backupId,
            wasDisabled: result.wasDisabled,
            sameName: result.sameName,
          });
        } else {
          failures.push({
            title: chosen[index]?.title ?? result.fileName,
            message: result.message ?? "установить не получилось",
          });
        }
      });
      const workDir = archive.workDir;
      const leftInArchive = archive.items.some(
        (item, index) => installable(item) && !selected[index],
      );
      if (currentDownloaded && failures.length === 0 && !leftInArchive) {
        void incomingRemoveSource(archive.source).catch(() => {});
      }
      session += 1;
      current = null;
      currentDownloaded = false;
      selected = [];
      if (workDir) void incomingDiscard(workDir).catch(() => {});
      window.dispatchEvent(
        new CustomEvent("bmm:library-changed", {
          detail: { fileNames: tickets.map((ticket) => ticket.fileName) },
        }),
      );
      const text = warnings.length > 0
        ? `${toastText(chosen, tickets.length, failures)} ${warnings[0]}`
        : toastText(chosen, tickets.length, failures);
      toasts.show(text, {
        undo:
          tickets.length > 0
            ? async () => {
                let problem: string | null = null;
                for (let index = tickets.length - 1; index >= 0; index -= 1) {
                  try {
                    await incomingUndo(tickets[index]);
                  } catch (reason) {
                    problem ??= errorText(reason);
                  }
                }
                window.dispatchEvent(new CustomEvent("bmm:library-changed"));
                if (problem) throw new Error(problem);
                toasts.show("Установка отменена");
              }
            : undefined,
      });
    } catch (reason) {
      toasts.show(errorText(reason));
    } finally {
      installing = false;
      void pump();
    }
  }

  async function undoTickets(tickets: IncomingTicket[]): Promise<void> {
    let problem: string | null = null;
    for (let index = tickets.length - 1; index >= 0; index -= 1) {
      try {
        await incomingUndo(tickets[index]);
      } catch (reason) {
        problem ??= errorText(reason);
      }
    }
    window.dispatchEvent(new CustomEvent("bmm:library-changed"));
    if (problem) throw new Error(problem);
    toasts.show("Установка отменена");
  }

  function panelInstalled(event: PanelInstalled): void {
    window.dispatchEvent(
      new CustomEvent("bmm:library-changed", {
        detail: { fileNames: event.fileNames },
      }),
    );
    const message = event.warning
      ? `${event.message} ${event.warning}`
      : event.message;
    toasts.show(message, {
      undo: event.tickets.length > 0 ? () => undoTickets(event.tickets) : undefined,
    });
  }

  function toggle(index: number, event: Event): void {
    selected[index] = (event.currentTarget as HTMLInputElement).checked;
  }

  function kindText(item: IncomingItemDto): string {
    if (item.kind === "alreadyInstalled") {
      return `Уже установлен (${item.replaces ?? item.fileName})`;
    }
    if (item.kind === "update") {
      const same = item.replaces?.toLowerCase() === item.fileName.toLowerCase();
      const target = same || !item.replaces ? "" : `: заменит ${item.replaces}`;
      const off = item.replacesEnabled ? "" : ", мод останется выключенным";
      return `Обновление${target}${off}`;
    }
    return "Новый мод";
  }

  function checksOf(item: IncomingItemDto): Check[] {
    const checks: Check[] = [];
    for (const finding of uniqueFindings(item, "block")) {
      checks.push({ level: "blocked", text: trimDot(finding.message) });
    }
    if (!checks.length) {
      checks.push({ level: "ok", text: "Опасных файлов и путей нет" });
    }
    if (item.repacked) {
      checks.push({
        level: "ok",
        text: "Файлы лежали в лишней папке — перепаковали, чтобы игра увидела мод",
      });
    }
    for (const finding of uniqueFindings(item, "warning")) {
      if (finding.code === "nested-root") continue;
      if (finding.code === "executable") {
        const files = suspiciousFiles(item);
        checks.push({
          level: "attention",
          text: `${trimDot(finding.message)}: ${suspiciousText(files)}. Отметьте мод, чтобы поставить`,
        });
        continue;
      }
      checks.push({ level: "attention", text: trimDot(finding.message) });
    }
    if (item.similarTo) {
      checks.push({
        level: "attention",
        text: `Похоже, такая машина уже стоит: «${item.similarTo.title}» (${item.similarTo.fileName}). В игре будут две похожие машины`,
      });
    }
    if (item.locked) {
      const folder = item.replacesSubFolder ?? "";
      checks.push({
        level: "blocked",
        text:
          folder.toLowerCase() === "repo"
            ? "Установленная версия скачана в самой игре — обновите её там, в «Репозитории», иначе в игре будет два одинаковых мода"
            : `Установленная версия лежит в подпапке «${folder}» — замените её там сами, иначе в игре будет два одинаковых мода`,
      });
    }
    if (item.blocked && !item.findings.some((finding) => finding.severity === "block")) {
      checks.push({ level: "blocked", text: "Архив не прошёл проверку безопасности" });
    }
    return checks;
  }

  function uniqueFindings(
    item: IncomingItemDto,
    severity: "warning" | "block",
  ): IncomingItemDto["findings"] {
    const seen = new Set<string>();
    const result: IncomingItemDto["findings"] = [];
    for (const finding of item.findings) {
      if (finding.severity !== severity || seen.has(finding.code)) continue;
      seen.add(finding.code);
      result.push(finding);
    }
    return result;
  }

  function toastText(
    chosen: IncomingItemDto[],
    installed: number,
    failures: { title: string; message: string }[],
  ): string {
    if (failures.length === 0) {
      if (installed === 1) {
        return chosen[0]?.kind === "update"
          ? `Обновлён «${shortName(chosen[0].title, 40)}»`
          : `Установлен «${shortName(chosen[0].title, 40)}»`;
      }
      return `Установлено ${installed} ${plural(installed, "мод", "мода", "модов")}`;
    }
    const first = failures[0];
    if (installed === 0) {
      return `Не удалось установить «${shortName(first.title, 40)}»: ${lowerFirst(first.message)}`;
    }
    return `Установлено ${installed} из ${chosen.length}. «${shortName(first.title, 32)}»: ${lowerFirst(first.message)}`;
  }

  function trimDot(text: string): string {
    return text.replace(/\.+$/, "");
  }

  function lowerFirst(text: string): string {
    if (!text) return text;
    return text.charAt(0).toLocaleLowerCase("ru") + text.slice(1);
  }

  function shortName(name: string, limit = 48): string {
    return name.length > limit ? `${name.slice(0, limit - 1)}…` : name;
  }

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось проверить архив.";
  }

  function hasFiles(event: DragEvent): boolean {
    return event.dataTransfer?.types.includes("Files") ?? false;
  }

  function onHtmlDragEnter(event: DragEvent): void {
    if (!hasFiles(event) || dialogOpen) return;
    event.preventDefault();
    dragActive = true;
  }

  function onHtmlDragOver(event: DragEvent): void {
    if (!hasFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) {
      event.dataTransfer.dropEffect = dialogOpen ? "none" : "copy";
    }
  }

  function onHtmlDragLeave(event: DragEvent): void {
    if (event.relatedTarget === null) dragActive = false;
  }

  function onHtmlDrop(event: DragEvent): void {
    event.preventDefault();
    dragActive = false;
    const files = Array.from(event.dataTransfer?.files ?? []);
    const archives = files.map((file) => file.name).filter(isArchivePath);
    if (archives.length > 0) incomingQueue.add(archives);
  }
</script>

<svelte:window onkeydown={onkeydown} />

{#if dragActive && !dialogOpen}
  <div class="drop-layer" aria-hidden="true" transition:softFade={{ duration: 160 }}>
    <div class="drop-card glass solid liquid-edge">
      <svg width="44" height="44" viewBox="0 0 44 44" fill="none">
        <path
          d="M22 6v21M13.5 19 22 27.5 30.5 19M8 34h36"
          stroke="currentColor"
          stroke-width="2.2"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
      </svg>
      <p>Отпустите — проверим и предложим установить</p>
      <span>zip, rar или 7z</span>
    </div>
  </div>
{/if}

{#if dialogOpen}
  <button
    class="backdrop"
    type="button"
    aria-label="Закрыть окно проверки"
    transition:softFade={{ duration: 160 }}
    onclick={close}
  ></button>
  <div
    class="dialog glass"
    role="dialog"
    aria-modal="true"
    aria-labelledby="incoming-title"
    use:trapFocus
    transition:pop={{ duration: 240, y: 10, centered: "both" }}
  >
    <header>
      <div class="heading">
        <h2 id="incoming-title">{title}</h2>
        {#if busy}
          <p class="hint">Распаковываем, проверяем и сравниваем с установленными модами…</p>
        {:else if current && current.items.length > 0}
          <p class="hint">
            {current.items.length}
            {plural(current.items.length, "мод", "мода", "модов")} · {formatSize(totalSize)}
            {#if incomingQueue.count > 0}
              · ещё {incomingQueue.count} в очереди
            {/if}
          </p>
        {/if}
      </div>
      <button
        class="icon"
        type="button"
        aria-label="Закрыть"
        onclick={close}
        disabled={installing}
      >
        ×
      </button>
    </header>

    {#if busy}
      <div class="busy-bar"><span></span></div>
    {:else if current}
      {#if current.problem}
        <p class="problem">{current.problem}</p>
      {:else}
        <div class="items scroll-area">
          {#each current.items as item, index (item.zipPath + index)}
            <article class="item" class:blocked={item.blocked || item.locked}>
              <div class="item-head">
                <div class="titles">
                  <h3>{item.title}</h3>
                  <p>
                    {kindText(item)} · {item.fileName} · {formatSize(item.size)}
                  </p>
                </div>
                {#if several}
                  <label class="check" class:disabled={!installable(item)}>
                    <input
                      type="checkbox"
                      checked={selected[index] ?? false}
                      disabled={!installable(item)}
                      onchange={(event) => toggle(index, event)}
                      aria-label={`Установить ${item.title}`}
                    />
                    <span aria-hidden="true"></span>
                  </label>
                {/if}
              </div>
              <ul class="checks">
                {#each checksOf(item) as check}
                  <li class={check.level}>
                    <span class="mark" aria-hidden="true">
                      {check.level === "ok" ? "✓" : check.level === "attention" ? "!" : "✕"}
                    </span>
                    <span>{check.text}</span>
                  </li>
                {/each}
              </ul>
            </article>
          {/each}
        </div>
        {#if hasUpdate}
          <p class="note">
            Прошлая версия сохранится — её можно вернуть в «Прошлых версиях»
          </p>
        {/if}
      {/if}
    {/if}

    {#if !busy}
      <footer>
        {#if current?.problem}
          <button class="primary" type="button" onclick={close}>Понятно</button>
        {:else}
          <button class="link" type="button" onclick={close} disabled={installing}>
            Отмена
          </button>
          <button
            bind:this={installButton}
            class="primary"
            type="button"
            onclick={() => void install()}
            disabled={!canInstall}
          >
            {installing ? "Устанавливаем…" : installLabel}
          </button>
        {/if}
      </footer>
    {/if}
  </div>
{/if}

<style>
  .drop-layer {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    background: color-mix(in oklab, var(--background) 55%, transparent);
    pointer-events: none;
  }

  .drop-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: min(420px, calc(100vw - 48px));
    padding: 32px 24px;
    border-radius: 14px;
    color: var(--foreground);
  }

  .drop-card svg {
    margin-bottom: 10px;
  }

  .drop-card p {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    text-align: center;
  }

  .drop-card span {
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 55;
    border: 0;
    padding: 0;
    background: var(--overlay);
    cursor: default;
  }

  .dialog {
    position: fixed;
    top: 50%;
    left: 50%;
    z-index: 60;
    display: flex;
    flex-direction: column;
    gap: 14px;
    width: min(600px, calc(100vw - 48px));
    max-height: min(760px, calc(100vh - 72px));
    padding: 24px 26px 22px;
    border-radius: 14px;
    color: var(--foreground);
    transform: translate(-50%, -50%);
  }

  header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
  }

  .heading {
    min-width: 0;
  }

  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 650;
    letter-spacing: -0.01em;
    line-height: 1.25;
    overflow-wrap: anywhere;
  }

  .hint {
    margin: 5px 0 0;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .icon {
    flex: none;
    width: 30px;
    height: 30px;
    border: 1px solid var(--glass-border);
    border-radius: 8px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 19px;
    line-height: 1;
    cursor: default;
  }

  .icon:hover:not(:disabled) {
    color: var(--foreground);
  }

  .icon:disabled {
    opacity: 0.4;
  }

  .busy-bar {
    position: relative;
    overflow: hidden;
    height: 3px;
    margin: 8px 2px 4px;
    border-radius: 8px;
    background: color-mix(in oklab, var(--foreground) 10%, transparent);
  }

  .busy-bar span {
    position: absolute;
    inset: 0 auto 0 0;
    width: 38%;
    border-radius: inherit;
    background: linear-gradient(90deg, transparent, var(--foreground), transparent);
    animation: sheen 1.3s ease-in-out infinite;
  }

  .problem {
    margin: 4px 2px;
    color: var(--muted-foreground);
    font-size: 13.5px;
    line-height: 1.5;
  }

  /* Список забирает всё свободное место окна и прокручивается сам; карточки не сжимаются,
     иначе при большом модпаке текст вылезал за их края. */
  .items {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: 10px;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding-right: 4px;
  }

  .dialog > :not(.items) {
    flex: none;
  }

  .item {
    flex: none;
    padding: 15px 16px 13px;
    border: 1px solid var(--glass-border);
    border-radius: 10px;
    background: color-mix(in oklab, var(--foreground) 4%, transparent);
  }

  .item.blocked {
    border-color: color-mix(in oklab, var(--danger) 40%, transparent);
  }

  .item-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
  }

  .titles {
    min-width: 0;
  }

  .titles h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .titles p {
    margin: 3px 0 0;
    color: var(--muted-foreground);
    font-size: 12px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .checks {
    display: flex;
    flex-direction: column;
    gap: 5px;
    margin: 11px 0 0;
    padding: 0;
    list-style: none;
  }

  .checks li {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    font-size: 12.5px;
    line-height: 1.42;
  }

  .mark {
    flex: none;
    width: 14px;
    text-align: center;
    font-weight: 700;
  }

  .checks li.ok .mark {
    color: var(--muted-foreground);
  }

  .checks li.attention .mark {
    color: var(--warning);
  }

  .checks li.blocked {
    color: var(--danger);
  }

  .checks li.blocked .mark {
    color: var(--danger);
  }

  .check {
    flex: none;
    position: relative;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    cursor: default;
  }

  .check input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }

  .check span {
    display: grid;
    place-items: center;
    width: 19px;
    height: 19px;
    border: 1px solid var(--glass-border);
    border-radius: 6px;
    color: var(--primary-foreground);
    font-size: 12px;
    font-weight: 700;
  }

  .check input:checked + span {
    border-color: transparent;
    background: var(--primary);
  }

  .check input:checked + span::after {
    content: "✓";
  }

  .check input:focus-visible + span {
    outline: 2px solid var(--ring);
    outline-offset: 2px;
  }

  .check.disabled {
    opacity: 0.35;
  }

  .note {
    margin: 0 2px;
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.45;
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 2px;
  }

  footer button {
    height: 34px;
    border-radius: 8px;
    font-size: 12px;
    font-weight: 600;
    cursor: default;
  }

  .primary {
    min-width: 132px;
    padding: 0 18px;
    border: 1px solid transparent;
    background: var(--primary);
    color: var(--primary-foreground);
  }

  .primary:hover:not(:disabled) {
    background: color-mix(in oklab, var(--primary) 86%, white);
  }

  .primary:disabled {
    opacity: 0.45;
  }

  .link {
    padding: 0 12px;
    border: 0;
    background: transparent;
    color: var(--muted-foreground);
  }

  .link:hover:not(:disabled) {
    color: var(--foreground);
  }


  @keyframes sheen {
    from {
      transform: translateX(-120%);
    }
    to {
      transform: translateX(320%);
    }
  }
</style>

<script lang="ts">
  import { errorText } from "#lib/shell/errorText.js";
  import { onMount } from "svelte";
  import { trapFocus } from "#lib/shell/focus.js";
  import { updatesDismiss } from "#lib/ipc/updates.js";
  import { StaleGuard } from "#lib/features/download/staleGuard.js";
  import { isTauri } from "@tauri-apps/api/core";
  import { emit } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import DrawCheck from "#lib/shell/DrawCheck.svelte";
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
    wholeArchiveInstalled,
    preselected,
    suspiciousFiles,
    suspiciousText,
  } from "#lib/features/incoming/selection.js";
  import { formatSize, plural } from "#lib/features/garage/format.js";
  import { safeExternalUrl } from "#lib/url.js";
  import {
    downloadBatch,
    downloadBatchGoto,
    downloadClose,
    downloadJobs,
    downloadPanelState,
    downloadReady,
    listenDownloadBatch,
    listenDownloadBatchAdded,
    listenDownloadCloseAsked,
    listenDownloadContext,
    listenDownloadJobs,
    listenDownloadProgress,
    listenDownloadReady,
    listenDownloadState,
    type DownloadJob,
    type DownloadRequest,
  } from "#lib/ipc/download.js";
  import {
    closeQuestion,
    downloading,
    jobForPage,
    jobName,
    jobRows,
    restarted,
    track,
    viewOf,
    type Tracked,
  } from "#lib/features/download/jobs.js";
  import {
    batchLine,
    collect,
    installRequests,
    modsLabel,
    sameAsInstalled,
    nextPosition,
    pickedItems,
    positionOf,
    type Collected,
  } from "#lib/features/download/batch.js";

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
  // Загрузок бывает несколько сразу: у каждой свой мод и свой ход. Докачанные архивы
  // проверяются по одному, остальные ждут в очереди.
  let jobs = $state<DownloadJob[]>([]);
  let tracked = $state<Record<string, Tracked>>({});
  let queue = $state<DownloadJob[]>([]);
  /** Загрузка, чей архив сейчас проверяем или показываем к установке. */
  let reviewing = $state<DownloadJob | null>(null);
  /** Архивы, уже взятые в проверку: второй раз одно и то же не показываем. */
  const taken = new Set<string>();
  /** Оборванные загрузки, о которых уже сказано в шагах. */
  let seenFailures = $state(new Set<string>());
  let closeAsk = $state<string | null>(null);
  // Обновление пачкой: страницы по очереди, архивы копятся, установка — одна в конце.
  let batch = $state<DownloadRequest[]>([]);
  let collected = $state<Collected[]>([]);
  let batchDone = $state(false);
  const inBatch = $derived(batch.length > 1);
  const position = $derived(positionOf(batch, context));
  const batchPicked = $derived(pickedItems(collected));
  let now = $state(Date.now());

  /** Мод, к которому относится показанный архив: страница могла уже смениться. */
  const reviewed = $derived(reviewing?.request ?? context);
  const pageJob = $derived(jobForPage(jobs, context?.pageUrl));
  // Ход видно, пока файл открытой страницы качается; после него — проверка и установка.
  const loading = $derived(
    step === 2 && !error && pageJob && !reviewing ? viewOf(pageJob, tracked, now) : null,
  );
  const rows = $derived(
    jobRows(jobs, {
      currentPage: context?.pageUrl,
      reviewing: reviewing?.path ?? null,
      queued: queue.map((job) => job.path),
      seen: seenFailures,
    }),
  );
  const busy = $derived(!!reviewing || inspecting || installing);

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


  function resetArchive(discard = true): void {
    const work = archive?.workDir;
    if (discard && work) void incomingDiscard(work).catch(() => {});
    archive = null;
    selected = [];
    installError = null;
  }

  const checks = new StaleGuard();

  /** Докачался архив: в очередь на проверку. */
  function onReady(job: DownloadJob): void {
    if (!job.path || taken.has(job.path)) return;
    taken.add(job.path);
    queue = [...queue, job];
    pump();
  }

  /** Следующий архив из очереди — если сейчас ничего не проверяем. */
  function pump(): void {
    if (reviewing || inspecting || installing) return;
    const [next, ...rest] = queue;
    if (!next) return;
    queue = rest;
    void prepare(next);
  }

  /** Шаги открытой страницы: качается её файл или ещё нет. */
  function showPage(): void {
    error = null;
    const others = downloading(jobs).filter((job) => job.path !== pageJob?.path);
    const also = others.length > 0 ? ` Ещё качается: ${others.map((job) => `«${jobName(job)}»`).join(", ")}.` : "";
    if (pageJob) {
      step = 2;
      status = "Файл скачивается";
      detail = `Дождитесь окончания загрузки — менеджер заберёт файл сам.${also}`;
    } else {
      step = 1;
      status = "Нажмите Download на странице";
      detail = `Скачайте архив на странице — остальное программа сделает сама.${also}`;
    }
  }

  /**
   * С архивом закончили (установили, отменили, поставить нечего): следующий архив,
   * иначе — снова страница, пока что-то качается, иначе окно закрывается.
   */
  async function finishReview(discard = true): Promise<void> {
    resetArchive(discard);
    reviewing = null;
    if (queue.length > 0) {
      pump();
      return;
    }
    if (downloading(jobs).length > 0) {
      showPage();
      return;
    }
    await close();
  }

  async function prepare(job: DownloadJob): Promise<void> {
    const path = job.path;
    reviewing = job;
    const token = checks.begin();
    resetArchive();
    inspecting = true;
    installError = null;
    step = 3;
    status = "Проверяем архив";
    detail = "Смотрим структуру, опасные пути и подозрительные файлы.";
    try {
      const [found] = await incomingInspect([path]);
      if (!checks.isCurrent(token)) {
        if (found) void incomingDiscard(found.workDir).catch(() => {});
        return;
      }
      if (!found) throw new Error("Не удалось проверить архив.");
      const owner = job.request ?? context;
      if (inBatch && owner) {
        reviewing = null;
        keepForBatch(owner, found);
        // После finally: пока флаг проверки поднят, очередь не сдвинется.
        queueMicrotask(pump);
        return;
      }
      archive = found;
      selected = found.items.map(preselected);
      sameVersionDismissed = false;
      if (sameAsInstalled(found)) {
        status = "Это та же версия, что уже стоит";
        detail = "Архив совпадает с установленным модом. Похоже, сайт подписал прежнюю версию новым номером.";
      } else if (found.problem) {
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
      if (checks.isCurrent(token)) {
        installError = errorText(reason);
        status = "Не удалось проверить архив";
        detail = `${job.request?.modName ? `«${job.request.modName}»: ` : ""}${installError}`;
        // Очередь не должна встать: следующий архив проверяется дальше.
        reviewing = null;
        if (queue.length > 0) queueMicrotask(pump);
      }
    } finally {
      if (checks.isCurrent(token)) inspecting = false;
    }
  }

  /** Моды пачки, у которых скачался тот же файл, что уже стоит. */
  let sameInBatch = $state<DownloadRequest[]>([]);
  let sameVersionDismissed = $state(false);
  let dismissing = $state(false);

  /** «Больше не предлагать эту версию»: обновление этого мода прячется до следующей версии. */
  async function dismissSame(requests: DownloadRequest[]): Promise<void> {
    if (dismissing) return;
    dismissing = true;
    try {
      for (const request of requests) {
        if (request.modId) await updatesDismiss(request.modId);
      }
      sameVersionDismissed = true;
      sameInBatch = [];
      // Главное окно перечитает «Обновления» — пункт уходит сразу.
      await emit("download://dismissed", {}).catch(() => {});
      status = requests.length > 1 ? "Эти версии больше не предлагаются" : "Эта версия больше не предлагается";
      detail = "Если на сайте выйдет другая версия, обновление покажется снова.";
    } catch (reason) {
      error = errorText(reason);
    } finally {
      dismissing = false;
    }
  }

  /** Архив пачки проверен: откладываем к общей установке и открываем следующий мод. */
  function keepForBatch(request: DownloadRequest, found: IncomingArchiveDto): void {
    // Тот же файл, что стоит: ставить нечего — запоминаем, чтобы в конце предложить
    // больше не показывать эту версию, и идём дальше.
    if (sameAsInstalled(found)) {
      void incomingDiscard(found.workDir).catch(() => {});
      sameInBatch = [
        ...sameInBatch.filter((entry) => entry.pageUrl !== request.pageUrl),
        request,
      ];
      advance(`«${request.modName}»: это та же версия, что уже стоит`);
      return;
    }
    const old = collected.find((entry) => entry.request.pageUrl === request.pageUrl);
    if (old && old.archive.workDir !== found.workDir) {
      void incomingDiscard(old.archive.workDir).catch(() => {});
    }
    collected = [
      ...collected.filter((entry) => entry.request.pageUrl !== request.pageUrl),
      collect(request, found),
    ];
    advance(found.problem ? `«${request.modName}»: ${found.problem}` : null);
  }

  function advance(note: string | null = null): void {
    const next = nextPosition(batch, position, collected);
    error = null;
    if (next === null) {
      finishBatch();
      return;
    }
    step = 1;
    status = note ?? "Архив проверен — открываем следующий мод";
    detail = `Скачано ${collected.length} из ${batch.length}. Установка — одна, в конце.`;
    void downloadBatchGoto(next).catch((reason) => {
      error = errorText(reason);
    });
  }

  function skipMod(): void {
    if (installing || inspecting) return;
    advance(`«${context?.modName ?? "Мод"}» пропущен`);
  }

  function finishBatch(): void {
    batchDone = true;
    step = 4;
    status = collected.length > 0 ? "Всё скачано" : "Ничего не скачано";
    detail =
      collected.length > 0
        ? "Проверьте список и установите всё одной кнопкой. Прошлые версии останутся в копиях."
        : "Можно закрыть окно.";
  }

  function toggleCollected(entry: number, index: number, event: Event): void {
    const checked = (event.currentTarget as HTMLInputElement).checked;
    collected = collected.map((item, position) =>
      position === entry
        ? { ...item, selected: item.selected.map((value, i) => (i === index ? checked : value)) }
        : item,
    );
  }

  function discardCollected(): void {
    for (const entry of collected) {
      if (entry.archive.workDir) void incomingDiscard(entry.archive.workDir).catch(() => {});
    }
    collected = [];
  }

  async function installBatch(): Promise<void> {
    const items = batchPicked;
    if (items.length === 0 || installing) return;
    installing = true;
    installError = null;
    status = "Устанавливаем";
    detail = `${modsLabel(items.length)}: заменяем архивы, прошлые версии сохраняем.`;
    try {
      // Тот же порядок, что у batchPicked: результаты сверяются с ним по индексу.
      const results = await incomingInstall(installRequests(collected));
      const tickets: IncomingTicket[] = [];
      const failures: string[] = [];
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
          failures.push(
            `«${items[index]?.title ?? result.fileName}»: ${result.message ?? "установить не получилось"}`,
          );
        }
      });
      const installedFiles = new Set(tickets.map((ticket) => ticket.fileName.toLowerCase()));
      for (const entry of collected) {
        if (wholeArchiveInstalled(entry.archive.items, installedFiles) && !entry.archive.problem) {
          void incomingRemoveSource(entry.archive.source).catch(() => {});
        }
      }
      if (tickets.length > 0) {
        await emit("download://installed", {
          tickets,
          fileNames: tickets.map((ticket) => ticket.fileName),
          message:
            tickets.length === 1 ? "Обновлён 1 мод" : `Обновлено ${modsLabel(tickets.length)}`,
          warning,
        }).catch(() => {});
      }
      if (failures.length > 0) {
        installError = failures.join("; ");
        status =
          tickets.length > 0
            ? `Установлено ${tickets.length}, не удалось ${failures.length}`
            : "Установка не удалась";
        detail = installError;
        installing = false;
        return;
      }
      status = "Установлено";
      detail = warning ?? "Библиотека обновлена, прошлые версии остались в копиях.";
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
    const name = reviewed?.modName ?? "мод";
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
        installRequests([{ request: reviewed, archive, selected }]),
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
      // Окно закрывается, только если больше ничего не качается и не ждёт проверки.
      await finishReview(false);
    } catch (reason) {
      installError = errorText(reason);
      status = "Установка не удалась";
      detail = installError;
      installing = false;
    }
  }

  /** Закрыть окно. Если что-то ещё качается или ждёт установки — сначала спросить. */
  async function close(): Promise<void> {
    if (installing) return;
    const question = closeQuestion(jobs, queue);
    if (question) {
      closeAsk = question;
      return;
    }
    await closeNow();
  }

  /** «Прервать и закрыть»: загрузки обрываются, недокачанные файлы удаляются. */
  async function closeNow(): Promise<void> {
    if (installing) return;
    closeAsk = null;
    const work = archive?.workDir;
    archive = null;
    selected = [];
    reviewing = null;
    queue = [];
    if (work) void incomingDiscard(work).catch(() => {});
    discardCollected();
    batch = [];
    batchDone = false;
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
      if (downloading(jobs).length > 0) now = Date.now();
    }, 1000);
    stops.push(() => clearInterval(ticker));
    void listenDownloadProgress((value) => {
      if (!disposed) tracked = track(tracked, value, Date.now());
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void downloadBatch()
      .then((value) => {
        if (!disposed) batch = value;
      })
      .catch(() => {});
    void listenDownloadBatch((value) => {
      if (disposed) return;
      discardCollected();
      sameInBatch = [];
      batchDone = false;
      batch = value;
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void listenDownloadContext((value) => {
      // Открыли другой мод. Загрузки и проверка прошлого идут дальше: архив
      // проверяется для своего мода, а не для того, что сейчас на странице.
      if (disposed) return;
      context = value;
      if (!busy) showPage();
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void listenDownloadState((value) => {
      // Пока на экране архив, шаги — про него; загрузка страницы видна в списке.
      if (disposed || busy) return;
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
    void listenDownloadReady((job) => {
      if (!disposed) onReady(job);
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void listenDownloadJobs((value) => {
      if (disposed) return;
      // Файл под прежним именем качается заново: прошлые отметки к нему не относятся.
      const fresh = restarted(jobs, value);
      if (fresh.length > 0) {
        for (const path of fresh) taken.delete(path);
        const left = { ...tracked };
        for (const path of fresh) delete left[path];
        tracked = left;
        seenFailures = new Set([...seenFailures].filter((path) => !fresh.includes(path)));
      }
      jobs = value;
      // Обрыв загрузки открытой страницы показан в шагах — если на экране не архив.
      if (!busy) {
        const here = value.filter(
          (job) => job.state === "failed" && job.request?.pageUrl === context?.pageUrl,
        );
        if (here.some((job) => !seenFailures.has(job.path))) {
          seenFailures = new Set([...seenFailures, ...here.map((job) => job.path)]);
        }
      }
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void downloadJobs()
      .then((value) => {
        if (!disposed && jobs.length === 0) jobs = value;
      })
      .catch(() => {});
    void downloadReady()
      .then((ready) => {
        if (!disposed) for (const job of ready) onReady(job);
      })
      .catch(() => {});
    void listenDownloadCloseAsked(() => {
      if (!disposed) void close();
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
      })
      .catch(() => {});
    void listenDownloadBatchAdded((request) => {
      if (disposed) return;
      if (!batch.some((queued) => queued.pageUrl === request.pageUrl)) {
        batch = [...batch, request];
      }
      if (batchDone) {
        batchDone = false;
        advance(`«${request.modName}» добавлен в очередь`);
      }
    })
      .then((stop) => {
        if (disposed) stop();
        else stops.push(stop);
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
        <PuzzlePiece size={26} weight="thin" aria-hidden="true" />
      {:else}
        <Package size={26} weight="thin" aria-hidden="true" />
      {/if}
    </div>
    <div class="title">
      <h1>Скачивание</h1>
      <p>{context?.modName ?? "Обновление мода"}: {versionText}</p>
    </div>
    <button class="close" type="button" aria-label="Закрыть" onclick={() => void close()}>
      <span>Закрыть</span>
      <X size={16} weight="bold" aria-hidden="true" />
    </button>
  </header>

  <!-- Адрес страницы — отдельная область с именем: иначе он висел вне разметки. -->
  <div class="urlbar" role="region" aria-label="Адрес страницы">
    <Lock size={13} weight="bold" aria-hidden="true" />
    <span>{address}</span>
  </div>

  <div class="site-hole" aria-hidden="true">
    {#if !isTauri()}
      <div class="fake-site">
        <Package size={44} weight="thin" aria-hidden="true" />
        <b>Страница мода</b>
        <small>Здесь откроется настоящая страница. Кнопку Download нажимаете вы.</small>
      </div>
    {/if}
  </div>

  <aside class="steps" aria-label="Этапы загрузки">
    {#if closeAsk}
      <!-- Вопрос держит фокус: Tab не уходит под него, сразу стоим на «Не закрывать»,
           Escape отвечает «не закрывать». Раньше с клавиатуры до него было не добраться. -->
      <div
        class="close-ask"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="close-ask-title"
        aria-describedby="close-ask-text"
        tabindex="-1"
        use:trapFocus={".keep"}
        onkeydown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            closeAsk = null;
          }
        }}
      >
        <b id="close-ask-title">Закрыть окно?</b>
        <span id="close-ask-text">{closeAsk}</span>
        <div class="batch-actions">
          <button class="danger" type="button" onclick={() => void closeNow()}>Прервать и закрыть</button>
          <button class="ghost keep" type="button" onclick={() => (closeAsk = null)}>Не закрывать</button>
        </div>
      </div>
    {/if}
    {#if inBatch}
      <section class="batch" aria-label="Обновление пачкой">
        <b>{batchDone ? `Скачано ${collected.length} из ${batch.length}` : batchLine(position, batch.length, collected.length)}</b>
        {#if sameInBatch.length > 0}
          <span>
            Та же версия, что уже стоит:
            {sameInBatch.map((request) => `«${request.modName}»`).join(", ")}.
          </span>
          {#if sameInBatch.some((request) => request.modId)}
            <div class="batch-actions">
              <button class="ghost" type="button" disabled={dismissing} onclick={() => void dismissSame(sameInBatch)}>
                Больше не предлагать {sameInBatch.length > 1 ? "эти версии" : "эту версию"}
              </button>
            </div>
          {/if}
        {/if}
        {#if !batchDone}
          <span>Скачайте архив — окно само откроет следующий мод. Установка — одна, в конце.</span>
          <div class="batch-actions">
            <button class="ghost" type="button" disabled={inspecting || installing} onclick={skipMod}>
              Пропустить мод
            </button>
            {#if collected.length > 0}
              <button class="ghost" type="button" disabled={inspecting || installing} onclick={finishBatch}>
                Установить скачанные ({collected.length})
              </button>
            {/if}
          </div>
        {/if}
      </section>
    {/if}
    <div class="steps-title">ЭТАПЫ</div>
    {#each steps as item (item.id)}
      {@const state = step > item.id ? "done" : step === item.id ? "active" : "waiting"}
      <div class="step {state}">
        <span class="marker">
          {#if state === "done"}
            <DrawCheck size={13} ring={false} />
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

    {#if rows.length > 0}
      <section class="jobs" aria-label="Загрузки">
        <div class="steps-title">ЗАГРУЗКИ</div>
        {#each rows as row (row.job.path)}
          {@const view = row.job.state === "downloading" ? viewOf(row.job, tracked, now) : null}
          <div class="job" class:failed={row.job.state === "failed"}>
            <b>{jobName(row.job)}</b>
            {#if view}
              <div
                class="bar"
                class:indeterminate={view.percent === null}
                role="progressbar"
                aria-label={`Ход загрузки «${jobName(row.job)}»`}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-valuenow={view.percent ?? undefined}
              >
                <span style={`width:${view.percent ?? 100}%`}></span>
              </div>
              <small class="mono">{view.line}</small>
            {:else}
              <small>{row.text}</small>
            {/if}
          </div>
        {/each}
      </section>
    {/if}

    {#if inspecting}
      <div class="ready-state">Проверяем архив…</div>
    {:else if inBatch && batchDone}
      <section class="ready" aria-label="Установка пачкой">
        <div class="ready-items">
          {#each collected as entry, entryIndex (entry.request.pageUrl)}
            <article class="ready-item" class:blocked={!!entry.archive.problem}>
              <div class="ready-text">
                <b>{entry.request.modName}</b>
                <small class="file">{archiveBaseName(entry.archive.source)}</small>
              </div>
              {#if entry.archive.problem}
                <p class="problem">{entry.archive.problem}</p>
              {:else}
                {#each entry.archive.items as item, index (item.zipPath + index)}
                  <label class="ready-check-row" class:disabled={!installable(item)}>
                    <input
                      type="checkbox"
                      checked={entry.selected[index] ?? false}
                      disabled={!installable(item) || installing}
                      onchange={(event) => toggleCollected(entryIndex, index, event)}
                    />
                    <span>
                      {item.title}
                      <small>{kindText(item)} · {formatSize(item.size)}</small>
                      {#if item.blocked}<small class="bad">Есть опасные файлы или пути</small>
                      {:else if suspiciousFiles(item).length > 0}
                        <small class="bad">Сомнительные файлы: {suspiciousText(suspiciousFiles(item))}</small>
                      {/if}
                    </span>
                  </label>
                {/each}
              {/if}
            </article>
          {/each}
        </div>
        {#if installError}<p class="problem">{installError}</p>{/if}
        <div class="ready-actions">
          <button
            class="primary"
            type="button"
            disabled={installing || batchPicked.length === 0}
            onclick={() => void installBatch()}
          >
            {installing ? "Устанавливаем…" : `Установить ${modsLabel(batchPicked.length)}`}
          </button>
          <button class="ghost" type="button" disabled={installing} onclick={() => void close()}>
            Отмена
          </button>
        </div>
      </section>
    {:else if archive}
      <section class="ready" aria-label="Установка">
        <div class="ready-head">
          <b>{archiveTitle}</b>
          <span>{items.length} {plural(items.length, "мод", "мода", "модов")} · {formatSize(archiveSize)}</span>
        </div>
        {#if archive.problem}
          <p class="problem">{archive.problem}</p>
          <button class="primary" type="button" onclick={() => void finishReview()}>Понятно</button>
        {:else if sameAsInstalled(archive)}
          <p class="problem">Это та же версия, что уже стоит, — ставить нечего.</p>
          <div class="ready-actions">
            {#if reviewed?.modId && !sameVersionDismissed}
              <button
                class="primary"
                type="button"
                disabled={dismissing}
                onclick={() => reviewed && void dismissSame([reviewed])}
              >
                Больше не предлагать эту версию
              </button>
            {/if}
            <button class="ghost" type="button" onclick={() => void finishReview()}>Закрыть</button>
          </div>
        {:else if !items.some(installable)}
          <p class="problem">В архиве нет модов, которые можно установить.</p>
          <button class="primary" type="button" onclick={() => void finishReview()}>Понятно</button>
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
            <button class="ghost" type="button" disabled={installing} onclick={() => void finishReview()}>
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
    transition:
      background-color var(--dur-2) var(--ease-out),
      border-color var(--dur-2) var(--ease-out),
      box-shadow var(--dur-2) var(--ease-out);
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
    animation: marker-done var(--dur-3) var(--ease-snap) both;
  }

  /* Этап закрылся: кружок заливается и чуть «щёлкает», галочка дорисовывается в нём. */
  @keyframes marker-done {
    from {
      scale: 0.86;
    }
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

  .batch {
    display: grid;
    gap: 4px;
    margin-bottom: 12px;
    padding: 10px 12px;
    border: 1px solid var(--accent-brand);
    border-radius: 9px;
    background: var(--accent-brand-soft);
    font-size: 12px;
    line-height: 1.4;
  }

  .batch b {
    font-size: 13px;
  }

  .batch span {
    color: var(--muted-foreground);
  }

  .batch-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }

  /* Закрыть окно, пока что-то качается: сначала спросить. */
  .close-ask {
    display: grid;
    gap: 4px;
    margin-bottom: 12px;
    padding: 10px 12px;
    border: 1px solid var(--danger);
    border-radius: 9px;
    background: var(--danger-soft);
    font-size: 12px;
    line-height: 1.4;
  }

  .close-ask b {
    font-size: 13px;
  }

  .close-ask button {
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--foreground);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .close-ask .danger {
    border-color: var(--danger);
    background: var(--danger);
    color: #fff;
  }

  /* Загрузки других модов: каждая со своим ходом. */
  .jobs {
    display: grid;
    gap: 8px;
    margin-top: 14px;
  }

  .job {
    display: grid;
    gap: 2px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--surface-2);
    font-size: 12px;
  }

  .job b {
    overflow: hidden;
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .job small {
    color: var(--muted-foreground);
  }

  .job.failed small {
    color: var(--danger);
  }

  .ready-check-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 6px;
    font-size: 12.5px;
  }

  .ready-check-row span {
    display: grid;
    gap: 1px;
  }

  .ready-check-row small {
    color: var(--muted-foreground);
  }

  .ready-check-row small.bad {
    color: var(--danger);
  }

  .ready-check-row.disabled {
    opacity: 0.6;
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
    animation: var(--run);
  }

  /* «Быстро» обрывает анимацию на последнем кадре — полоса уезжала за край, и шла
     ли работа, было не видно. Без движения полоса стоит во всю ширину, бледнее. */
  :global(.no-motion) .bar.indeterminate span {
    width: 100% !important;
    animation: none;
    transform: none;
    opacity: 0.45;
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

<script lang="ts">
  import { SaveQueue } from "#lib/features/settings/saveQueue.js";
  import { motion } from "#lib/shell/motion.svelte.js";
  import { onDestroy, onMount } from "svelte";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { open } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import Check from "phosphor-svelte/lib/Check";
  import Cpu from "phosphor-svelte/lib/Cpu";
  import Download from "phosphor-svelte/lib/DownloadSimple";
  import FolderOpen from "phosphor-svelte/lib/FolderOpen";
  import Globe from "phosphor-svelte/lib/Globe";
  import Monitor from "phosphor-svelte/lib/Monitor";
  import Moon from "phosphor-svelte/lib/Moon";
  import Package from "phosphor-svelte/lib/Package";
  import Palette from "phosphor-svelte/lib/Palette";
  import Plus from "phosphor-svelte/lib/Plus";
  import RefreshCw from "phosphor-svelte/lib/ArrowsClockwise";
  import RotateCcw from "phosphor-svelte/lib/ArrowCounterClockwise";
  import Sun from "phosphor-svelte/lib/Sun";
  import X from "phosphor-svelte/lib/X";
  import { isTauri } from "@tauri-apps/api/core";
  import { Switch } from "#lib/components/ui/switch/index.js";
  import { appearance } from "#lib/shell/appearance.svelte.js";
  import { openExternal } from "#lib/shell/openExternal.js";
  import { toasts } from "#lib/shell/toasts.svelte.js";
  import { appDataFolder, type SettingsDto } from "#lib/ipc/settings.js";
  import {
    appUpdateCancel,
    appUpdateCheck,
    appUpdateDownload,
    appUpdateRestart,
    appUpdateState,
    listenAppUpdateProgress,
    listenAppUpdateState,
    type AppUpdateSnapshot,
  } from "#lib/ipc/appUpdate.js";

  type Theme = SettingsDto["theme"];
  type Mode = SettingsDto["mode"];
  type Backdrop = SettingsDto["background"];

  // Акцент. Имена тем прежние, чтобы сохранённые настройки не сбросились.
  const THEMES: { id: Theme; label: string; desc: string; color: string }[] = [
    { id: "night", label: "Оранжевый", desc: "как в BeamNG", color: "#ff7a1a" },
    { id: "deep", label: "Синий", desc: "спокойный", color: "#3d8bff" },
    { id: "dusk", label: "Красный", desc: "гоночный", color: "#ff4f4f" },
    { id: "forest", label: "Зелёный", desc: "мягкий", color: "#33c47c" },
    { id: "graphite", label: "Графит", desc: "без цвета", color: "#dcdfe3" },
  ];
  const BACKDROPS: {
    id: Backdrop;
    label: string;
    desc: string;
    gradient: string;
  }[] = [
    {
      id: "accent",
      label: "Акцент",
      desc: "как интерфейс",
      gradient:
        "linear-gradient(135deg, var(--accent-brand), color-mix(in oklab, var(--accent-brand) 30%, #ffffff))",
    },
    {
      id: "aurora",
      label: "Сияние",
      desc: "бирюза и фиолет",
      gradient: "linear-gradient(135deg, #2dd4bf, #8b5cf6 55%, #38bdf8)",
    },
    {
      id: "sunset",
      label: "Закат",
      desc: "тёплый и мягкий",
      gradient: "linear-gradient(135deg, #fb7185, #f59e0b 55%, #a855f7)",
    },
    {
      id: "ocean",
      label: "Океан",
      desc: "глубина и прохлада",
      gradient: "linear-gradient(135deg, #22d3ee, #3b82f6 55%, #14b8a6)",
    },
    {
      id: "nebula",
      label: "Туманность",
      desc: "фиолет и маджента",
      gradient: "linear-gradient(135deg, #a855f7, #ec4899 55%, #6366f1)",
    },
    {
      id: "mono",
      label: "Графит",
      desc: "без цвета",
      gradient: "linear-gradient(135deg, #94a3b8, #cbd5e1 55%, #64748b)",
    },
  ];
  const MODES: { id: Mode; label: string; icon: typeof Moon }[] = [
    { id: "dark", label: "Тёмная", icon: Moon },
    { id: "light", label: "Светлая", icon: Sun },
    { id: "system", label: "Как в Windows", icon: Monitor },
  ];
  const SECTIONS = [
    { id: "appearance", label: "Внешний вид", icon: Palette },
    { id: "folders", label: "Папки", icon: FolderOpen },
    { id: "mods", label: "Моды и загрузки", icon: Package },
    { id: "updates", label: "Обновления", icon: RefreshCw },
    { id: "sites", label: "Сайты", icon: Globe },
    { id: "program", label: "О программе", icon: Cpu },
  ] as const;
  const REPO_URL = "https://github.com/SnikS1337/beamng-mod-manager";
  const RELEASES_URL = `${REPO_URL}/releases/latest`;
  type SectionId = (typeof SECTIONS)[number]["id"];
  const SOURCES = [
    { id: "modland", label: "Modland", note: "Иногда просит подтвердить, что вы человек" },
    { id: "modhub", label: "ModHub", note: "Иногда просит подтвердить, что вы человек" },
    { id: "stmods", label: "STMods", note: "Каталог модов BeamNG" },
    { id: "modsgaming", label: "ModsGaming", note: "Русский каталог модов" },
    { id: "beamng", label: "BeamNG (официальный)", note: "Репозиторий и обновления; в поиске не участвует" },
  ];
  // `/settings?section=folders` — ссылки вроде «Не нашли папку с модами» ведут сразу к нужному.
  const requested = new URLSearchParams(globalThis.location?.search ?? "").get("section");
  let activeSection = $state<SectionId>(
    SECTIONS.find((section) => section.id === requested)?.id ?? "appearance",
  );
  let sectionsEl = $state<HTMLDivElement | null>(null);
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let revision = 0;

  function selectSection(id: SectionId) {
    if (id === activeSection) return;
    void motion.run(() => {
      activeSection = id;
      if (sectionsEl) sectionsEl.scrollTop = 0;
    }, "flow-section");
  }
  let draft = $state<SettingsDto | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let dirty = $state(false);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);
  let appUpdate = $state<AppUpdateSnapshot | null>(null);
  let updateBusy = $state(false);
  let updateError = $state<string | null>(null);
  let updateProgress = $state(0);
  let unlistenUpdate: (() => void) | undefined;
  let unlistenUpdateState: (() => void) | undefined;
  let unlistenClose: (() => void) | undefined;
  let disposed = false;

  onMount(() => {
    void boot();
    void loadAppUpdate();
    if (isTauri()) {
      // Окно закрывается без onDestroy: несохранённую правку дописываем до выхода.
      void getCurrentWindow()
        .onCloseRequested(async () => {
          if (saveTimer) clearTimeout(saveTimer);
          // Сначала дожидаемся идущего сохранения: правка, сделанная во время него,
          // иначе терялась — окно закрывалось раньше, чем её успевали записать.
          await saves.idle().catch(() => {});
          if (dirty) await save(true).catch(() => {});
        })
        .then((stop) => {
          if (disposed) stop();
          else unlistenClose = stop;
        })
        .catch(() => {});
    }
    void listenAppUpdateProgress((value) => {
      updateProgress = value;
    }).then((stop) => {
      if (disposed) stop();
      else unlistenUpdate = stop;
    });
    void listenAppUpdateState((snapshot) => {
      appUpdate = snapshot;
      updateProgress = snapshot.state.progress;
    }).then((stop) => {
      if (disposed) stop();
      else unlistenUpdateState = stop;
    });
    return () => {
      disposed = true;
      unlistenUpdate?.();
      unlistenUpdateState?.();
      unlistenClose?.();
    };
  });

  onDestroy(() => {
    disposed = true;
    unlistenUpdate?.();
    unlistenUpdateState?.();
    unlistenClose?.();
    if (saveTimer) clearTimeout(saveTimer);
    if (dirty) void save(true);
  });

  async function loadAppUpdate() {
    try {
      appUpdate = await appUpdateState();
      updateProgress = appUpdate.state.progress;
    } catch (reason) {
      updateError = errorText(reason);
    }
  }

  async function checkAppUpdate() {
    if (updateBusy) return;
    updateBusy = true;
    updateError = null;
    try {
      appUpdate = await appUpdateCheck();
      updateProgress = appUpdate.state.progress;
    } catch (reason) {
      updateError = errorText(reason);
    } finally {
      updateBusy = false;
    }
  }

  async function downloadAppUpdate() {
    if (updateBusy) return;
    updateBusy = true;
    updateError = null;
    if (appUpdate) {
      appUpdate = {
        ...appUpdate,
        state: { ...appUpdate.state, stage: "downloading", progress: 0, error: null },
      };
      updateProgress = 0;
    }
    try {
      appUpdate = await appUpdateDownload();
      updateProgress = appUpdate.state.progress;
    } catch (reason) {
      updateError = errorText(reason);
    } finally {
      updateBusy = false;
    }
  }

  async function cancelAppUpdate() {
    try {
      appUpdate = await appUpdateCancel();
      updateProgress = 0;
    } catch (reason) {
      updateError = errorText(reason);
    }
  }

  async function restartAppUpdate() {
    try {
      await appUpdateRestart();
    } catch (reason) {
      updateError = errorText(reason);
    }
  }

  const releaseNotes = $derived.by(() => {
    const text = appUpdate?.state.manifest?.notes?.trim();
    if (!text) return [] as string[];
    const parts = text
      .split(/\s+-\s+|\r?\n/)
      .map((part) => part.trim().replace(/^[-•]\s*/, ""))
      .filter(Boolean);
    return parts.length > 1 ? parts : [text];
  });

  function updateStageText(): string {
    const stage = appUpdate?.state.stage;
    switch (stage) {
      case "available":
        return `Доступна версия ${appUpdate?.state.manifest?.version ?? ""}`;
      case "upToDate":
        return "Установлена последняя версия";
      case "downloading":
        return `Скачиваем… ${Math.round(updateProgress * 100)}%`;
      case "readyToRestart":
        return `Версия ${appUpdate?.state.manifest?.version ?? ""} скачана — перезапустите программу`;
      case "failed":
        return appUpdate?.state.error ?? "Не удалось проверить обновление";
      case "checking":
        return "Проверяем обновление…";
      default:
        return "Новые версии программы берём с GitHub";
    }
  }

  async function revealDataFolder(): Promise<void> {
    try {
      const folder = await appDataFolder();
      await revealItemInDir(folder);
    } catch {
      toasts.show("Не удалось открыть папку данных");
    }
  }

  async function boot() {
    loading = true;
    error = null;
    try {
      const loaded = appearance.settings ?? (await appearance.load());
      if (loaded) draft = clone(loaded);
      // Без настроек раздел раньше навсегда показывал «Загружаем…».
      else error = "Не удалось прочитать настройки.";
    } catch (reason) {
      error = errorText(reason);
    } finally {
      loading = false;
    }
  }

  function clone(value: SettingsDto): SettingsDto {
    return {
      ...value,
      disabledSources: [...value.disabledSources],
      watchFolders: [...value.watchFolders],
    };
  }

  const NUMBER_DEFAULTS = {
    checkTtlHours: 6,
    autoCheckEveryHours: 0,
    keepBackups: 3,
    maxCardChecksPerSource: 150,
    activeModsWarnAbove: 30,
  };
  const numbersValid = $derived(
    draft
      ? Object.keys(NUMBER_DEFAULTS).every((key) => {
          const value = draft?.[key as keyof typeof NUMBER_DEFAULTS];
          return typeof value === "number" && Number.isFinite(value);
        })
      : true,
  );

  const downloadsInsideMods = $derived.by(() => {
    const downloads = draft?.downloadsFolder?.trim();
    const mods = draft?.modsFolder?.trim();
    if (!downloads || !mods) return false;
    const normalize = (path: string) =>
      path.replace(/[\\/]+$/, "").toLowerCase();
    const inside = normalize(downloads);
    const parent = normalize(mods);
    return inside === parent || inside.startsWith(`${parent}\\`) || inside.startsWith(`${parent}/`);
  });

  function numberOr(value: unknown, fallback: number): number {
    return typeof value === "number" && Number.isFinite(value)
      ? value
      : fallback;
  }

  function withNumberFallbacks(value: SettingsDto): SettingsDto {
    const saved = appearance.settings;
    return {
      ...value,
      checkTtlHours: numberOr(
        value.checkTtlHours,
        saved?.checkTtlHours ?? NUMBER_DEFAULTS.checkTtlHours,
      ),
      autoCheckEveryHours: numberOr(
        value.autoCheckEveryHours,
        saved?.autoCheckEveryHours ?? NUMBER_DEFAULTS.autoCheckEveryHours,
      ),
      keepBackups: numberOr(
        value.keepBackups,
        saved?.keepBackups ?? NUMBER_DEFAULTS.keepBackups,
      ),
      maxCardChecksPerSource: numberOr(
        value.maxCardChecksPerSource,
        saved?.maxCardChecksPerSource ?? NUMBER_DEFAULTS.maxCardChecksPerSource,
      ),
      activeModsWarnAbove: numberOr(
        value.activeModsWarnAbove,
        saved?.activeModsWarnAbove ?? NUMBER_DEFAULTS.activeModsWarnAbove,
      ),
    };
  }

  function mark(event?: Event) {
    const control = event?.currentTarget as HTMLInputElement | HTMLSelectElement | null;
    // Пустое числовое поле — незаконченный ввод, а не «ноль»: ждём цифру.
    if (
      control?.tagName === "INPUT" &&
      (control as HTMLInputElement).type === "number" &&
      control.value.trim() === ""
    ) {
      revision += 1;
      return;
    }
    dirty = true;
    message = null;
    revision += 1;
    if (draft) appearance.preview(draft);
    // Тумблер, тема или режим — дискретное действие: сохраняем сразу, чтобы
    // закрытое окно не потеряло его. Набор текста ждёт паузы.
    const discrete =
      !control ||
      control.tagName === "SELECT" ||
      ["checkbox", "radio"].includes((control as HTMLInputElement).type);
    if (discrete) {
      if (saveTimer) clearTimeout(saveTimer);
      void save(!numbersValid);
    } else {
      scheduleSave();
    }
  }

  function scheduleSave() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void save(), 350);
  }

  // Сохранения идут по одному; закрытие окна ждёт последнее (см. onCloseRequested).
  let queuedFallbacks = false;
  const saves = new SaveQueue(async () => {
    const fallbacks = queuedFallbacks;
    queuedFallbacks = false;
    await saveOnce(fallbacks);
  });

  function save(useNumberFallbacks = false): Promise<void> {
    queuedFallbacks ||= useNumberFallbacks;
    return saves.request();
  }

  async function saveOnce(useNumberFallbacks: boolean) {
    if (!draft || !dirty) return;
    if (!numbersValid && !useNumberFallbacks) return;
    const rev = revision;
    const payload = useNumberFallbacks ? withNumberFallbacks(draft) : draft;
    saving = true;
    error = null;
    try {
      const saved = clone(await appearance.save(payload));
      if (revision === rev) {
        draft = saved;
        dirty = false;
        message = "Сохранено";
      }
    } catch (reason) {
      error = errorText(reason);
    } finally {
      saving = false;
      if (dirty && revision !== rev) scheduleSave();
    }
  }

  const preset = $derived(
    draft?.uiAnimations && draft.animatedBackground
      ? "beauty"
      : draft && !draft.uiAnimations && !draft.animatedBackground
        ? "fast"
        : null,
  );

  // Набор, к которому вернуться, если движение поменяли вручную.
  let lastPreset = $state<"beauty" | "fast">("beauty");
  $effect(() => {
    if (preset) lastPreset = preset;
  });

  function applyPreset(kind: "beauty" | "fast") {
    if (!draft) return;
    lastPreset = kind;
    const on = kind === "beauty";
    draft.uiAnimations = on;
    draft.animatedBackground = on;
    mark();
  }

  function selectBackdrop(id: Backdrop) {
    if (!draft || draft.background === id) return;
    draft.background = id;
    mark();
  }

  function selectMode(id: Mode, event: MouseEvent) {
    if (!draft || draft.mode === id) return;
    const root = document.documentElement;
    const x = event.clientX > 0 ? event.clientX : window.innerWidth / 2;
    const y = event.clientY > 0 ? event.clientY : window.innerHeight / 2;
    root.style.setProperty("--theme-x", `${x}px`);
    root.style.setProperty("--theme-y", `${y}px`);
    void motion.run(() => {
      if (!draft) return;
      draft.mode = id;
      mark();
    }, "flow-theme");
  }

  function selectTheme(id: Theme, event: MouseEvent) {
    if (!draft || draft.theme === id) return;
    const root = document.documentElement;
    const x = event.clientX > 0 ? event.clientX : window.innerWidth / 2;
    const y = event.clientY > 0 ? event.clientY : window.innerHeight / 2;
    root.style.setProperty("--theme-x", `${x}px`);
    root.style.setProperty("--theme-y", `${y}px`);
    void motion.run(() => {
      if (!draft) return;
      draft.theme = id;
      mark();
    }, "flow-theme");
  }

  function setFlag(
    key: "uiAnimations" | "animatedBackground",
    value: boolean,
  ) {
    if (!draft) return;
    draft[key] = value;
    mark();
  }

  function sourceEnabled(id: string): boolean {
    return !draft?.disabledSources.includes(id);
  }

  function setSource(id: string, enabled: boolean) {
    if (!draft) return;
    const rest = draft.disabledSources.filter((source) => source !== id);
    draft.disabledSources = enabled ? rest : [...rest, id];
    mark();
  }

  async function pickFolder(field: "modsFolder" | "userFolder" | "downloadsFolder") {
    if (!draft) return;
    if (!isTauri()) {
      message = "Выбор папки доступен в приложении.";
      return;
    }
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title:
          field === "modsFolder"
            ? "Папка модов (…\\mods)"
            : field === "userFolder"
              ? "Папка пользователя игры"
              : "Куда скачивать обновления",
        // Открываем там, где папка уже выбрана, а не в последней случайной.
        defaultPath: draft[field] || undefined,
      });
      if (typeof picked === "string") {
        draft[field] = picked;
        mark();
      }
    } catch (reason) {
      toasts.show(errorText(reason));
    }
  }

  function resetDownloadsFolder(): void {
    if (!draft) return;
    draft.downloadsFolder = null;
    mark();
  }

  async function addWatchFolder() {
    if (!draft) return;
    if (!isTauri()) {
      message = "Выбор папки доступен в приложении.";
      return;
    }
    try {
      const picked = await open({ directory: true, multiple: false, title: "Папка с загрузками" });
      if (typeof picked === "string" && !draft.watchFolders.includes(picked)) {
        draft.watchFolders = [...draft.watchFolders, picked];
        mark();
      }
    } catch (reason) {
      toasts.show(errorText(reason));
    }
  }

  function removeWatchFolder(folder: string) {
    if (!draft) return;
    draft.watchFolders = draft.watchFolders.filter((item) => item !== folder);
    mark();
  }

  async function openFolder(path: string | null) {
    if (!path) return;
    try {
      await revealItemInDir(path);
    } catch (reason) {
      toasts.show(errorText(reason));
    }
  }

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось выполнить действие.";
  }
</script>

<div class="settings-page">
  <header class="head">
    <div>
      <h1 class="display">Настройки</h1>
    </div>
    <div class="head-actions">
      <span class="save-state">
        <span
          class="save-dot"
          class:busy={(saving || dirty) && !error && numbersValid}
          class:invalid={!numbersValid && !error}
          class:failed={!!error}
        ></span>
        {error
          ? "Не сохранено"
          : !numbersValid
            ? "Проверьте числа"
            : saving || dirty
              ? "Сохраняем…"
              : message ?? "Сохраняется автоматически"}
      </span>
    </div>
  </header>

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  {#if loading}
    <div class="state"><p>Загружаем настройки…</p></div>
  {:else if !draft}
    <div class="state">
      <p>Настройки не загрузились.</p>
      <button class="btn-glass" type="button" onclick={() => void boot()}>Повторить</button>
    </div>
  {:else}
    <div class="settings-layout">
      <nav class="settings-side" aria-label="Разделы настроек">
        {#each SECTIONS as section (section.id)}
          {@const Icon = section.icon}
          <button
            type="button"
            class:active={activeSection === section.id}
            onclick={() => selectSection(section.id)}
          >
            <Icon size={16} />
            <span>{section.label}</span>
          </button>
        {/each}
      </nav>
      <div class="settings-sections flow-section-body" bind:this={sectionsEl}>
        {#if activeSection === "appearance"}
        <section id="settings-appearance" class="block">
          <div class="block-head">
            <h2 class="display">Внешний вид</h2>
            <span>тема, акцент и движение</span>
          </div>

      <div class="appearance-grid">
        <div class="appearance-left">
          <span class="group-title">Готовые наборы</span>
          <div class="presets">
            <button
              type="button"
              class:active={preset === "beauty"}
              onclick={() => applyPreset("beauty")}
            >
              <b>Живо</b>
              <small>плавные переходы и мягкий свет фона</small>
            </button>
            <button
              type="button"
              class:active={preset === "fast"}
              onclick={() => applyPreset("fast")}
            >
              <b>Быстро</b>
              <small>без движения — для слабых компьютеров</small>
            </button>
          </div>

          {#if preset === null}
            <p class="preset-note">
              Движение настроено вручную.
              <button type="button" onclick={() => applyPreset(lastPreset)}>
                Вернуть «{lastPreset === "beauty" ? "Живо" : "Быстро"}»
              </button>
            </p>
          {/if}

          <span class="group-title">Движение</span>
          <div class="switch-list">
            <div class="switch-row">
              <div>
                <b>Анимации интерфейса</b>
                <small>Плавные переходы между разделами; карточки переезжают на новые места, панели выезжают.</small>
              </div>
              <Switch
                size="lg"
                checked={draft.uiAnimations}
                onCheckedChange={(value) => setFlag("uiAnimations", value)}
                aria-label="Анимации интерфейса"
              />
            </div>
            <div class="switch-row">
              <div>
                <b>Живой фон</b>
                <small>Мягкий свет акцентного цвета медленно движется по фону. Работает при включённых анимациях интерфейса.</small>
              </div>
              <Switch
                size="lg"
                checked={draft.animatedBackground}
                disabled={!draft.uiAnimations}
                onCheckedChange={(value) => setFlag("animatedBackground", value)}
                aria-label="Живой фон"
              />
            </div>
          </div>
        </div>

        <div class="appearance-right">
          <span class="group-title">Тема</span>
          <div class="choices">
            {#each MODES as mode (mode.id)}
              {@const ModeIcon = mode.icon}
              <button
                type="button"
                class:active={draft.mode === mode.id}
                onclick={(event) => selectMode(mode.id, event)}
              >
                <ModeIcon size={14} />
                {mode.label}
              </button>
            {/each}
          </div>

          <span class="group-title">Акцент</span>
          <div class="theme-grid">
            {#each THEMES as theme (theme.id)}
              <button
                type="button"
                class="swatch"
                class:active={draft.theme === theme.id}
                aria-pressed={draft.theme === theme.id}
                onclick={(event) => selectTheme(theme.id, event)}
              >
                <span class="tone" style={`background:${theme.color}`}>
                  {#if draft.theme === theme.id}
                    <Check size={13} weight="bold" />
                  {/if}
                </span>
                <span class="swatch-text">
                  <b>{theme.label}</b>
                  <small>{theme.desc}</small>
                </span>
              </button>
            {/each}
          </div>

          <span class="group-title">Фон</span>
          <div class="backdrop-grid">
            {#each BACKDROPS as item (item.id)}
              <button
                type="button"
                class="backdrop-swatch"
                class:active={draft.background === item.id}
                aria-pressed={draft.background === item.id}
                onclick={() => selectBackdrop(item.id)}
              >
                <span class="backdrop-tone" style={`background:${item.gradient}`}>
                  {#if draft.background === item.id}
                    <Check size={14} weight="bold" />
                  {/if}
                </span>
                <span class="backdrop-text">
                  <b>{item.label}</b>
                  <small>{item.desc}</small>
                </span>
              </button>
            {/each}
          </div>
          <p class="group-hint">
            Цвет интерфейса и фон можно выбрать отдельно. «Акцент» повторяет цвет кнопок.
          </p>
        </div>
      </div>
    </section>
        {/if}
        {#if activeSection === "folders"}
        <section id="settings-folders" class="block">
          <div class="block-head">
            <h2 class="display">Папки</h2>
        <span>моды и данные игры</span>
      </div>

      <div class="row">
        <div class="row-text">
          <label for="mods-folder">Папка модов</label>
          <small>Папка mods внутри папки пользователя игры.</small>
        </div>
        <div class="path-row">
          <input id="mods-folder" readonly value={draft.modsFolder ?? ""} placeholder="не выбрана" title={draft.modsFolder ?? ""} />
          <button type="button" onclick={() => void pickFolder("modsFolder")}>
            <FolderOpen size={13} />
            Выбрать
          </button>
          <button
            type="button"
            disabled={!draft.modsFolder}
            onclick={() => void openFolder(draft?.modsFolder ?? null)}
          >
            Показать
          </button>
        </div>
      </div>

      <div class="row">
        <div class="row-text">
          <label for="user-folder">Папка пользователя игры</label>
          <small>Где игра хранит настройки и моды. Обычно находится сама.</small>
        </div>
        <div class="path-row">
          <input id="user-folder" readonly value={draft.userFolder ?? ""} placeholder="определяется автоматически" title={draft.userFolder ?? ""} />
          <button type="button" onclick={() => void pickFolder("userFolder")}>
            <FolderOpen size={13} />
            Выбрать
          </button>
          <button
            type="button"
            disabled={!draft.userFolder}
            onclick={() => void openFolder(draft?.userFolder ?? null)}
          >
            Показать
          </button>
        </div>
      </div>
    </section>
        {/if}
        {#if activeSection === "mods"}
        <section id="settings-mods" class="block">
          <div class="block-head">
            <h2 class="display">Моды и загрузки</h2>
        <span>прежние версии, предупреждения, скачанные архивы</span>
      </div>

      <div class="row">
        <div class="row-text">
          <label for="keep-backups">Сколько прежних версий хранить</label>
          <small>При обновлении или исправлении старый файл мода уходит в копии.</small>
        </div>
        <input
          id="keep-backups"
          class="number"
          type="number"
          min="1"
          max="20"
          bind:value={draft.keepBackups}
          oninput={mark}
        />
      </div>

      <div class="row">
        <div class="row-text">
          <label for="warn-above">Предупреждать, если включено больше</label>
          <small>Много модов — игра дольше грузится. 0 — не предупреждать.</small>
        </div>
        <div class="unit">
          <input
            id="warn-above"
            class="number"
            type="number"
            min="0"
            max="1000"
            bind:value={draft.activeModsWarnAbove}
            oninput={mark}
          />
          <span>файлов</span>
        </div>
      </div>

      <div class="switch-row">
        <div>
          <b>Скачанные моды</b>
          <small>Новый архив в «Загрузках» или в папках ниже — программа предложит его установить.</small>
        </div>
        <Switch
          size="lg"
          checked={draft.allowDownloads}
          onCheckedChange={(value) => {
            if (draft) {
              draft.allowDownloads = value;
              mark();
            }
          }}
          aria-label="Разрешить предлагать скачанные моды"
        />
      </div>

      <div class="row">
        <div class="row-text">
          <label for="downloads-folder">Куда скачивать обновления</label>
          <small>По умолчанию системные «Загрузки». Сюда попадут архивы из окна загрузки.</small>
        </div>
        <div class="path-row">
          <input
            id="downloads-folder"
            readonly
            value={draft.downloadsFolder ?? ""}
            placeholder="системные «Загрузки»"
            title={draft.downloadsFolder ?? "Системные «Загрузки»"}
          />
          <button type="button" onclick={() => void pickFolder("downloadsFolder")}>
            <FolderOpen size={13} />
            Выбрать
          </button>
          <button type="button" disabled={!draft.downloadsFolder} onclick={resetDownloadsFolder}>
            Сбросить
          </button>
        </div>
      </div>
      {#if downloadsInsideMods}
        <p class="field-warning">
          Эта папка внутри папки модов: игра увидит скачанный архив как ещё один мод.
          Выберите папку вне модов — иначе программа будет сохранять архивы в системные «Загрузки».
        </p>
      {/if}

      <div class="switch-row">
        <div>
          <b>Удалять скачанный архив после установки</b>
          <small>Файл из «Загрузок» или окна загрузки удалится, когда мод встанет. Копия прошлой версии останется.</small>
        </div>
        <Switch
          size="lg"
          checked={draft.deleteDownloadsAfterInstall}
          onCheckedChange={(value) => {
            if (!draft) return;
            draft.deleteDownloadsAfterInstall = value;
            mark();
          }}
          aria-label="Удалять скачанный архив после установки"
        />
      </div>

      <div class="row">
        <span class="label">Ещё папки со скачанными модами</span>
        <div class="watch">
          {#each draft.watchFolders as folder (folder)}
            <div class="watch-row">
              <span>{folder}</span>
              <button type="button" aria-label="Убрать папку" onclick={() => removeWatchFolder(folder)}>
                <X size={12} />
              </button>
            </div>
          {/each}
          <button class="add" type="button" onclick={() => void addWatchFolder()}>
            <Plus size={13} />
            Добавить папку
          </button>
        </div>
      </div>
    </section>
        {/if}
        {#if activeSection === "updates"}
        <section id="settings-updates" class="block">
          <div class="block-head">
            <h2 class="display">Обновления</h2>
        <span>когда и сколько проверять</span>
      </div>

      <div class="row">
        <div class="row-text">
          <label for="ttl">Не перепроверять мод чаще, чем раз в</label>
          <small>Недавно проверенные моды пропускаются — проверка быстрее.</small>
        </div>
        <div class="unit">
          <input
            id="ttl"
            class="number"
            type="number"
            min="1"
            max="168"
            bind:value={draft.checkTtlHours}
            oninput={mark}
          />
          <span>ч</span>
        </div>
      </div>

      <div class="row">
        <div class="row-text">
          <label for="cards">Страниц с одного сайта за проверку</label>
          <small>Остальные проверятся в следующий раз — так сайт не закроет доступ.</small>
        </div>
        <div class="unit">
          <input
            id="cards"
            class="number"
            type="number"
            min="5"
            max="500"
            bind:value={draft.maxCardChecksPerSource}
            oninput={mark}
          />
          <span>шт.</span>
        </div>
      </div>

      <div class="row">
        <div class="row-text">
          <label for="auto-every">Проверять, пока программа открыта, каждые</label>
          <small>0 — не проверять по расписанию, только при запуске и по кнопке.</small>
        </div>
        <div class="unit">
          <input
            id="auto-every"
            class="number"
            type="number"
            min="0"
            max="168"
            bind:value={draft.autoCheckEveryHours}
            oninput={mark}
          />
          <span>ч</span>
        </div>
      </div>

      <div class="switch-row">
        <div>
          <b>Проверять при запуске</b>
          <small>Сразу после открытия сверять версии модов с сайтами.</small>
        </div>
        <Switch
          size="lg"
          checked={draft.autoCheckOnStart}
          onCheckedChange={(value) => {
            if (draft) {
              draft.autoCheckOnStart = value;
              mark();
            }
          }}
          aria-label="Проверять обновления при запуске"
        />
      </div>
      <div class="switch-row">
        <div>
          <b>Проверять выключенные моды</b>
          <small>Обновления ищутся и для модов, которые сейчас выключены. Выключите, чтобы не тратить запросы.</small>
        </div>
        <Switch
          size="lg"
          checked={draft.checkDisabledMods}
          onCheckedChange={(value) => {
            if (draft) {
              draft.checkDisabledMods = value;
              mark();
            }
          }}
          aria-label="Проверять обновления выключенных модов"
        />
      </div>
      <div class="switch-row">
        <div>
          <b>Обновления программы</b>
          <small>Проверять новые версии самой программы на GitHub.</small>
        </div>
        <Switch
          size="lg"
          checked={draft.checkAppUpdates}
          onCheckedChange={(value) => {
            if (draft) {
              draft.checkAppUpdates = value;
              mark();
            }
          }}
          aria-label="Проверять обновления программы"
        />
      </div>
    </section>
        {/if}
        {#if activeSection === "sites"}
        <section id="settings-sites" class="block">
          <div class="block-head">
            <h2 class="display">Сайты</h2>
        <span>где искать моды и обновления</span>
      </div>
      <div class="switch-list">
        {#each SOURCES as source (source.id)}
          <div class="switch-row">
            <div>
              <b>{source.label}</b>
              <small>{source.note}</small>
            </div>
            <Switch
              size="lg"
              checked={sourceEnabled(source.id)}
              onCheckedChange={(value) => setSource(source.id, value)}
              aria-label={source.label}
            />
          </div>
        {/each}
      </div>
      <p class="hint">Поиск использует включённые каталоги; BeamNG нужен для обновлений и страниц модов.</p>
    </section>
        {/if}
        {#if activeSection === "program"}
        <section id="settings-program" class="block">
          <div class="block-head">
            <h2 class="display">О программе</h2>
            <span>версия {appUpdate?.currentVersion ?? "…"}</span>
          </div>
          <p class="about-lead">
            BeamNG Mod Manager держит моды BeamNG.drive в порядке: гараж, установка,
            проверка обновлений и поиск сразу по нескольким сайтам.
          </p>
          <div class="about-row">
            <div class="about-update">
              <p class="about-stage">{updateStageText()}</p>
              {#if updateError}
                <span class="update-error">{updateError}</span>
              {/if}
              {#if appUpdate?.state.stage === "downloading"}
                <div class="update-track">
                  <span style={`width:${Math.max(2, Math.round(updateProgress * 100))}%`}></span>
                </div>
              {/if}
            </div>
            <div class="update-actions">
              {#if appUpdate?.state.stage === "available"}
                <button class="primary-small" type="button" disabled={updateBusy} onclick={() => void downloadAppUpdate()}>
                  <Download size={13} />
                  Скачать
                </button>
              {:else if appUpdate?.state.stage === "downloading"}
                <button type="button" onclick={() => void cancelAppUpdate()}>Отменить</button>
              {:else if appUpdate?.state.stage === "readyToRestart"}
                <button class="primary-small" type="button" onclick={() => restartAppUpdate()}>
                  <RotateCcw size={13} />
                  Перезапустить
                </button>
              {:else if appUpdate?.state.stage === "checking"}
                <button type="button" disabled>Проверяем…</button>
              {:else}
                <button type="button" disabled={updateBusy} onclick={() => void checkAppUpdate()}>
                  {updateBusy ? "Проверяем…" : "Проверить обновление"}
                </button>
              {/if}
            </div>
          </div>
          <div class="about-links">
            <button type="button" onclick={() => void openExternal(REPO_URL)}>GitHub</button>
            <button type="button" onclick={() => void openExternal(RELEASES_URL)}>Релизы и загрузки</button>
            <button type="button" onclick={() => void revealDataFolder()}>Папка данных</button>
          </div>
          {#if releaseNotes.length > 0}
            <details class="about-notes">
              <summary>Что нового в версии {appUpdate?.state.manifest?.version ?? appUpdate?.currentVersion ?? ""}</summary>
              <ul>
                {#each releaseNotes as note}
                  <li>{note}</li>
                {/each}
              </ul>
            </details>
          {/if}
        </section>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .settings-page {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 150px 14px 24px;
    border-bottom: 1px solid var(--border);
  }

  h1 {
    margin: 0;
    font-size: 30px;
    line-height: 1.05;
  }


  .save-state {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .save-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--ok);
    transition: background-color var(--dur-2) var(--ease-out);
  }

  .save-dot.busy,
  .save-dot.invalid {
    background: var(--warning);
  }

  .save-dot.failed {
    background: var(--danger);
  }

  .banner {
    margin: 12px 24px 0;
    padding: 10px 14px;
    border-radius: 8px;
    font-size: 13px;
  }

  .banner.error {
    border: 1px solid color-mix(in oklab, var(--danger) 45%, transparent);
    background: var(--danger-soft);
    color: var(--danger);
  }

  .state {
    display: grid;
    place-content: center;
    min-height: 240px;
    color: var(--muted-foreground);
  }

  .settings-layout {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    flex: 1;
    min-height: 0;
  }

  .settings-side {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 16px 12px;
    border-right: 1px solid var(--border);
    background: var(--surface);
  }

  .settings-side button {
    position: relative;
    display: flex;
    align-items: center;
    gap: 10px;
    height: 38px;
    padding: 0 12px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 14px;
    font-weight: 600;
    text-align: left;
    cursor: default;
    transition:
      background-color var(--dur-2) var(--ease-out),
      color var(--dur-2) var(--ease-out);
  }

  .settings-side button:hover {
    background: var(--surface-2);
    color: var(--foreground);
  }

  .settings-side button.active {
    background: var(--surface-3);
    color: var(--foreground);
    box-shadow: inset 3px 0 0 var(--accent-brand);
  }

  .settings-sections {
    min-width: 0;
    overflow-y: auto;
    padding: 24px;
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: 920px;
    animation: rise-in var(--dur-3) var(--ease-out) both;
  }

  .block-head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 12px;
  }

  .block-head h2 {
    margin: 0;
    font-size: 22px;
  }

  .block-head span {
    color: var(--faint-foreground);
    font-size: 13px;
  }

  .group-title {
    display: block;
    margin: 14px 0 8px;
    color: var(--faint-foreground);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .appearance-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 8px 32px;
  }

  .presets {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }

  .presets button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 3px;
    padding: 12px 14px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--foreground);
    text-align: left;
    cursor: default;
    transition:
      border-color var(--dur-2) var(--ease-out),
      background-color var(--dur-2) var(--ease-out);
  }

  .presets button:hover {
    background: var(--surface-2);
  }

  .presets button.active {
    border-color: var(--accent-brand);
    box-shadow: inset 0 0 0 1px var(--accent-brand);
  }

  .presets b {
    font-size: 14.5px;
    font-weight: 800;
  }

  .presets small,
  .switch-row small {
    color: var(--muted-foreground);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .preset-note {
    margin: 8px 0 0;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .preset-note button {
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--accent-brand-text);
    font: inherit;
    font-weight: 700;
    cursor: default;
  }

  .preset-note button:hover {
    text-decoration: underline;
  }

  .switch-list {
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }

  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 14px;
  }

  .switch-row + .switch-row {
    border-top: 1px solid var(--border);
  }

  .block > .switch-row {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }

  .block > .switch-row + .switch-row {
    margin-top: 0;
  }

  .switch-row div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .switch-row b {
    font-size: 14px;
    font-weight: 700;
  }

  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .choices button {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 34px;
    padding: 0 14px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--muted-foreground);
    font-size: 13.5px;
    font-weight: 600;
    cursor: default;
  }

  .choices button.active {
    border-color: var(--foreground);
    background: var(--foreground);
    color: var(--background);
  }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  /* Пять акцентов: последний добирает пустое место, чтобы ряд не выглядел обрезанным. */
  .theme-grid .swatch:last-child:nth-child(5) {
    grid-column: span 2;
  }

  .swatch {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--foreground);
    text-align: left;
    cursor: default;
    transition:
      transform var(--dur-1) var(--ease-out),
      border-color var(--dur-2) var(--ease-out);
  }

  .swatch:hover {
    transform: translateY(-1px);
    border-color: var(--faint-foreground);
  }

  .swatch.active {
    border-color: var(--accent-brand);
  }

  .tone {
    display: grid;
    flex: none;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    color: #111;
    box-shadow: inset 0 0 0 1px rgb(0 0 0 / 0.15);
  }

  .swatch-text {
    display: flex;
    flex-direction: column;
  }

  .swatch-text b {
    font-size: 13.5px;
    font-weight: 700;
  }

  .swatch-text small {
    color: var(--faint-foreground);
    font-size: 11.5px;
  }

  .group-hint {
    margin: 7px 0 0;
    color: var(--faint-foreground);
    font-size: 11.5px;
    line-height: 1.4;
  }

  .backdrop-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .backdrop-swatch {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 8px;
    border: 1px solid var(--border-strong);
    border-radius: 10px;
    background: var(--surface);
    color: var(--foreground);
    text-align: left;
    cursor: default;
    transition:
      transform var(--dur-1) var(--ease-out),
      border-color var(--dur-2) var(--ease-out);
  }

  .backdrop-swatch:hover {
    transform: translateY(-1px);
    border-color: var(--faint-foreground);
  }

  .backdrop-swatch.active {
    border-color: var(--accent-brand);
    box-shadow: 0 0 0 1px var(--accent-brand);
  }

  .backdrop-tone {
    display: grid;
    place-items: center;
    height: 46px;
    border-radius: 7px;
    color: #fff;
    text-shadow: 0 1px 2px rgb(0 0 0 / 0.35);
  }

  .backdrop-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }

  .backdrop-text b {
    font-size: 13px;
    font-weight: 700;
  }

  .backdrop-text small {
    color: var(--faint-foreground);
    font-size: 11px;
  }

  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 10px 16px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }

  .row label,
  .row .label {
    font-size: 14px;
    font-weight: 700;
  }

  .path-row {
    display: flex;
    flex: 1 1 420px;
    gap: 6px;
  }

  .path-row input {
    flex: 1;
    min-width: 0;
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--foreground);
    font-size: 13px;
    user-select: text;
  }

  .path-row input::placeholder {
    color: var(--faint-foreground);
  }

  .path-row button,
  .update-actions button,
  .add,
  .watch-row button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--foreground);
    font-size: 13px;
    font-weight: 700;
    cursor: default;
    transition: background-color var(--dur-1) var(--ease-out);
  }

  .path-row button:hover:not(:disabled),
  .update-actions button:hover:not(:disabled),
  .add:hover {
    background: var(--surface-3);
  }

  .path-row button:disabled,
  .update-actions button:disabled {
    opacity: 0.5;
  }

  .update-actions .primary-small {
    border-color: var(--accent-brand);
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
  }

  .update-actions .primary-small:hover:not(:disabled) {
    background: var(--accent-brand-hover);
  }

  .number {
    width: 90px;
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--foreground);
    font-family: var(--font-mono);
    font-size: 13px;
    user-select: text;
  }

  .number:focus,
  .path-row input:focus {
    border-color: var(--accent-brand);
    outline: none;
  }

  .row-text {
    display: flex;
    flex: 1 1 260px;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .row-text small {
    color: var(--muted-foreground);
    font-size: 12.5px;
    line-height: 1.4;
  }

  .field-warning {
    margin: -2px 0 0;
    padding: 8px 10px;
    border: 1px solid var(--warning-border);
    border-radius: 8px;
    background: var(--warning-soft);
    color: var(--warning);
    font-size: 12px;
    line-height: 1.45;
  }

  .unit {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .watch {
    display: flex;
    flex: 1 1 420px;
    flex-direction: column;
    gap: 6px;
  }

  .watch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 6px 6px 6px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    font-size: 13px;
  }

  .watch-row span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .watch-row button {
    height: 26px;
    padding: 0 8px;
  }

  .add {
    align-self: flex-start;
  }

  .hint {
    margin: 10px 2px 0;
    color: var(--faint-foreground);
    font-size: 12.5px;
  }

  .about-lead {
    margin: 0 0 14px;
    max-width: 680px;
    color: var(--muted-foreground);
    font-size: 13.5px;
    line-height: 1.55;
  }

  .about-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px 24px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }

  .about-update {
    display: flex;
    flex: 1 1 320px;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }

  .about-stage {
    margin: 0;
    font-size: 14px;
    font-weight: 700;
  }

  .update-error {
    color: var(--danger);
    font-size: 12.5px;
  }

  .about-links {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 12px;
  }

  .about-links button {
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--foreground);
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    cursor: default;
  }

  .about-links button:hover {
    background: var(--surface-3);
  }

  .about-notes {
    margin-top: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
  }

  .about-notes summary {
    font-size: 13px;
    font-weight: 600;
    cursor: default;
  }

  .about-notes ul {
    overflow-wrap: anywhere;
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 8px 0 0;
    padding-left: 18px;
    list-style: disc;
    color: var(--muted-foreground);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .update-track {
    height: 6px;
    margin-top: 4px;
    overflow: hidden;
    border-radius: 3px;
    background: var(--surface-3);
  }

  .update-track span {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: var(--accent-brand);
    transition: width var(--dur-3) var(--ease-out);
  }

  .update-actions {
    display: flex;
    gap: 8px;
  }

  @media (max-width: 860px) {
    .settings-layout {
      grid-template-columns: minmax(0, 1fr);
    }

    .settings-side {
      flex-direction: row;
      flex-wrap: wrap;
      border-right: 0;
      border-bottom: 1px solid var(--border);
    }
  }
</style>

<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import {
    Check,
    Cpu,
    Download,
    FolderOpen,
    Globe,
    Monitor,
    Moon,
    Package,
    Palette,
    Plus,
    RefreshCw,
    RotateCcw,
    Sun,
    X,
  } from "@lucide/svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { Switch } from "$lib/components/ui/switch";
  import { appearance } from "$lib/shell/appearance.svelte";
  import { type SettingsDto } from "$lib/ipc/settings";
  import {
    appUpdateCancel,
    appUpdateCheck,
    appUpdateDownload,
    appUpdateRestart,
    appUpdateState,
    listenAppUpdateProgress,
    type AppUpdateSnapshot,
  } from "$lib/ipc/appUpdate";

  type Theme = SettingsDto["theme"];
  type Mode = SettingsDto["mode"];

  const THEMES: { id: Theme; label: string; desc: string }[] = [
    { id: "night", label: "Ночь", desc: "синий, сиреневый" },
    { id: "deep", label: "Глубина", desc: "бирюзовый, синий" },
    { id: "dusk", label: "Закат", desc: "сливовый, янтарь" },
    { id: "forest", label: "Лес", desc: "изумрудный, золото" },
    { id: "graphite", label: "Графит", desc: "серый, голубой" },
  ];
  const MODES: { id: Mode; label: string; icon: typeof Moon }[] = [
    { id: "dark", label: "Тёмная", icon: Moon },
    { id: "light", label: "Светлая", icon: Sun },
    { id: "system", label: "Как в системе", icon: Monitor },
  ];
  const SECTIONS = [
    { id: "appearance", label: "Внешний вид", icon: Palette },
    { id: "folders", label: "Папки", icon: FolderOpen },
    { id: "mods", label: "Моды и загрузки", icon: Package },
    { id: "updates", label: "Обновления", icon: RefreshCw },
    { id: "sites", label: "Сайты", icon: Globe },
    { id: "program", label: "Программа", icon: Cpu },
  ] as const;
  type SectionId = (typeof SECTIONS)[number]["id"];
  const SOURCES = [
    { id: "modland", label: "Modland", note: "Каталог с проверкой «вы человек»" },
    { id: "stmods", label: "STMods", note: "Каталог модов BeamNG" },
    { id: "modsgaming", label: "ModsGaming", note: "Русский каталог, картинки со страницы" },
    { id: "beamng", label: "BeamNG (официальный)", note: "Репозиторий и обновления от авторов" },
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
    activeSection = id;
    if (sectionsEl) sectionsEl.scrollTop = 0;
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

  onMount(() => {
    void boot();
    void loadAppUpdate();
    void listenAppUpdateProgress((value) => {
      updateProgress = value;
    }).then((stop) => {
      unlistenUpdate = stop;
    });
    return () => unlistenUpdate?.();
  });

  onDestroy(() => {
    unlistenUpdate?.();
    if (saveTimer) clearTimeout(saveTimer);
    if (dirty) void save();
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

  function updateStageText(): string {
    const stage = appUpdate?.state.stage;
    switch (stage) {
      case "available":
        return `Доступна версия ${appUpdate?.state.manifest?.version ?? ""}`;
      case "upToDate":
        return "Установлена последняя версия";
      case "downloading":
        return `Скачиваем ${Math.round(updateProgress * 100)}%`;
      case "readyToRestart":
        return `Версия ${appUpdate?.state.manifest?.version ?? ""} готова`;
      case "failed":
        return appUpdate?.state.error ?? "Не удалось проверить обновление";
      case "checking":
        return "Проверяем обновление…";
      default:
        return "Проверить обновления программы";
    }
  }

  async function boot() {
    loading = true;
    try {
      const loaded = appearance.settings ?? (await appearance.load());
      if (loaded) draft = clone(loaded);
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

  function mark() {
    dirty = true;
    message = null;
    revision += 1;
    scheduleSave();
  }

  function scheduleSave() {
    if (saveTimer) clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void save(), 350);
  }

  async function save() {
    if (!draft || saving) return;
    const rev = revision;
    saving = true;
    error = null;
    try {
      const saved = clone(await appearance.save(draft));
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
    draft?.liquidGlass && draft.uiAnimations && draft.fireflies
      ? "beauty"
      : draft && !draft.liquidGlass && !draft.uiAnimations && !draft.fireflies
        ? "fast"
        : null,
  );

  function applyPreset(kind: "beauty" | "fast") {
    if (!draft) return;
    const on = kind === "beauty";
    draft.liquidGlass = on;
    draft.uiAnimations = on;
    draft.fireflies = on;
    draft.backgroundTilt = false;
    mark();
  }

  function setFlag(
    key: "liquidGlass" | "uiAnimations" | "fireflies" | "backgroundTilt",
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

  async function pickFolder(field: "modsFolder" | "userFolder") {
    if (!draft) return;
    if (!isTauri()) {
      message = "Выбор папки доступен в приложении.";
      return;
    }
    try {
      const picked = await open({
        directory: true,
        multiple: false,
        title: field === "modsFolder" ? "Папка модов (…\\mods)" : "Папка пользователя игры",
        // Открываем там, где папка уже выбрана, а не в последней случайной.
        defaultPath: draft[field] || undefined,
      });
      if (typeof picked === "string") {
        draft[field] = picked;
        mark();
      }
    } catch (reason) {
      error = errorText(reason);
    }
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
      error = errorText(reason);
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
      await openPath(path);
    } catch (reason) {
      error = errorText(reason);
    }
  }

  function errorText(reason: unknown): string {
    if (typeof reason === "string") return reason;
    if (reason && typeof reason === "object" && "message" in reason) {
      return String((reason as { message: unknown }).message);
    }
    return "Не удалось сохранить настройки.";
  }
</script>

<div class="settings-page">
  <header class="head">
    <div>
      <h1>Настройки</h1>
      <p>изменения сохраняются сами</p>
    </div>
    <div class="head-actions">
      <span class="save-state">
        <span class="save-dot" class:busy={(saving || dirty) && !error} class:failed={!!error}></span>
        {error
          ? "Не сохранено"
          : saving
            ? "Сохраняем…"
            : dirty
              ? "Сохраняем изменения…"
              : message ?? "Все изменения сохранены"}
      </span>
    </div>
  </header>

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  {#if loading || !draft}
    <div class="state glass liquid-edge"><p>Загружаем настройки…</p></div>
  {:else}
    <div class="settings-layout">
      <nav class="settings-side glass liquid-edge" aria-label="Разделы настроек">
        {#each SECTIONS as section (section.id)}
          {@const Icon = section.icon}
          <button
            type="button"
            class:active={activeSection === section.id}
            onclick={() => selectSection(section.id)}
          >
            <Icon size={16} strokeWidth={1.8} />
            <span>{section.label}</span>
          </button>
        {/each}
      </nav>
      <div class="settings-sections" bind:this={sectionsEl}>
        {#if activeSection === "appearance"}
        <section id="settings-appearance" class="block glass liquid-edge">
          <div class="block-head">
            <h2>Внешний вид</h2>
            <span>тема, панели и движение</span>
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
              <b>Красиво</b>
              <small>полупрозрачные панели, анимации, огоньки</small>
            </button>
            <button
              type="button"
              class:active={preset === "fast"}
              onclick={() => applyPreset("fast")}
            >
              <b>Быстро</b>
              <small>плотные панели, без анимаций</small>
            </button>
          </div>

          <span class="group-title">Движение</span>
          <div class="switch-list">
            <div class="switch-row">
              <div>
                <b>Полупрозрачные панели</b>
                <small>Сквозь панели виден цветной фон. Выключено — плотные панели.</small>
              </div>
              <Switch
                size="lg"
                checked={draft.liquidGlass}
                onCheckedChange={(value) => setFlag("liquidGlass", value)}
                aria-label="Полупрозрачные панели"
              />
            </div>
            <div class="switch-row">
              <div>
                <b>Анимации интерфейса</b>
                <small>Капля-выбор, появление и подъём карточек.</small>
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
                <b>Огоньки</b>
                <small>Над фоном плавают светлые точки, на кромках стекла вспыхивают искры.</small>
              </div>
              <Switch
                size="lg"
                checked={draft.fireflies}
                onCheckedChange={(value) => setFlag("fireflies", value)}
                aria-label="Огоньки"
              />
            </div>
            <div class="switch-row">
              <div>
                <b>Наклон фона</b>
                <small>Фон слегка следует за мышкой.</small>
              </div>
              <Switch
                size="lg"
                checked={draft.backgroundTilt}
                onCheckedChange={(value) => setFlag("backgroundTilt", value)}
                aria-label="Наклон фона"
              />
            </div>
          </div>
        </div>

        <div class="appearance-right">
          <span class="group-title">Оформление</span>
          <div class="settings-subcard">
            <div class="subcard-title">Тема</div>
            <p class="hint">Светлое стекло с тёмным текстом или тёмное.</p>
            <div class="choices">
              {#each MODES as mode (mode.id)}
                {@const ModeIcon = mode.icon}
                <button
                  type="button"
                  class:active={draft.mode === mode.id}
                  onclick={() => {
                    if (draft) {
                      draft.mode = mode.id;
                      mark();
                    }
                  }}
                >
                  <ModeIcon size={14} strokeWidth={1.8} />
                  {mode.label}
                </button>
              {/each}
            </div>
          </div>

          <span class="group-title">Фон</span>
          <div class="theme-grid">
            {#each THEMES as theme (theme.id)}
              <button
                type="button"
                class="swatch {theme.id}"
                class:active={draft.theme === theme.id}
                onclick={() => {
                  if (draft) {
                    draft.theme = theme.id;
                    mark();
                  }
                }}
              >
                <span class="tone"></span>
                {#if draft.theme === theme.id}
                  <span class="chosen"><Check size={13} strokeWidth={2.4} /></span>
                {/if}
                <b>{theme.label}</b>
                <small>{theme.desc}</small>
              </button>
            {/each}
          </div>
        </div>
      </div>
    </section>
        {/if}
        {#if activeSection === "folders"}
        <section id="settings-folders" class="block glass liquid-edge">
          <div class="block-head">
            <h2>Папки</h2>
        <span>моды и данные игры</span>
      </div>

      <div class="row">
        <label for="mods-folder">Папка модов</label>
        <div class="path-row">
          <input id="mods-folder" readonly value={draft.modsFolder ?? ""} placeholder="не выбрана" />
          <button type="button" onclick={() => void pickFolder("modsFolder")}>
            <FolderOpen size={13} />
            Выбрать
          </button>
          <button
            type="button"
            disabled={!draft.modsFolder}
            onclick={() => void openFolder(draft?.modsFolder ?? null)}
          >
            Открыть
          </button>
        </div>
      </div>

      <div class="row">
        <label for="user-folder">Папка пользователя игры</label>
        <div class="path-row">
          <input id="user-folder" readonly value={draft.userFolder ?? ""} placeholder="определяется автоматически" />
          <button type="button" onclick={() => void pickFolder("userFolder")}>
            <FolderOpen size={13} />
            Выбрать
          </button>
          <button
            type="button"
            disabled={!draft.userFolder}
            onclick={() => void openFolder(draft?.userFolder ?? null)}
          >
            Открыть
          </button>
        </div>
      </div>
    </section>
        {/if}
        {#if activeSection === "mods"}
        <section id="settings-mods" class="block glass liquid-edge">
          <div class="block-head">
            <h2>Моды и загрузки</h2>
        <span>копии, предупреждения и наблюдение за папками</span>
      </div>

      <div class="row">
        <label for="keep-backups">Сколько копий хранить</label>
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
        <label for="warn-above">Предупреждать, если включено больше</label>
        <input
          id="warn-above"
          class="number"
          type="number"
          min="0"
          max="1000"
          bind:value={draft.activeModsWarnAbove}
          oninput={mark}
        />
      </div>

      <div class="switch-row">
        <div>
          <b>Скачанные моды</b>
          <small>Предлагать установку архивов из «Загрузок» и наблюдаемых папок.</small>
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
        <span class="label">Наблюдать за папками</span>
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
        <section id="settings-updates" class="block glass liquid-edge">
          <div class="block-head">
            <h2>Обновления</h2>
        <span>как часто проверять сайты</span>
      </div>

      <div class="row">
        <label for="ttl">Проверять не чаще чем раз в</label>
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
        <label for="cards">Карточек с сайта за проверку</label>
        <input
          id="cards"
          class="number"
          type="number"
          min="5"
          max="500"
          bind:value={draft.maxCardChecksPerSource}
          oninput={mark}
        />
      </div>

      <div class="row">
        <label for="auto-every">Автопроверка каждые</label>
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
          <span>ч (0 — выключена)</span>
        </div>
      </div>

      <div class="switch-row">
        <div>
          <b>Проверять при запуске</b>
          <small>Сразу после открытия программы сверять версии модов с сайтами.</small>
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
          <b>Обновления программы</b>
          <small>Проверять новые версии самого менеджера на GitHub.</small>
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
        <section id="settings-sites" class="block glass liquid-edge">
          <div class="block-head">
            <h2>Сайты</h2>
        <span>где искать моды</span>
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
      <p class="hint">Поиск и проверка обновлений используют только включённые сайты.</p>
    </section>
        {/if}
        {#if activeSection === "program"}
        <section id="settings-program" class="block glass liquid-edge">
          <div class="block-head">
            <h2>Программа</h2>
        <span>версия {appUpdate?.currentVersion ?? "1.0.0"}</span>
      </div>
      <div class="update-row">
        <div class="update-main">
          <p>{updateStageText()}</p>
          {#if appUpdate?.state.manifest?.notes}
            <span>{appUpdate.state.manifest.notes}</span>
          {/if}
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
            <button class="primary-small" type="button" onclick={() => void restartAppUpdate()}>
              <RotateCcw size={13} />
              Перезапустить
            </button>
          {:else}
            <button type="button" disabled={updateBusy} onclick={() => void checkAppUpdate()}>
              {updateBusy ? "Проверяем…" : "Проверить обновление"}
            </button>
          {/if}
        </div>
      </div>
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
    gap: 13px;
    height: 100%;
    min-height: 0;
    overflow: hidden;
    padding-bottom: 10px;
  }

  .head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    padding: 4px 4px 0;
  }

  h1 {
    margin: 0;
    font-size: 26px;
    font-weight: 650;
    letter-spacing: -0.02em;
  }

  .head p {
    margin: 4px 0 0;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .save-state {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--muted-foreground);
    font-size: 12.5px;
  }

  .save-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: oklch(0.72 0.14 150);
  }

  .save-dot.busy {
    background: color-mix(in oklab, var(--primary) 80%, white 10%);
    animation: save-pulse 1.1s ease-in-out infinite;
  }

  .save-dot.failed {
    background: oklch(0.68 0.19 25);
  }

  @keyframes save-pulse {
    50% {
      opacity: 0.35;
    }
  }

  .banner.error {
    padding: 9px 12px;
    border: 1px solid color-mix(in oklab, oklch(0.75 0.16 25) 35%, transparent);
    border-radius: 12px;
    background: color-mix(in oklab, oklch(0.75 0.16 25) 10%, transparent);
    color: oklch(0.82 0.12 25);
    font-size: 10.5px;
  }

  .block {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px 18px;
    border-radius: 18px;
  }

  .block-head {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }

  .block-head h2 {
    margin: 0;
    font-size: 14px;
    font-weight: 630;
  }

  .block-head span {
    color: var(--muted-foreground);
    font-size: 9.5px;
  }

  .row {
    display: grid;
    grid-template-columns: minmax(180px, 280px) minmax(0, 1fr);
    align-items: center;
    gap: 12px;
  }

  .row > label,
  .row > .label {
    color: var(--muted-foreground);
    font-size: 11px;
  }

  .appearance-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.12fr) minmax(300px, 0.88fr);
    align-items: start;
    gap: 22px;
  }

  .appearance-left,
  .appearance-right {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
  }

  .group-title {
    color: var(--muted-foreground);
    font-size: 9px;
    font-weight: 650;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .presets {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }

  .presets button {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 10px 12px;
    border: 1px solid var(--glass-border);
    border-radius: 13px;
    background: color-mix(in oklab, var(--foreground) 4%, transparent);
    color: var(--foreground);
    text-align: left;
    cursor: default;
  }

  .presets button b {
    font-size: 11.5px;
    font-weight: 600;
  }

  .presets button small {
    color: var(--muted-foreground);
    font-size: 9px;
    line-height: 1.35;
  }

  .presets button.active {
    border-color: color-mix(in oklab, var(--primary) 55%, transparent);
    background: color-mix(in oklab, var(--primary) 11%, transparent);
  }

  .switch-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 12px;
    border: 1px solid color-mix(in oklab, var(--foreground) 6%, transparent);
    border-radius: 13px;
    background: color-mix(in oklab, var(--foreground) 3%, transparent);
  }

  .switch-row b {
    display: block;
    font-size: 10.5px;
    font-weight: 600;
  }

  .switch-row small {
    display: block;
    margin-top: 2px;
    color: var(--muted-foreground);
    font-size: 9px;
    line-height: 1.4;
  }

  .switch-row :global([data-slot="switch"]) {
    flex: none;
  }

  .settings-subcard {
    padding: 12px;
    border: 1px solid color-mix(in oklab, var(--foreground) 6%, transparent);
    border-radius: 14px;
    background: color-mix(in oklab, var(--foreground) 3%, transparent);
  }

  .subcard-title {
    margin-bottom: 3px;
    font-size: 10.5px;
    font-weight: 620;
  }

  @media (max-width: 1150px) {
    .appearance-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }

  .choices {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .choices button {
    height: 27px;
    padding: 0 11px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 10px;
    cursor: default;
  }

  .choices button.active {
    background: color-mix(in oklab, var(--primary) 15%, transparent);
    color: var(--foreground);
    border-color: color-mix(in oklab, var(--primary) 45%, transparent);
  }

  .theme-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 8px;
  }

  .swatch {
    position: relative;
    display: block;
    height: 64px;
    padding: 0;
    overflow: hidden;
    border: 1px solid var(--glass-border);
    border-radius: 13px;
    background: transparent;
    color: #fff;
    text-align: left;
    cursor: default;
  }

  .swatch .tone {
    position: absolute;
    inset: 0;
  }

  .swatch::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(to top, rgb(5 8 16 / 0.8), transparent 68%);
  }

  .swatch.night .tone {
    background: linear-gradient(135deg, #3d3580, #1b4b63);
  }
  .swatch.deep .tone {
    background: linear-gradient(135deg, #2c3e92, #14606b);
  }
  .swatch.dusk .tone {
    background: linear-gradient(135deg, #a5366f, #c46b35);
  }
  .swatch.forest .tone {
    background: linear-gradient(135deg, #236544, #4b7a35);
  }
  .swatch.graphite .tone {
    background: linear-gradient(135deg, #4c4f58, #2d3038);
  }

  .swatch b,
  .swatch small {
    position: absolute;
    left: 8px;
    right: 8px;
    z-index: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-shadow: 0 1px 8px rgb(0 0 0 / 0.5);
  }

  .swatch b {
    bottom: 18px;
    font-size: 10.5px;
    font-weight: 600;
  }

  .swatch small {
    bottom: 7px;
    color: rgb(255 255 255 / 0.68);
    font-size: 8px;
  }

  .swatch.active {
    border-color: color-mix(in oklab, var(--primary) 80%, white 10%);
    box-shadow: 0 0 0 1px color-mix(in oklab, var(--primary) 55%, transparent);
  }

  .chosen {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 2;
    display: grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 999px;
    background: rgb(8 12 24 / 0.5);
    color: #fff;
  }

  .number {
    width: 86px;
    height: 28px;
    padding: 0 9px;
    border: 1px solid var(--glass-border);
    border-radius: 9px;
    background: color-mix(in oklab, var(--foreground) 4%, transparent);
    color: var(--foreground);
    font-size: 11px;
  }

  .unit {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .path-row {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }

  .path-row input {
    min-width: 0;
    flex: 1;
    height: 29px;
    padding: 0 10px;
    border: 1px solid var(--glass-border);
    border-radius: 9px;
    background: color-mix(in oklab, var(--foreground) 4%, transparent);
    color: var(--muted-foreground);
    font-size: 10.5px;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .path-row button,
  .watch button,
  .add {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: transparent;
    color: var(--foreground);
    font-size: 9.5px;
    cursor: default;
  }

  .path-row button:disabled {
    opacity: 0.45;
  }

  .watch {
    display: flex;
    flex-direction: column;
    gap: 5px;
    align-items: flex-start;
  }

  .watch-row {
    display: flex;
    align-items: center;
    gap: 6px;
    max-width: 100%;
    padding: 4px 4px 4px 10px;
    border: 1px solid var(--glass-border);
    border-radius: 9px;
    color: var(--muted-foreground);
    font-size: 10px;
  }

  .watch-row span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .watch-row button {
    width: 22px;
    height: 22px;
    justify-content: center;
    padding: 0;
  }

  .hint {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 9.5px;
  }

  .update-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
  }

  .update-main {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }

  .update-main p {
    margin: 0;
    color: var(--foreground);
    font-size: 11px;
  }

  .update-main span {
    color: var(--muted-foreground);
    font-size: 9.5px;
    line-height: 1.4;
  }

  .update-error {
    color: oklch(0.82 0.12 25);
  }

  .update-track {
    width: min(320px, 100%);
    height: 4px;
    overflow: hidden;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
  }

  .update-track span {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--primary);
    transition: width 0.2s ease-out;
  }

  .update-actions {
    flex: none;
  }

  .update-actions button {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: transparent;
    color: var(--foreground);
    font-size: 9.5px;
    cursor: default;
  }

  .update-actions .primary-small {
    border-color: transparent;
    background: var(--primary);
    color: var(--primary-foreground);
  }

  .update-actions button:disabled {
    opacity: 0.5;
  }

  .state {
    display: grid;
    place-content: center;
    min-height: 260px;
    border-radius: 18px;
    color: var(--muted-foreground);
    font-size: 12px;
  }

  @media (max-width: 760px) {
    .row {
      grid-template-columns: minmax(0, 1fr);
      gap: 5px;
    }
    .theme-grid {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }

  /* ── раскладка с разделами: ближе к legacy ── */
  .settings-layout {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    align-items: stretch;
    gap: 18px;
    flex: 1;
    min-height: 0;
  }

  .settings-side {
    align-self: start;
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 10px;
    border-radius: 20px;
  }

  .settings-side button {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 42px;
    padding: 0 13px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 13px;
    font-weight: 520;
    text-align: left;
    cursor: default;
    transition:
      background 150ms ease,
      color 150ms ease;
  }

  .settings-side button:hover {
    background: color-mix(in oklab, var(--foreground) 6%, transparent);
    color: var(--foreground);
  }

  .settings-side button.active {
    background: color-mix(in oklab, var(--primary) 16%, transparent);
    color: var(--foreground);
    box-shadow: inset 0 0 0 1px color-mix(in oklab, var(--primary) 28%, transparent);
  }

  .settings-sections {
    display: flex;
    flex-direction: column;
    gap: 13px;
    min-width: 0;
    min-height: 0;
    overflow-y: auto;
    padding-right: 4px;
  }

  .settings-sections > section {
    min-height: 100%;
    box-sizing: border-box;
  }

  .hint {
    font-size: 11.5px;
    line-height: 1.45;
  }

  .block {
    gap: 16px;
    padding: 20px 22px;
    border-radius: 20px;
  }

  .block-head h2 {
    font-size: 17px;
    letter-spacing: -0.01em;
  }

  .block-head span {
    font-size: 11.5px;
  }

  .group-title {
    margin-top: 4px;
    font-size: 10px;
  }

  .row > label,
  .row > .label {
    font-size: 13px;
  }

  .appearance-grid {
    grid-template-columns: minmax(0, 1fr);
    gap: 18px;
  }

  .appearance-left,
  .appearance-right {
    gap: 12px;
  }

  .presets button {
    padding: 13px 15px;
    border-radius: 15px;
  }

  .presets button b {
    font-size: 13px;
  }

  .presets button small {
    font-size: 11px;
    line-height: 1.4;
  }

  .switch-row {
    padding: 13px 16px;
    border-radius: 15px;
  }

  .switch-row b {
    font-size: 13.5px;
  }

  .switch-row small {
    margin-top: 3px;
    font-size: 11.5px;
    line-height: 1.4;
  }

  .settings-subcard {
    padding: 16px;
    border-radius: 16px;
  }

  .subcard-title {
    font-size: 13px;
  }

  .choices {
    gap: 7px;
  }

  .choices button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 34px;
    padding: 0 14px;
    font-size: 12.5px;
  }

  .theme-grid {
    gap: 10px;
  }

  .swatch {
    height: 88px;
    border-radius: 15px;
  }

  .swatch b {
    bottom: 24px;
    left: 11px;
    font-size: 12.5px;
  }

  .swatch small {
    bottom: 10px;
    left: 11px;
    font-size: 9.5px;
  }

  .number {
    width: 96px;
    height: 34px;
    font-size: 13px;
  }

  .unit {
    font-size: 12px;
  }

  .path-row input {
    height: 36px;
    font-size: 12.5px;
  }

  .path-row button,
  .watch button,
  .add {
    height: 34px;
    padding: 0 13px;
    font-size: 11.5px;
  }

  .watch-row {
    padding: 6px 6px 6px 12px;
    font-size: 11.5px;
  }

  .update-main p {
    font-size: 13.5px;
  }

  .update-main span {
    font-size: 11.5px;
  }

  .update-actions button {
    height: 34px;
    padding: 0 14px;
    font-size: 11.5px;
  }

  /* ── живое превью вместо абстрактных пятен ── */

















  @media (max-width: 1180px) {
    .settings-layout {
      grid-template-columns: 190px minmax(0, 1fr);
      gap: 14px;
    }

    .settings-side button {
      padding: 0 10px;
      font-size: 12px;
    }
  }
</style>

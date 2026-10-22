<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { FolderOpen, Plus, Save, X } from "@lucide/svelte";
  import { isTauri } from "@tauri-apps/api/core";
  import { appearance } from "$lib/shell/appearance.svelte";
  import { type SettingsDto } from "$lib/ipc/settings";

  type Theme = SettingsDto["theme"];
  type Mode = SettingsDto["mode"];

  const THEMES: { id: Theme; label: string }[] = [
    { id: "night", label: "Ночь" },
    { id: "deep", label: "Глубина" },
    { id: "dusk", label: "Закат" },
    { id: "forest", label: "Лес" },
    { id: "graphite", label: "Графит" },
  ];
  const MODES: { id: Mode; label: string }[] = [
    { id: "dark", label: "Тёмная" },
    { id: "light", label: "Светлая" },
    { id: "system", label: "Как в системе" },
  ];
  const SOURCES = [
    { id: "modland", label: "Modland" },
    { id: "stmods", label: "STMods" },
    { id: "modsgaming", label: "ModsGaming" },
    { id: "beamng", label: "BeamNG (официальный)" },
  ];

  let draft = $state<SettingsDto | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let dirty = $state(false);
  let message = $state<string | null>(null);
  let error = $state<string | null>(null);

  onMount(() => {
    void boot();
  });

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
  }

  async function save() {
    if (!draft || saving) return;
    saving = true;
    error = null;
    try {
      draft = clone(await appearance.save(draft));
      dirty = false;
      message = "Сохранено";
    } catch (reason) {
      error = errorText(reason);
    } finally {
      saving = false;
    }
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
      const picked = await open({ directory: true, multiple: false, title: "Выбрать папку" });
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
      <p>{dirty ? "есть несохранённые изменения" : "изменения применяются после сохранения"}</p>
    </div>
    <div class="head-actions">
      {#if message}
        <span class="saved">{message}</span>
      {/if}
      <button
        class="save glass liquid-edge"
        type="button"
        disabled={!draft || saving || !dirty}
        onclick={() => void save()}
      >
        <Save size={14} />
        {saving ? "Сохраняем…" : "Сохранить"}
      </button>
    </div>
  </header>

  {#if error}
    <div class="banner error">{error}</div>
  {/if}

  {#if loading || !draft}
    <div class="state glass liquid-edge"><p>Загружаем настройки…</p></div>
  {:else}
    <section class="block glass liquid-edge">
      <div class="block-head">
        <h2>Внешний вид</h2>
        <span>тема, стекло и движение</span>
      </div>

      <div class="row">
        <span class="label">Режим</span>
        <div class="choices">
          {#each MODES as mode (mode.id)}
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
              {mode.label}
            </button>
          {/each}
        </div>
      </div>

      <div class="row">
        <span class="label">Фон</span>
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
              <span></span>
              {theme.label}
            </button>
          {/each}
        </div>
      </div>

      <div class="toggles">
        <label class="check">
          <input
            type="checkbox"
            bind:checked={draft.liquidGlass}
            onchange={mark}
          />
          Жидкое стекло
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={draft.uiAnimations} onchange={mark} />
          Анимации интерфейса
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={draft.animatedBackground}
            onchange={mark}
          />
          Движение фона
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={draft.fireflies} onchange={mark} />
          Огоньки
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={draft.backgroundTilt}
            onchange={mark}
          />
          Наклон фона
        </label>
      </div>
    </section>

    <section class="block glass liquid-edge">
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

    <section class="block glass liquid-edge">
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

      <div class="toggles">
        <label class="check">
          <input
            type="checkbox"
            bind:checked={draft.allowDownloads}
            onchange={mark}
          />
          Разрешить предлагать скачанные моды
        </label>
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

    <section class="block glass liquid-edge">
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

      <div class="toggles">
        <label class="check">
          <input
            type="checkbox"
            bind:checked={draft.autoCheckOnStart}
            onchange={mark}
          />
          Проверять при запуске
        </label>
        <label class="check">
          <input
            type="checkbox"
            bind:checked={draft.checkAppUpdates}
            onchange={mark}
          />
          Проверять обновления программы
        </label>
      </div>
    </section>

    <section class="block glass liquid-edge">
      <div class="block-head">
        <h2>Сайты</h2>
        <span>где искать моды</span>
      </div>
      <div class="toggles">
        {#each SOURCES as source (source.id)}
          <label class="check">
            <input
              type="checkbox"
              checked={sourceEnabled(source.id)}
              onchange={(event) =>
                setSource(source.id, (event.currentTarget as HTMLInputElement).checked)}
            />
            {source.label}
          </label>
        {/each}
      </div>
      <p class="hint">Поиск и проверка обновлений используют только включённые сайты.</p>
    </section>
  {/if}
</div>

<style>
  .settings-page {
    display: flex;
    flex-direction: column;
    gap: 13px;
    min-height: 100%;
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
    font-size: 10.5px;
  }

  .head-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .saved {
    color: oklch(0.83 0.14 150);
    font-size: 10px;
  }

  .save {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 13px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: color-mix(in oklab, var(--primary) 16%, transparent);
    color: var(--foreground);
    font-size: 10.5px;
    cursor: default;
  }

  .save:disabled {
    opacity: 0.45;
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
    grid-template-columns: repeat(5, minmax(0, 1fr));
    gap: 6px;
    max-width: 560px;
  }

  .swatch {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 5px;
    padding: 4px;
    border: 1px solid var(--glass-border);
    border-radius: 11px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 9px;
    cursor: default;
  }

  .swatch span {
    height: 30px;
    border-radius: 7px;
  }

  .swatch.night span {
    background: linear-gradient(135deg, #3d3580, #1b4b63);
  }
  .swatch.deep span {
    background: linear-gradient(135deg, #2c3e92, #14606b);
  }
  .swatch.dusk span {
    background: linear-gradient(135deg, #a5366f, #c46b35);
  }
  .swatch.forest span {
    background: linear-gradient(135deg, #236544, #4b7a35);
  }
  .swatch.graphite span {
    background: linear-gradient(135deg, #4c4f58, #2d3038);
  }

  .swatch.active {
    border-color: var(--primary);
    color: var(--foreground);
  }

  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    color: var(--foreground);
    font-size: 10.5px;
    cursor: default;
  }

  .check input {
    width: 15px;
    height: 15px;
    accent-color: var(--primary);
    cursor: default;
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
</style>

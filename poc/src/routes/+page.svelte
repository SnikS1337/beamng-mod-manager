<script lang="ts">
  import { onMount } from "svelte";
  import {
    Check,
    Cloud,
    Download,
    Package,
    RefreshCw,
    Search,
    Settings,
    X,
  } from "@lucide/svelte";
  import { Badge } from "$lib/components/ui/badge";
  import { Button } from "$lib/components/ui/button";
  import { Switch } from "$lib/components/ui/switch";
  import Background from "$lib/poc/Background.svelte";
  import ModGrid from "$lib/poc/ModGrid.svelte";
  import Titlebar from "$lib/poc/Titlebar.svelte";
  import { getEnvInfo, loadLibrary, openModlandProbe } from "$lib/poc/api";
  import type { EnvInfo, ModEntry, ModSource } from "$lib/poc/api";

  type Section = "mods" | "updates" | "search" | "settings";
  type Filter = "all" | "enabled" | "updates" | "disabled";

  const nav = [
    { id: "mods", label: "Моды", icon: Package },
    { id: "updates", label: "Обновления", icon: Download },
    { id: "search", label: "Поиск", icon: Search },
    { id: "settings", label: "Настройки", icon: Settings },
  ] as const;

  const filterItems = [
    { id: "all", label: "Все" },
    { id: "enabled", label: "Включённые" },
    { id: "updates", label: "Обновления" },
    { id: "disabled", label: "Выключенные" },
  ] as const;

  const SOURCE_LABEL: Record<ModSource, string> = {
    repo: "Репозиторий",
    modland: "Modland",
    stmods: "STMods",
    modsgaming: "ModsGaming",
  };

  let section = $state<Section>("mods");
  let filter = $state<Filter>("all");
  let query = $state("");

  let count = $state(5000);
  let mods = $state<ModEntry[]>([]);
  let selectedId = $state<number | null>(null);
  let loading = $state(false);
  let loadMs = $state(0);
  let reloadVersion = $state(0);
  let backend = $state<"tauri" | "mock">("mock");
  let env = $state<EnvInfo | null>(null);
  let error = $state<string | null>(null);

  let dark = $state(true);
  let glass = $state(true);
  let motion = $state(true);

  let fps = $state(0);
  let renderedCount = $state(0);

  let counts = $derived.by(() => {
    let enabled = 0;
    let updates = 0;
    for (const mod of mods) {
      if (mod.enabled) enabled += 1;
      if (mod.updateAvailable) updates += 1;
    }
    return {
      all: mods.length,
      enabled,
      updates,
      disabled: mods.length - enabled,
    };
  });

  let filtered = $derived.by(() => {
    const needle = query.trim().toLocaleLowerCase("ru");
    return mods.filter((mod) => {
      if (filter === "enabled" && !mod.enabled) return false;
      if (filter === "disabled" && mod.enabled) return false;
      if (filter === "updates" && !mod.updateAvailable) return false;
      if (!needle) return true;
      return (
        mod.title.toLocaleLowerCase("ru").includes(needle) ||
        mod.file.toLocaleLowerCase("ru").includes(needle)
      );
    });
  });

  let selected = $derived(mods.find((mod) => mod.id === selectedId) ?? null);

  onMount(() => {
    void reload(count);
    void getEnvInfo()
      .then((value) => (env = value))
      .catch(() => {});
  });

  $effect(() => {
    document.documentElement.classList.toggle("dark", dark);
  });

  $effect(() => {
    let raf = 0;
    let frames = 0;
    let last = performance.now();

    const tick = (now: number) => {
      frames += 1;
      if (now - last >= 500) {
        fps = Math.round((frames * 1000) / (now - last));
        frames = 0;
        last = now;
      }
      raf = requestAnimationFrame(tick);
    };

    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  });

  async function reload(next = count) {
    loading = true;
    error = null;

    try {
      const result = await loadLibrary(next);
      mods = result.items;
      backend = result.backend;
      loadMs = Math.round(result.ms);
      count = next;
      if (!mods.some((mod) => mod.id === selectedId)) {
        selectedId = mods[0]?.id ?? null;
      }
      reloadVersion += 1;
    } catch (err) {
      error = err instanceof Error ? err.message : String(err);
    } finally {
      loading = false;
    }
  }

  function setCount(next: number) {
    if (next === count) return;
    void reload(next);
  }

  function toggle(id: number) {
    const target = mods.find((mod) => mod.id === id);
    if (target) target.enabled = !target.enabled;
  }

  function toggleSelected() {
    if (selected) toggle(selected.id);
  }
</script>

<svelte:head>
  <title>ModManager BeamNG — PoC</title>
</svelte:head>

<div class="app" class:no-glass={!glass} class:no-motion={!motion}>
  <Background />
  <div class="noise"></div>
  <Titlebar />

  <div class="workspace">
    <aside class="rail glass liquid-edge">
      <div class="rail-items">
        {#each nav as item (item.id)}
          {@const Icon = item.icon}
          <button
            type="button"
            class="nav"
            class:active={section === item.id}
            title={item.label}
            aria-pressed={section === item.id}
            onclick={() => (section = item.id)}
          >
            <Icon size={20} strokeWidth={1.6} />
            <span>{item.label}</span>
          </button>
        {/each}
      </div>
      <div class="rail-foot">
        <Badge variant="outline">PoC</Badge>
      </div>
    </aside>

    <header class="island glass liquid-edge">
      <div class="search">
        <Search size={15} strokeWidth={1.8} />
        <input bind:value={query} placeholder="Найти мод" aria-label="Найти мод" />
        {#if query}
          <button type="button" class="clear" aria-label="Очистить" onclick={() => (query = "")}>
            <X size={13} strokeWidth={2} />
          </button>
        {/if}
      </div>

      <div class="filters">
        {#each filterItems as item (item.id)}
          <button
            type="button"
            class="filter"
            class:active={filter === item.id}
            aria-pressed={filter === item.id}
            onclick={() => (filter = item.id)}
          >
            {item.label}
            <span>{counts[item.id].toLocaleString("ru-RU")}</span>
          </button>
        {/each}
      </div>

      <Button variant="secondary" size="sm" disabled={loading} onclick={() => void reload(count)}>
        <RefreshCw size={14} class={loading ? "spin" : ""} />
        Перезагрузить
      </Button>
    </header>

    <main class="main">
      {#if section === "mods"}
        <ModGrid
          items={filtered}
          selectedId={selectedId}
          onselect={(id) => (selectedId = id)}
          ontoggle={toggle}
          bind:renderedCount
          resetKey={`${filter}|${query}|${reloadVersion}`}
        />
      {:else}
        <div class="placeholder glass liquid-edge">
          <p>{nav.find((item) => item.id === section)?.label}</p>
          <span>Раздел будет перенесён на следующем этапе. Сейчас PoC проверяет «Моды».</span>
        </div>
      {/if}
    </main>

    <aside class="inspector glass liquid-edge scroll-area">
      <div class="poc-head">
        <div>
          <p class="eyebrow">Проверка PoC</p>
          <h2>Скелет окна</h2>
        </div>
        <Badge variant={backend === "tauri" ? "default" : "secondary"}>
          {backend === "tauri" ? "Rust IPC" : "браузер"}
        </Badge>
      </div>

      <div class="metrics">
        <div class="metric"><span>IPC</span><b>{loadMs} мс</b></div>
        <div class="metric">
          <span>Карточек в DOM</span>
          <b>{renderedCount} / {filtered.length.toLocaleString("ru-RU")}</b>
        </div>
        <div class="metric">
          <span>FPS</span>
          <b class:good={fps >= 50} class:mid={fps >= 35 && fps < 50} class:bad={fps < 35}>{fps}</b>
        </div>
        <div class="metric"><span>Всего модов</span><b>{mods.length.toLocaleString("ru-RU")}</b></div>
      </div>

      {#if env}
        <p class="env">
          {env.backend === "tauri" ? "Tauri" : "Web"} {env.appVersion} · {env.os}/{env.arch}
        </p>
      {/if}

      {#if error}
        <div class="error">{error}</div>
      {/if}

      <div class="section">
        <h3>Библиотека</h3>
        <div class="counts">
          {#each [500, 5000, 20000] as size (size)}
            <Button
              variant={count === size ? "default" : "ghost"}
              size="sm"
              onclick={() => setCount(size)}
            >
              {size.toLocaleString("ru-RU")}
            </Button>
          {/each}
        </div>
        <p class="hint">5 000 — рабочий объём, 20 000 — проверка запаса.</p>
      </div>

      <div class="section">
        <h3>Настройки PoC</h3>
        <div class="row"><span>Анимации</span><Switch bind:checked={motion} /></div>
        <div class="row"><span>Жидкое стекло</span><Switch bind:checked={glass} /></div>
        <div class="row"><span>Тёмная тема</span><Switch bind:checked={dark} /></div>
      </div>

      <div class="section">
        <h3>Modland / Cloudflare</h3>
        <p class="hint">
          Откроется отдельное окно WebView. Проверку «вы человек?» проходите вручную.
        </p>
        <Button variant="outline" size="sm" onclick={() => void openModlandProbe()}>
          <Cloud size={14} />
          Открыть окно Modland
        </Button>
      </div>

      {#if selected}
        <div class="section">
          <h3>Выбранный мод</h3>
          <p class="selected-title">{selected.title}</p>
          <p class="mono">{selected.file}</p>
          <div class="kv"><span>Версия</span><b>v{selected.version}</b></div>
          <div class="kv"><span>Источник</span><b>{SOURCE_LABEL[selected.source]}</b></div>
          <div class="kv"><span>Размер</span><b>{selected.sizeMb} МБ</b></div>
          <div class="kv"><span>Обновление</span><b>{selected.updateAvailable ? "есть" : "нет"}</b></div>
          <div class="row"><span>Включён</span><Switch checked={selected.enabled} onCheckedChange={toggleSelected} /></div>
        </div>
      {/if}

      <div class="section risks">
        <h3>Уже проверяет PoC</h3>
        <ul>
          <li><Check size={12} /> окно без нативной рамки</li>
          <li><Check size={12} /> CSS-стекло и анимированный фон</li>
          <li><Check size={12} /> виртуализация 5 000 карточек</li>
          <li><Check size={12} /> IPC Rust ↔ Svelte</li>
          <li><Check size={12} /> shadcn-svelte + Tailwind 4</li>
        </ul>
        <h3>Не входит в скелет</h3>
        <ul class="open">
          <li>самообновление и откат</li>
          <li>RAR/7z и установка архивов</li>
          <li>общие куки Modland</li>
          <li>CI, снимки, релизы</li>
        </ul>
      </div>
    </aside>
  </div>
</div>

<style>
  .app {
    position: relative;
    height: 100vh;
    overflow: hidden;
  }

  .workspace {
    position: absolute;
    inset: 44px 0 0;
    z-index: 10;
    display: grid;
    grid-template-columns: 86px minmax(0, 1fr) 330px;
    grid-template-rows: 58px minmax(0, 1fr);
    gap: 12px;
    padding: 2px 14px 14px;
  }

  .rail {
    grid-row: 1 / 3;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 10px 7px;
    border-radius: 24px;
  }

  .rail-items {
    display: flex;
    flex-direction: column;
    gap: 5px;
    width: 100%;
  }

  .rail-foot {
    margin-top: auto;
    padding-bottom: 4px;
    opacity: 0.65;
  }

  .nav {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    width: 100%;
    height: 58px;
    border: 1px solid transparent;
    border-radius: 18px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 9.5px;
    cursor: default;
    transition:
      background 180ms ease,
      color 180ms ease,
      border-color 180ms ease;
  }

  .nav span {
    max-width: 100%;
    padding: 0 2px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .nav:hover {
    background: color-mix(in oklab, var(--foreground) 7%, transparent);
    color: var(--foreground);
  }

  .nav.active {
    border-color: var(--glass-border);
    background: var(--glass-fill-strong);
    color: var(--foreground);
    box-shadow: inset 0 1px 0 var(--glass-highlight);
  }

  .island {
    grid-column: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
    padding: 8px 10px;
    border-radius: 20px;
  }

  .search {
    display: flex;
    flex: 0 1 300px;
    align-items: center;
    gap: 7px;
    height: 38px;
    min-width: 170px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: color-mix(in oklab, var(--foreground) 5%, transparent);
    color: var(--muted-foreground);
  }

  .search input {
    width: 100%;
    border: 0;
    outline: 0;
    background: transparent;
    color: var(--foreground);
    font-size: 12.5px;
  }

  .search input::placeholder {
    color: var(--muted-foreground);
  }

  .clear {
    display: grid;
    place-items: center;
    padding: 2px;
    border: 0;
    background: transparent;
    color: var(--muted-foreground);
    cursor: default;
  }

  .filters {
    display: flex;
    flex: 1;
    gap: 2px;
    min-width: 0;
    overflow: hidden;
  }

  .filter {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 9px;
    border: 1px solid transparent;
    border-radius: 10px;
    background: transparent;
    color: var(--muted-foreground);
    font-size: 11px;
    white-space: nowrap;
    cursor: default;
    transition:
      background 160ms ease,
      color 160ms ease;
  }

  .filter span {
    padding: 1px 5px;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
    font-size: 9.5px;
    font-variant-numeric: tabular-nums;
  }

  .filter:hover {
    background: color-mix(in oklab, var(--foreground) 6%, transparent);
    color: var(--foreground);
  }

  .filter.active {
    border-color: var(--glass-border);
    background: var(--glass-fill-strong);
    color: var(--foreground);
  }

  .main {
    grid-column: 2;
    min-width: 0;
    min-height: 0;
  }

  .placeholder {
    display: grid;
    place-content: center;
    gap: 4px;
    height: 100%;
    padding: 30px;
    border-radius: 24px;
    color: var(--muted-foreground);
    text-align: center;
  }

  .placeholder p {
    margin: 0;
    color: var(--foreground);
    font-size: 16px;
    font-weight: 600;
  }

  .placeholder span {
    max-width: 360px;
    font-size: 11.5px;
    line-height: 1.5;
  }

  .inspector {
    grid-column: 3;
    grid-row: 1 / 3;
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 15px;
    border-radius: 24px;
    overflow-y: auto;
  }

  .poc-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
  }

  .eyebrow {
    margin: 0 0 2px;
    color: var(--muted-foreground);
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  .poc-head h2 {
    margin: 0;
    font-size: 16px;
    font-weight: 650;
    letter-spacing: -0.01em;
  }

  .metrics {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 7px;
  }

  .metric {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding: 8px 9px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: color-mix(in oklab, var(--foreground) 4%, transparent);
  }

  .metric span {
    color: var(--muted-foreground);
    font-size: 9.5px;
  }

  .metric b {
    font-size: 14px;
    font-weight: 650;
    font-variant-numeric: tabular-nums;
  }

  .metric b.good {
    color: oklch(0.78 0.16 150);
  }

  .metric b.mid {
    color: oklch(0.82 0.15 85);
  }

  .metric b.bad {
    color: oklch(0.7 0.19 25);
  }

  .env {
    margin: -4px 0 0;
    color: var(--muted-foreground);
    font-family: "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
    font-size: 9.5px;
  }

  .error {
    padding: 8px 10px;
    border: 1px solid oklch(0.6 0.2 25 / 0.5);
    border-radius: 10px;
    background: oklch(0.6 0.2 25 / 0.12);
    color: oklch(0.8 0.13 25);
    font-size: 11px;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .section:first-of-type {
    padding-top: 0;
    border-top: 0;
  }

  .section h3 {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 9.5px;
    font-weight: 650;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .counts {
    display: flex;
    gap: 4px;
  }

  .hint {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 10px;
    line-height: 1.45;
  }

  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    min-height: 30px;
    gap: 10px;
    font-size: 11.5px;
  }

  .selected-title {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
  }

  .mono {
    margin: -3px 0 2px;
    overflow: hidden;
    color: var(--muted-foreground);
    font-family: "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
    font-size: 9.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .kv {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .kv b {
    color: var(--foreground);
    font-weight: 500;
  }

  .risks ul {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding: 0;
    color: var(--muted-foreground);
    font-size: 10px;
    list-style: none;
  }

  .risks li {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .risks li :global(svg) {
    color: oklch(0.78 0.16 150);
  }

  .risks ul.open li {
    opacity: 0.6;
  }

  .risks ul.open li::before {
    content: "—";
    color: var(--muted-foreground);
  }

  @media (max-width: 1250px) {
    .filter span {
      display: none;
    }
  }

  @media (max-width: 1180px) {
    .workspace {
      grid-template-columns: 80px minmax(0, 1fr) 300px;
    }
  }

  @media (max-width: 1020px) {
    .workspace {
      grid-template-columns: 76px minmax(0, 1fr);
    }

    .inspector {
      display: none;
    }
  }
</style>

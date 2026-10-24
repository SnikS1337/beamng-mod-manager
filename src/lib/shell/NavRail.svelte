<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { LayoutGrid, RefreshCw, Search, SlidersVertical } from "@lucide/svelte";
  import { page } from "$app/state";
  import { updatesBadge } from "$lib/shell/updatesBadge.svelte";

  // Как в legacy: моды (четыре плитки), обновления, поиск и — под чертой — настройки.
  const nav = [
    { href: "/", label: "Моды", icon: LayoutGrid },
    { href: "/updates", label: "Обновления", icon: RefreshCw },
    { href: "/search", label: "Поиск", icon: Search },
  ] as const;

  let version = $state("");

  onMount(() => {
    updatesBadge.start();
    void getVersion()
      .then((value) => (version = value))
      .catch(() => {});
  });

  const settingsActive = $derived(page.url.pathname === "/settings");
</script>

<aside class="rail glass liquid-edge">
  <nav>
    {#each nav as item (item.href)}
      {@const Icon = item.icon}
      {@const active = page.url.pathname === item.href}
      <a
        class="nav"
        class:active
        href={item.href}
        aria-current={active ? "page" : undefined}
        aria-label={item.href === "/updates" && updatesBadge.available > 0
          ? `Обновления, ${updatesBadge.available} ждут обновления`
          : item.label}
      >
        <span class="icon">
          <Icon size={22} strokeWidth={1.7} />
          {#if item.href === "/updates" && updatesBadge.available > 0}
            <span class="badge" aria-hidden="true">
              {updatesBadge.available > 99 ? "99+" : updatesBadge.available}
            </span>
          {/if}
        </span>
        <span class="label">{item.label}</span>
      </a>
    {/each}
    <span class="line" aria-hidden="true"></span>
    <a
      class="nav"
      class:active={settingsActive}
      href="/settings"
      aria-current={settingsActive ? "page" : undefined}
    >
      <span class="icon"><SlidersVertical size={22} strokeWidth={1.7} /></span>
      <span class="label">Настройки</span>
    </a>
  </nav>
  {#if version}
    <span class="version" title="Версия программы">{version}</span>
  {/if}
</aside>

<style>
  .rail {
    align-self: start;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    width: 84px;
    padding: 8px;
    border-radius: 42px;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .nav {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    height: 64px;
    border-radius: 32px;
    color: color-mix(in oklab, var(--foreground) 72%, transparent);
    font-size: 11px;
    font-weight: 500;
    line-height: 1.1;
    text-align: center;
    text-decoration: none;
    transition:
      background 220ms ease,
      color 220ms ease,
      box-shadow 220ms ease;
  }

  .nav:hover {
    background: color-mix(in oklab, var(--foreground) 7%, transparent);
    color: var(--foreground);
  }

  /* Выбор — тихая светлая плашка с бликом по верхней кромке (капля «в покое», legacy). */
  .nav.active {
    background: var(--drop-fill);
    color: var(--foreground);
    font-weight: 600;
    box-shadow:
      inset 0 1px 0 var(--glass-highlight),
      inset 0 0 0 1px var(--glass-border),
      0 6px 18px var(--drop-glow);
  }

  .nav:focus-visible {
    outline: 3px solid var(--ring);
    outline-offset: 1px;
  }

  .label {
    max-width: 68px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .icon {
    position: relative;
    display: inline-flex;
  }

  .badge {
    position: absolute;
    top: -6px;
    right: -12px;
    min-width: 18px;
    height: 18px;
    padding: 0 5px;
    border-radius: 999px;
    background: var(--foreground);
    color: var(--background);
    font-size: 11px;
    font-weight: 700;
    line-height: 18px;
    text-align: center;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.3);
  }

  .line {
    height: 1px;
    margin: 4px 14px;
    background: color-mix(in oklab, var(--foreground) 14%, transparent);
  }

  .version {
    align-self: center;
    margin-bottom: 2px;
    color: color-mix(in oklab, var(--foreground) 45%, transparent);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }
</style>

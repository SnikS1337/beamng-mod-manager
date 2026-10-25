<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import Garage from "phosphor-svelte/lib/Garage";
  import ArrowsClockwise from "phosphor-svelte/lib/ArrowsClockwise";
  import MagnifyingGlass from "phosphor-svelte/lib/MagnifyingGlass";
  import SlidersHorizontal from "phosphor-svelte/lib/SlidersHorizontal";
  import { page } from "$app/state";
  import { updatesBadge } from "#lib/shell/updatesBadge.svelte.js";

  const nav = [
    { href: "/", label: "Гараж", icon: Garage },
    { href: "/updates", label: "Обновления", icon: ArrowsClockwise },
    { href: "/search", label: "Поиск", icon: MagnifyingGlass },
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

<aside class="rail" data-tauri-drag-region>
  <div class="logo" aria-hidden="true" data-tauri-drag-region>BM</div>
  <nav>
    {#each nav as item (item.href)}
      {@const Icon = item.icon}
      {@const active = page.url.pathname === item.href}
      <a class="nav" class:active href={item.href} aria-current={active ? "page" : undefined}>
        <span class="icon">
          <Icon size={23} weight={active ? "fill" : "regular"} />
          {#if item.href === "/updates" && updatesBadge.available > 0}
            <span class="badge" aria-label="ждут обновления: {updatesBadge.available}">
              {updatesBadge.available > 99 ? "99+" : updatesBadge.available}
            </span>
          {/if}
        </span>
        <span class="label">{item.label}</span>
      </a>
    {/each}
  </nav>
  <div class="spacer" data-tauri-drag-region></div>
  <a
    class="nav"
    class:active={settingsActive}
    href="/settings"
    aria-current={settingsActive ? "page" : undefined}
  >
    <span class="icon"><SlidersHorizontal size={23} weight={settingsActive ? "fill" : "regular"} /></span>
    <span class="label">Настройки</span>
  </a>
  {#if version}
    <span class="version mono" title="Версия программы">{version}</span>
  {/if}
</aside>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    width: 76px;
    height: 100%;
    padding: 14px 0 10px;
    box-sizing: border-box;
    border-right: 1px solid var(--border);
    background: var(--surface);
  }

  .logo {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    margin-bottom: 14px;
    border-radius: 10px;
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
    font-family: var(--font-display);
    font-size: 17px;
    font-weight: 700;
    letter-spacing: 0.02em;
  }

  nav {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .spacer {
    flex: 1;
    align-self: stretch;
  }

  .nav {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    width: 64px;
    height: 58px;
    border-radius: 10px;
    color: var(--muted-foreground);
    font-size: 10.5px;
    font-weight: 600;
    text-decoration: none;
    transition:
      background-color var(--dur-2) var(--ease-out),
      color var(--dur-2) var(--ease-out);
  }

  .nav::before {
    content: "";
    position: absolute;
    left: -6px;
    top: 50%;
    width: 3px;
    height: 0;
    border-radius: 2px;
    background: var(--accent-brand);
    transform: translateY(-50%);
    transition: height var(--dur-3) var(--ease-out);
  }

  .nav:hover {
    background: var(--surface-2);
    color: var(--foreground);
  }

  .nav.active {
    background: var(--surface-3);
    color: var(--foreground);
  }

  .nav.active::before {
    height: 26px;
  }

  .nav.active .icon {
    color: var(--accent-brand-text);
  }

  .label {
    max-width: 64px;
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
    top: -5px;
    right: -11px;
    min-width: 17px;
    height: 17px;
    padding: 0 4px;
    box-sizing: border-box;
    border-radius: 999px;
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
    font-size: 10.5px;
    font-weight: 800;
    line-height: 17px;
    text-align: center;
    animation: pop-in var(--dur-3) var(--ease-out) both;
  }

  .version {
    margin-top: 6px;
    color: var(--faint-foreground);
    font-size: 10px;
  }
</style>

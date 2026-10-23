<script lang="ts">
  import { Download, Package, Search, Settings } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { updatesBadge } from "$lib/shell/updatesBadge.svelte";

  onMount(() => updatesBadge.start());

  const nav = [
    { href: "/", label: "Моды", icon: Package },
    { href: "/updates", label: "Обновления", icon: Download },
    { href: "/search", label: "Поиск", icon: Search },
    { href: "/settings", label: "Настройки", icon: Settings },
  ] as const;
</script>

<aside class="rail glass liquid-edge">
  <nav>
    {#each nav as item (item.href)}
      {@const Icon = item.icon}
      {@const active = page.url.pathname === item.href}
      <a class="nav" class:active href={item.href} aria-current={active ? "page" : undefined}>
        <span class="icon">
          <Icon size={20} strokeWidth={1.6} />
          {#if item.href === "/updates" && updatesBadge.available > 0}
            <span class="badge" aria-label="ждут обновления: {updatesBadge.available}">
              {updatesBadge.available > 99 ? "99+" : updatesBadge.available}
            </span>
          {/if}
        </span>
        <span>{item.label}</span>
      </a>
    {/each}
  </nav>
</aside>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    border-radius: 20px;
    padding: 10px 8px;
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
    gap: 4px;
    padding: 9px 4px;
    border-radius: 14px;
    color: var(--muted-foreground);
    font-size: 9.5px;
    line-height: 1.15;
    text-align: center;
    text-decoration: none;
    transition:
      background 160ms ease,
      color 160ms ease;
  }

  .nav:hover,
  .nav.active {
    background: color-mix(in oklab, var(--foreground) 10%, transparent);
    color: var(--foreground);
  }

  .nav.active {
    font-weight: 600;
  }

  .icon {
    position: relative;
    display: inline-flex;
  }

  .badge {
    position: absolute;
    top: -7px;
    right: -11px;
    min-width: 17px;
    height: 17px;
    padding: 0 4px;
    border-radius: 999px;
    background: var(--foreground);
    color: var(--background);
    font-size: 10px;
    font-weight: 700;
    line-height: 17px;
    text-align: center;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.3);
  }
</style>

<script lang="ts">
  import { Download, Package, Search, Settings } from "@lucide/svelte";
  import { page } from "$app/state";

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
        <Icon size={20} strokeWidth={1.6} />
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
</style>

<script lang="ts">
  import { onMount } from "svelte";
  import { onNavigate } from "$app/navigation";
  import { IconContext } from "phosphor-svelte";
  import { canFlow, motion } from "#lib/shell/motion.svelte.js";
  import IncomingHost from "#lib/features/incoming/IncomingHost.svelte";
  import CommandPalette from "#lib/shell/CommandPalette.svelte";
  import Background from "#lib/shell/Background.svelte";
  import NavRail from "#lib/shell/NavRail.svelte";
  import Titlebar from "#lib/shell/Titlebar.svelte";
  import ToastHost from "#lib/shell/ToastHost.svelte";
  import WelcomeTour from "#lib/shell/WelcomeTour.svelte";
  import { appearance } from "#lib/shell/appearance.svelte.js";

  let { children } = $props();
  let welcome = $state(false);

  onMount(() => {
    const uninstall = motion.install();
    void (async () => {
      const loaded = appearance.settings ?? (await appearance.load());
      welcome = loaded ? !loaded.firstRunDone : false;
    })();
    return uninstall;
  });

  // Смена раздела — одним движением: старый уходит, новый проявляется, полоска
  // в меню переезжает к новому пункту.
  onNavigate((navigation) => {
    if (!canFlow(document)) return;
    if (navigation.from?.url.pathname === navigation.to?.url.pathname) return;
    return new Promise<void>((resolve) => {
      void motion.run(async () => {
        resolve();
        await navigation.complete;
      }, "flow-route");
    });
  });

  async function closeWelcome() {
    welcome = false;
    const current = appearance.settings;
    if (!current) return;
    try {
      await appearance.save({ ...current, firstRunDone: true });
    } catch {
      // Настройки не сохранились — окно просто закроется до следующего запуска.
    }
  }
</script>

<IconContext values={{ "aria-hidden": "true" }}>
  <div class="app">
    <Background />
    <Titlebar />

    <div class="workspace">
      <NavRail />
      <main class="content scroll-area">
        {@render children()}
      </main>
    </div>

    <IncomingHost />
    <CommandPalette />
    <ToastHost />
    <WelcomeTour open={welcome} onClose={() => void closeWelcome()} />
  </div>
</IconContext>

<style>
  .app {
    position: relative;
    height: 100vh;
    overflow: hidden;
  }

  .workspace {
    position: relative;
    z-index: 10;
    display: grid;
    grid-template-columns: 76px minmax(0, 1fr);
    height: 100vh;
  }

  .content {
    min-width: 0;
    overflow: auto;
    view-transition-name: page;
  }
</style>

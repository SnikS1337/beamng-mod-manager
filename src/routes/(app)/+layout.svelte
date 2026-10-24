<script lang="ts">
  import { onMount } from "svelte";
  import IncomingHost from "#lib/features/incoming/IncomingHost.svelte";
  import Background from "#lib/shell/Background.svelte";
  import NavRail from "#lib/shell/NavRail.svelte";
  import Titlebar from "#lib/shell/Titlebar.svelte";
  import ToastHost from "#lib/shell/ToastHost.svelte";
  import WelcomeTour from "#lib/shell/WelcomeTour.svelte";
  import { appearance } from "#lib/shell/appearance.svelte.js";

  let { children } = $props();
  let welcome = $state(false);

  onMount(() => {
    void (async () => {
      const loaded = appearance.settings ?? (await appearance.load());
      welcome = loaded ? !loaded.firstRunDone : false;
    })();
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

<div class="app">
  <Background />
  <div class="noise"></div>
  <Titlebar />

  <div class="workspace">
    <NavRail />
    <main class="content scroll-area">
      {@render children()}
    </main>
  </div>

  <IncomingHost />
  <ToastHost />
  <WelcomeTour open={welcome} onClose={() => void closeWelcome()} />
</div>

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
    grid-template-columns: 84px minmax(0, 1fr);
    gap: 16px;
    height: 100vh;
    padding: 16px;
  }

  .content {
    min-width: 0;
    overflow: auto;
    border-radius: 20px;
  }

  @media (max-width: 1020px) {
    .workspace {
      grid-template-columns: 84px minmax(0, 1fr);
      gap: 12px;
    }
  }
</style>

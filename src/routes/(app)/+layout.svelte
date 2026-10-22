<script lang="ts">
  import IncomingHost from "$lib/features/incoming/IncomingHost.svelte";
  import Background from "$lib/shell/Background.svelte";
  import NavRail from "$lib/shell/NavRail.svelte";
  import Titlebar from "$lib/shell/Titlebar.svelte";
  import ToastHost from "$lib/shell/ToastHost.svelte";

  let { children } = $props();
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
    grid-template-columns: 96px minmax(0, 1fr);
    gap: 12px;
    height: calc(100vh - 44px);
    margin-top: 44px;
    padding: 0 12px 12px;
  }

  .content {
    min-width: 0;
    overflow: auto;
    border-radius: 20px;
  }

  @media (max-width: 1020px) {
    .workspace {
      grid-template-columns: 76px minmax(0, 1fr);
    }
  }
</style>

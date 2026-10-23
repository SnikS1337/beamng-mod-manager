<script lang="ts">
  import { appearance } from "$lib/shell/appearance.svelte";

  let tiltX = $state(0);
  let tiltY = $state(0);

  const fireflies = $derived(appearance.settings?.fireflies ?? true);
  const tilt = $derived(appearance.settings?.backgroundTilt ?? false);
  const motion = $derived(appearance.settings?.uiAnimations ?? true);
  // Флажок Windows «анимация элементов» движение не выключает: только настройки программы (0.3.11).
  const reducedMotion = false;

  const flies = Array.from({ length: 18 }, (_, index) => ({
    left: ((index * 37 + 11) % 96) + 2,
    top: ((index * 53 + 7) % 88) + 6,
    delay: (index % 9) * 0.9,
    duration: 9 + (index % 7) * 1.7,
    size: 2 + (index % 3),
  }));

  function move(event: MouseEvent) {
    if (!tilt || !motion || reducedMotion) {
      tiltX = 0;
      tiltY = 0;
      return;
    }
    tiltX = (event.clientX / window.innerWidth - 0.5) * 10;
    tiltY = (event.clientY / window.innerHeight - 0.5) * 8;
  }

  function reset() {
    tiltX = 0;
    tiltY = 0;
  }
</script>

<svelte:window onmousemove={move} onmouseleave={reset} />

<div class="bg" aria-hidden="true">
  <div class="blobs" style={`transform:translate3d(${tilt ? tiltX : 0}px, ${tilt ? tiltY : 0}px, 0)`}>
    <div class="bg-blob a"></div>
    <div class="bg-blob b"></div>
    <div class="bg-blob c"></div>
    <div class="bg-blob d"></div>
  </div>
  <div class="bg-grid"></div>
  {#if fireflies}
    <div class="flies">
      {#each flies as fly, index (index)}
        <span
          style={`left:${fly.left}%;top:${fly.top}%;width:${fly.size}px;height:${fly.size}px;animation-delay:-${fly.delay}s;animation-duration:${fly.duration}s`}
        ></span>
      {/each}
    </div>
  {/if}
</div>

<style>
  .bg {
    position: fixed;
    inset: 0;
    z-index: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .blobs {
    position: absolute;
    inset: 0;
    transition: transform 0.28s ease-out;
    will-change: transform;
  }

  .bg-blob {
    position: absolute;
    width: 52vw;
    height: 52vw;
    border-radius: 50%;
    filter: blur(72px);
    opacity: 0.34;
  }

  .a {
    top: -22vh;
    left: -12vw;
    background: var(--blob-a);
  }

  .b {
    top: 8vh;
    right: -18vw;
    background: var(--blob-b);
  }

  .c {
    bottom: -30vh;
    left: 18vw;
    background: var(--blob-c);
  }

  .d {
    bottom: -24vh;
    right: 4vw;
    width: 34vw;
    height: 34vw;
    background: var(--blob-d);
    opacity: 0.22;
  }

  .bg-grid {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(var(--grid-line) 1px, transparent 1px),
      linear-gradient(90deg, var(--grid-line) 1px, transparent 1px);
    background-size: 56px 56px;
    mask-image: radial-gradient(ellipse at 50% 40%, #000, transparent 78%);
    -webkit-mask-image: radial-gradient(ellipse at 50% 40%, #000, transparent 78%);
  }

  .flies {
    position: absolute;
    inset: 0;
  }

  .flies span {
    position: absolute;
    border-radius: 50%;
    background: color-mix(in oklab, var(--blob-b) 80%, white);
    box-shadow: 0 0 8px color-mix(in oklab, var(--blob-b) 70%, transparent);
    opacity: 0.4;
    animation: fly 12s ease-in-out infinite alternate;
  }

  :global(.no-motion) .flies span {
    animation: none;
    opacity: 0.35;
  }


  @keyframes fly {
    0% {
      transform: translate3d(0, 0, 0) scale(0.7);
      opacity: 0.15;
    }
    40% {
      opacity: 0.65;
    }
    100% {
      transform: translate3d(18px, -26px, 0) scale(1.2);
      opacity: 0.1;
    }
  }

</style>

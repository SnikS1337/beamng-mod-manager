<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import { appearance } from "#lib/shell/appearance.svelte.js";
  import { lightSpot } from "./backgroundSpot";

  // Тихий фон: графит, мягкий свет акцентного цвета и тонкая сетка гаража.
  // Свет стоит у раздела, который открыт, и плавно переезжает при смене раздела —
  // фон «ведёт» за собой. «Живой фон» добавляет медленное дыхание света; выключен —
  // свет неподвижен. Двигается только transform: без перерисовки, почти без нагрузки.
  const live = $derived(
    (appearance.settings?.animatedBackground ?? true) && (appearance.settings?.uiAnimations ?? true),
  );
  const spot = $derived(lightSpot(page.url.pathname));
  let paused = $state(false);

  onMount(() => {
    const sync = () => {
      paused = document.visibilityState === "hidden";
    };
    sync();
    document.addEventListener("visibilitychange", sync);
    return () => document.removeEventListener("visibilitychange", sync);
  });
</script>

<div class="bg" class:live class:paused aria-hidden="true">
  <div class="wash"></div>
  <div class="spot" style={`--x:${spot.x}%; --y:${spot.y}%; --power:${spot.power}`}>
    <div class="light warm"></div>
    <div class="light cool"></div>
  </div>
  <div class="grid"></div>
  <div class="lantern"></div>
  <div class="lantern alt"></div>
  <div class="grain"></div>
</div>

<style>
  .bg {
    position: fixed;
    inset: 0;
    z-index: 0;
    overflow: hidden;
    pointer-events: none;
    background: var(--background);
  }

  /* Цветная «погода» окна: три мягких пятна, без движения и перерисовки. */
  .wash {
    position: absolute;
    inset: 0;
    background:
      radial-gradient(
        58% 68% at 10% 6%,
        color-mix(in oklab, var(--backdrop-a) 27%, transparent),
        transparent 72%
      ),
      radial-gradient(
        48% 58% at 90% 14%,
        color-mix(in oklab, var(--backdrop-b) 23%, transparent),
        transparent 70%
      ),
      radial-gradient(
        55% 55% at 76% 88%,
        color-mix(in oklab, var(--backdrop-c) 12%, transparent),
        transparent 76%
      ),
      linear-gradient(
        135deg,
        color-mix(in oklab, var(--backdrop-c) 7%, transparent),
        transparent 62%
      );
    opacity: 0.95;
  }

  :global(html:not(.dark)) .wash {
    opacity: 0.88;
  }

  /* Точка света: у каждого раздела своя, переезд — секунду, с мягким торможением. */
  .spot {
    position: absolute;
    inset: 0;
    transform: translate3d(calc(var(--x) - 50%), calc(var(--y) - 50%), 0);
    opacity: var(--power);
    transition:
      transform 1.4s var(--ease-out),
      opacity 1.4s var(--ease-out);
    will-change: transform;
  }

  :global(.no-motion) .spot {
    transition: none;
    will-change: auto;
  }

  .light {
    position: absolute;
    border-radius: 50%;
  }

  .warm {
    left: 20%;
    top: -30%;
    width: 90%;
    height: 90%;
    background: radial-gradient(
      closest-side,
      color-mix(in oklab, var(--backdrop-a) 11%, transparent),
      color-mix(in oklab, var(--backdrop-a) 4%, transparent) 55%,
      transparent
    );
  }

  /* Слабый ответный свет снизу: без него тёмный низ окна выглядел провалом. */
  .cool {
    left: -25%;
    top: 55%;
    width: 75%;
    height: 75%;
    background: radial-gradient(
      closest-side,
      color-mix(in oklab, var(--backdrop-b) 5%, transparent),
      transparent
    );
  }

  /* Дыхание: два света ходят с разным шагом (41 и 59 с) — рисунок не повторяется
     заметно, движение не читается как «анимация по кругу». */
  .live .warm {
    animation: drift-warm 41s var(--ease-in-out) infinite alternate;
  }

  .live .cool {
    animation: drift-cool 59s var(--ease-in-out) infinite alternate;
  }

  /* Свёрнутое окно не должно греть процессор медленным светом. */
  .paused .light {
    animation-play-state: paused;
  }

  .grid {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(var(--grid-line) 1px, transparent 1px),
      linear-gradient(90deg, var(--grid-line) 1px, transparent 1px);
    background-size: 48px 48px;
    mask-image: radial-gradient(ellipse 90% 70% at 60% 0%, #000 0%, transparent 75%);
  }

  /* Цветной «фонарь» ходит по неподвижной сетке: двигается только свет,
     линии остаются на месте. */
  .lantern {
    position: absolute;
    left: 50%;
    top: 50%;
    width: min(78vw, 78vh);
    aspect-ratio: 1;
    margin-left: calc(min(78vw, 78vh) / -2);
    margin-top: calc(min(78vw, 78vh) / -2);
    border-radius: 50%;
    background: radial-gradient(
      circle,
      color-mix(in oklab, var(--backdrop-a) 20%, transparent) 0%,
      color-mix(in oklab, var(--backdrop-a) 7%, transparent) 42%,
      transparent 72%
    );
    opacity: 0;
    pointer-events: none;
    mix-blend-mode: screen;
    will-change: transform, opacity;
  }

  .lantern.alt {
    width: min(62vw, 62vh);
    margin-left: calc(min(62vw, 62vh) / -2);
    margin-top: calc(min(62vw, 62vh) / -2);
    background: radial-gradient(
      circle,
      color-mix(in oklab, var(--backdrop-b) 17%, transparent) 0%,
      color-mix(in oklab, var(--backdrop-b) 6%, transparent) 40%,
      transparent 70%
    );
  }

  :global(html:not(.dark)) .lantern {
    mix-blend-mode: multiply;
  }

  .live .lantern {
    opacity: 0.7;
    animation: lantern-roam 34s var(--ease-in-out) infinite alternate;
  }

  .live .lantern.alt {
    opacity: 0.5;
    animation: lantern-roam-alt 47s var(--ease-in-out) infinite alternate;
  }

  .paused .lantern {
    animation-play-state: paused;
  }

  :global(.no-motion) .lantern {
    animation: none;
    will-change: auto;
  }

  /* Мелкое зерно убирает полосы на плавных переходах света. Картинка неподвижна. */
  .grain {
    position: absolute;
    inset: 0;
    opacity: 0.035;
    background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='160' height='160'><filter id='n'><feTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2' stitchTiles='stitch'/></filter><rect width='100%' height='100%' filter='url(%23n)'/></svg>");
  }

  @keyframes drift-warm {
    0% {
      transform: translate3d(-5%, 0, 0) scale(1);
    }
    50% {
      transform: translate3d(3%, 4%, 0) scale(1.06);
    }
    100% {
      transform: translate3d(8%, -2%, 0) scale(0.97);
    }
  }

  @keyframes drift-cool {
    from {
      transform: translate3d(0, 0, 0) scale(1);
    }
    to {
      transform: translate3d(10%, -6%, 0) scale(1.1);
    }
  }

  @keyframes lantern-roam {
    0% {
      transform: translate3d(-22vw, -14vh, 0) scale(0.9);
    }
    25% {
      transform: translate3d(8vw, -6vh, 0) scale(1.08);
    }
    50% {
      transform: translate3d(24vw, 14vh, 0) scale(1);
    }
    75% {
      transform: translate3d(-4vw, 22vh, 0) scale(1.12);
    }
    100% {
      transform: translate3d(-20vw, 4vh, 0) scale(0.94);
    }
  }

  @keyframes lantern-roam-alt {
    0% {
      transform: translate3d(18vw, 10vh, 0) scale(1.05);
    }
    35% {
      transform: translate3d(-16vw, 18vh, 0) scale(0.92);
    }
    70% {
      transform: translate3d(-24vw, -10vh, 0) scale(1.1);
    }
    100% {
      transform: translate3d(10vw, -20vh, 0) scale(0.96);
    }
  }
</style>

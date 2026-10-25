<script lang="ts">
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
</script>

<div class="bg" class:live aria-hidden="true">
  <div class="spot" style={`--x:${spot.x}%; --y:${spot.y}%; --power:${spot.power}`}>
    <div class="light warm"></div>
    <div class="light cool"></div>
  </div>
  <div class="grid"></div>
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
      color-mix(in oklab, var(--accent-brand) 11%, transparent),
      color-mix(in oklab, var(--accent-brand) 4%, transparent) 55%,
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
      color-mix(in oklab, var(--accent-brand) 5%, transparent),
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

  .grid {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(var(--grid-line) 1px, transparent 1px),
      linear-gradient(90deg, var(--grid-line) 1px, transparent 1px);
    background-size: 48px 48px;
    mask-image: radial-gradient(ellipse 90% 70% at 60% 0%, #000 0%, transparent 75%);
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
</style>

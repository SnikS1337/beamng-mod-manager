<script lang="ts">
  import { appearance } from "#lib/shell/appearance.svelte.js";

  // Тихий фон: графит, мягкий свет акцентного цвета сверху и тонкая сетка гаража.
  // «Живой фон» в настройках медленно смещает свет; выключен — фон неподвижен.
  const drift = $derived(
    (appearance.settings?.animatedBackground ?? true) && (appearance.settings?.uiAnimations ?? true),
  );
</script>

<div class="bg" aria-hidden="true">
  <div class="light" class:drift></div>
  <div class="grid"></div>
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

  .light {
    position: absolute;
    inset: -30% -10% auto 20%;
    height: 80%;
    background: radial-gradient(
      closest-side,
      color-mix(in oklab, var(--accent-brand) 9%, transparent),
      transparent
    );
  }

  .light.drift {
    animation: drift 38s var(--ease-in-out) infinite alternate;
  }

  .grid {
    position: absolute;
    inset: 0;
    background-image:
      linear-gradient(var(--grid-line) 1px, transparent 1px),
      linear-gradient(90deg, var(--grid-line) 1px, transparent 1px);
    background-size: 48px 48px;
    mask-image: linear-gradient(to bottom, #000 0%, transparent 70%);
  }

  @keyframes drift {
    from {
      transform: translate3d(-6%, 0, 0);
    }
    to {
      transform: translate3d(8%, 6%, 0);
    }
  }
</style>

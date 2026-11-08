<script lang="ts">
  // Галочка, которая «дорисовывается»: кружок упруго появляется, потом штрих
  // проводится от начала к концу. Играет, когда элемент появляется; в режиме
  // «Быстро» сразу стоит готовой (общее правило обрывает анимацию на последнем кадре).
  let { size = 14, ring = true, delay = 0 }: { size?: number; ring?: boolean; delay?: number } = $props();
</script>

<svg
  class="draw-check"
  class:with-ring={ring}
  width={size}
  height={size}
  viewBox="0 0 16 16"
  aria-hidden="true"
  style:--check-delay="{delay}ms"
>
  {#if ring}<circle cx="8" cy="8" r="7.25" />{/if}
  <path d="M4.6 8.3 7 10.6 11.6 5.6" pathLength="1" />
</svg>

<style>
  .draw-check {
    flex: none;
    overflow: visible;
  }

  circle {
    fill: currentColor;
    opacity: 0.18;
    transform-origin: 8px 8px;
    animation: check-ring 420ms var(--ease-snap) var(--check-delay) both;
  }

  path {
    fill: none;
    stroke: currentColor;
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: check-draw 380ms cubic-bezier(0.65, 0, 0.35, 1) calc(var(--check-delay) + 140ms) both;
  }

  .draw-check:not(.with-ring) path {
    animation-delay: var(--check-delay);
  }

  @keyframes check-ring {
    from {
      opacity: 0;
      transform: scale(0.4);
    }
  }

  @keyframes check-draw {
    to {
      stroke-dashoffset: 0;
    }
  }
</style>

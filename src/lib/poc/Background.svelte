<div class="bg" aria-hidden="true">
  <div class="bg-blob a"></div>
  <div class="bg-blob b"></div>
  <div class="bg-blob c"></div>
  <div class="bg-blob d"></div>
  <div class="bg-grid"></div>
</div>

<style>
  .bg {
    position: fixed;
    inset: 0;
    z-index: 0;
    overflow: hidden;
    pointer-events: none;
  }

  .bg-blob {
    position: absolute;
    width: 52vw;
    height: 52vw;
    border-radius: 50%;
    filter: blur(72px);
    opacity: 0.5;
    animation: drift 28s ease-in-out infinite alternate;
    will-change: transform;
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
    animation-duration: 34s;
    animation-delay: -7s;
  }

  .c {
    bottom: -30vh;
    left: 18vw;
    background: var(--blob-c);
    animation-duration: 31s;
    animation-delay: -13s;
  }

  .d {
    bottom: -24vh;
    right: 4vw;
    width: 34vw;
    height: 34vw;
    background: var(--blob-d);
    opacity: 0.32;
    animation-duration: 25s;
    animation-delay: -3s;
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

  @keyframes drift {
    0% {
      transform: translate3d(0, 0, 0) scale(1);
    }
    50% {
      transform: translate3d(6vw, 4vh, 0) scale(1.12);
    }
    100% {
      transform: translate3d(-4vw, 7vh, 0) scale(0.94);
    }
  }
</style>

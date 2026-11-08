<script lang="ts">
  import { siteMark } from "./siteMark";

  // Монограмма сайта. Название рядом пишет место, где значок стоит; сам значок —
  // для глаза, читалке экрана он не нужен.
  let { id, size = 18 }: { id: string; size?: number } = $props();
  const mark = $derived(siteMark(id));
</script>

<span
  class="site-mark"
  aria-hidden="true"
  style={`--mark:${mark.color}; --mark-size:${size}px`}
>{mark.letters}</span>

<style>
  .site-mark {
    display: inline-grid;
    flex: none;
    place-items: center;
    width: var(--mark-size);
    height: var(--mark-size);
    border-radius: calc(var(--mark-size) * 0.3);
    background: var(--mark);
    color: #fff;
    font-size: calc(var(--mark-size) * 0.46);
    font-weight: 800;
    letter-spacing: 0.02em;
    line-height: 1;
    box-shadow: inset 0 0 0 1px color-mix(in oklab, #fff 14%, transparent);
    user-select: none;
  }
</style>

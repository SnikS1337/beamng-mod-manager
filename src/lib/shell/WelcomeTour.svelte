<script lang="ts">
  import Archive from "phosphor-svelte/lib/Archive";
  import RefreshCw from "phosphor-svelte/lib/ArrowsClockwise";
  import Search from "phosphor-svelte/lib/MagnifyingGlass";
  import Sparkles from "phosphor-svelte/lib/Sparkle";
  import { trapFocus } from "./focus";

  let { open, onClose }: { open: boolean; onClose: () => void } = $props();

  function onkeydown(event: KeyboardEvent) {
    if (open && event.key === "Escape") {
      event.preventDefault();
      onClose();
    }
  }

  const points = [
    {
      icon: Archive,
      title: "Моды под контролем",
      text: "Карточки, включение и выключение, удаление с отменой — как в игре, только удобнее.",
    },
    {
      icon: RefreshCw,
      title: "Обновления — сами",
      text: "Программа сверяет версии с сайтами и показывает, что можно обновить.",
    },
    {
      icon: Search,
      title: "Поиск сразу везде",
      text: "Modland, STMods и ModsGaming; один мод с разных сайтов — одна карточка.",
    },
    {
      icon: Sparkles,
      title: "Свои архивы",
      text: "Перетащите zip, rar или 7z на окно, а скачанные архивы подхватятся из «Загрузок».",
    },
  ];
</script>

<svelte:window {onkeydown} />

{#if open}
  <div class="backdrop">
    <div
      class="tour glass"
      role="dialog"
      aria-modal="true"
      aria-labelledby="welcome-title"
      use:trapFocus={".start"}
    >
      <p class="eyebrow">Первый запуск</p>
      <h1 class="display" id="welcome-title">ModManager BeamNG</h1>
      <p class="lead">Всё для порядка в модах — в одном окне. Коротко о главном:</p>
      <div class="points">
        {#each points as point (point.title)}
          {@const Icon = point.icon}
          <div class="point">
            <span class="icon"><Icon size={17} /></span>
            <div>
              <h2>{point.title}</h2>
              <p>{point.text}</p>
            </div>
          </div>
        {/each}
      </div>
      <footer>
        <span>Это окно больше не появится.</span>
        <button class="start" type="button" onclick={onClose}>Начать</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    padding: 24px;
    background: var(--overlay);
    animation: fade-in var(--dur-2) var(--ease-out) both;
  }

  .tour {
    width: min(580px, calc(100vw - 48px));
    padding: 28px 30px 24px;
    border-radius: 14px;
    animation: welcome-in var(--dur-3) var(--ease-out) both;
  }

  .eyebrow {
    margin: 0 0 6px;
    color: var(--accent-brand-text);
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0;
    font-size: 32px;
    line-height: 1.05;
  }

  .lead {
    margin: 8px 0 18px;
    color: var(--muted-foreground);
    font-size: 14px;
    line-height: 1.5;
  }

  .points {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .point {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
  }

  .icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 34px;
    height: 34px;
    border-radius: 8px;
    background: var(--accent-brand-soft);
    color: var(--accent-brand-text);
  }

  h2 {
    margin: 0;
    font-size: 14.5px;
    font-weight: 800;
  }

  .point p {
    margin: 2px 0 0;
    color: var(--muted-foreground);
    font-size: 13px;
    line-height: 1.45;
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    margin-top: 20px;
  }

  footer span {
    color: var(--faint-foreground);
    font-size: 12.5px;
  }

  footer button {
    height: 38px;
    min-width: 130px;
    padding: 0 18px;
    border: 0;
    border-radius: 8px;
    background: var(--accent-brand);
    color: var(--accent-brand-contrast);
    font-size: 14px;
    font-weight: 800;
    cursor: default;
  }

  footer button:hover {
    background: var(--accent-brand-hover);
  }

  @keyframes welcome-in {
    from {
      opacity: 0;
      transform: translateY(10px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
</style>

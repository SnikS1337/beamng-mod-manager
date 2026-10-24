<script lang="ts">
  import { tick } from "svelte";
  import { Archive, RefreshCw, Search, Sparkles } from "@lucide/svelte";

  let { open, onClose }: { open: boolean; onClose: () => void } = $props();
  let dialog = $state<HTMLDivElement | null>(null);
  let startButton = $state<HTMLButtonElement | null>(null);

  $effect(() => {
    if (!open) return;
    void tick().then(() => startButton?.focus());
  });

  function onkeydown(event: KeyboardEvent) {
    if (event.key !== "Tab" || !dialog) return;
    const focusable = Array.from(
      dialog.querySelectorAll<HTMLElement>(
        'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])',
      ),
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
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
      title: "Обновления сами",
      text: "Программа сверяет версии с сайтами и показывает, что можно обновить.",
    },
    {
      icon: Search,
      title: "Поиск сразу везде",
      text: "Modland, STMods, ModsGaming и репозиторий BeamNG; один мод с разных сайтов — одна карточка.",
    },
    {
      icon: Sparkles,
      title: "Свои архивы",
      text: "Перетащите zip, rar или 7z на окно, а скачанные архивы подхватятся из «Загрузок».",
    },
  ];
</script>

{#if open}
  <div class="backdrop">
    <div
      class="tour glass liquid-edge"
      role="dialog"
      aria-modal="true"
      aria-labelledby="welcome-title"
      tabindex="-1"
      bind:this={dialog}
      onkeydown={onkeydown}
    >
      <p class="eyebrow">Первый запуск</p>
      <h1 id="welcome-title">ModManager BeamNG</h1>
      <p class="lead">Всё, чтобы держать моды в порядке, — в одном окне. Коротко о главном:</p>
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
        <button type="button" bind:this={startButton} onclick={onClose}>Начать</button>
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
    background: rgba(6, 10, 20, 0.66);
  }

  .tour {
    width: min(560px, calc(100vw - 48px));
    padding: 28px 30px 24px;
    border-radius: 28px;
    animation: welcome-in 0.32s ease-out;
  }

  .eyebrow {
    margin: 0 0 6px;
    color: var(--primary);
    font-size: 9.5px;
    font-weight: 700;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0;
    font-size: 25px;
    font-weight: 660;
    letter-spacing: -0.02em;
  }

  .lead {
    margin: 8px 0 18px;
    color: var(--muted-foreground);
    font-size: 11.5px;
    line-height: 1.5;
  }

  .points {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }

  .point {
    display: flex;
    gap: 11px;
    align-items: flex-start;
    padding: 10px 12px;
    border: 1px solid color-mix(in oklab, var(--foreground) 8%, transparent);
    border-radius: 14px;
    background: color-mix(in oklab, var(--foreground) 3%, transparent);
  }

  .icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 30px;
    height: 30px;
    border-radius: 10px;
    background: color-mix(in oklab, var(--primary) 14%, transparent);
    color: var(--primary);
  }

  h2 {
    margin: 0;
    font-size: 12.5px;
    font-weight: 620;
  }

  .point p {
    margin: 2px 0 0;
    color: var(--muted-foreground);
    font-size: 10px;
    line-height: 1.45;
  }

  footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    margin-top: 18px;
  }

  footer span {
    color: var(--muted-foreground);
    font-size: 9.5px;
  }

  footer button {
    height: 34px;
    min-width: 120px;
    padding: 0 18px;
    border: 0;
    border-radius: 999px;
    background: var(--primary);
    color: var(--primary-foreground);
    font-size: 11px;
    font-weight: 650;
    cursor: default;
  }

  @keyframes welcome-in {
    from {
      opacity: 0;
      transform: translateY(10px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }
</style>

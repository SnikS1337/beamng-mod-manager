<script lang="ts">
  import { Button } from "$lib/components/ui/button";
  import type { ModCardDto } from "$lib/ipc/library";
  import { formatDateTime, formatSize } from "./format";

  let {
    card,
    onRestore,
    onClose,
  }: {
    card: ModCardDto;
    onRestore: (card: ModCardDto) => void;
    onClose: () => void;
  } = $props();
</script>

<aside class="inspector glass liquid-edge">
  <div class="top">
    <span class="eyebrow">Удалённый мод</span>
    <button class="close" type="button" aria-label="Закрыть панель" onclick={onClose}>×</button>
  </div>

  <div class="hero">
    <span>В копиях</span>
  </div>

  <h2>{card.name}</h2>
  <p class="file">{card.fileName}</p>

  <div class="facts">
    <div><span>Размер</span><b>{formatSize(card.sizeBytes)}</b></div>
    <div><span>Удалён</span><b>{formatDateTime(card.savedAt)}</b></div>
    {#if card.copies > 1}
      <div><span>Копий</span><b>{card.copies}</b></div>
    {/if}
    {#if card.version}
      <div><span>Версия</span><b>{card.version}</b></div>
    {/if}
  </div>

  <p class="hint">Мод снова появится в папке, игра включит его сама.</p>
  <Button onclick={() => onRestore(card)}>Вернуть</Button>
</aside>

<style>
  .inspector {
    display: flex;
    flex-direction: column;
    gap: 8px;
    height: 100%;
    overflow-y: auto;
    border-radius: 20px;
    padding: 16px;
  }

  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .eyebrow {
    color: var(--muted-foreground);
    font-size: 9.5px;
    font-weight: 650;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .close {
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 999px;
    background: color-mix(in oklab, var(--foreground) 8%, transparent);
    color: var(--muted-foreground);
    font-size: 18px;
    line-height: 1;
    cursor: default;
  }

  .hero {
    display: grid;
    place-items: center;
    height: 132px;
    border-radius: 16px;
    background: linear-gradient(135deg, oklch(0.32 0.03 250), oklch(0.25 0.02 260));
  }

  .hero span {
    color: rgb(255 255 255 / 0.22);
    font-size: 18px;
    font-weight: 650;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h2 {
    margin: 4px 0 0;
    font-size: 20px;
    font-weight: 650;
  }

  .file {
    margin: 0 0 6px;
    overflow: hidden;
    color: var(--muted-foreground);
    font-family: "SFMono-Regular", ui-monospace, Menlo, Consolas, monospace;
    font-size: 9.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .facts {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 12px 0;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }

  .facts div {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    color: var(--muted-foreground);
    font-size: 10.5px;
  }

  .facts b {
    color: var(--foreground);
    font-weight: 500;
  }

  .hint {
    margin: 2px 0 4px;
    color: var(--muted-foreground);
    font-size: 10.5px;
    line-height: 1.4;
  }

  @media (max-width: 900px) {
    .inspector {
      display: none;
    }
  }
</style>

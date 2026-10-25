<script lang="ts">
  import type { ModCardDto } from "#lib/ipc/library.js";
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

<aside class="inspector scroll-area" aria-label="Удалённый мод">
  <div class="top">
    <span class="eyebrow">Удалённый мод</span>
    <button class="close" type="button" aria-label="Закрыть панель" onclick={onClose}>×</button>
  </div>

  <div class="hero">
    <span>В копиях</span>
  </div>

  <h2 class="display">{card.name}</h2>
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
  <button class="btn-primary" type="button" onclick={() => onRestore(card)}>Вернуть</button>
</aside>

<style>
  .inspector {
    display: flex;
    flex-direction: column;
    gap: 10px;
    height: 100%;
    box-sizing: border-box;
    overflow-y: auto;
    padding: 20px;
    border-left: 1px solid var(--border);
    background: var(--surface);
    animation: fade-in var(--dur-3) var(--ease-out) both;
  }

  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .eyebrow {
    color: var(--faint-foreground);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .close {
    width: 30px;
    height: 30px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--muted-foreground);
    font-size: 18px;
    line-height: 1;
    cursor: default;
  }

  .hero {
    display: grid;
    place-items: center;
    height: 140px;
    border-radius: 10px;
    background: repeating-linear-gradient(135deg, var(--surface-2) 0 12px, var(--surface-3) 12px 24px);
  }

  .hero span {
    color: var(--faint-foreground);
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  h2 {
    margin: 4px 0 0;
    font-size: 23px;
    line-height: 1.1;
  }

  .file {
    margin: 0 0 6px;
    overflow: hidden;
    color: var(--faint-foreground);
    font-size: 12.5px;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .facts {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: 10px;
  }

  .facts div {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 9px 12px;
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .facts div + div {
    border-top: 1px solid var(--border);
  }

  .facts b {
    color: var(--foreground);
    font-weight: 600;
  }

  .hint {
    margin: 2px 0 4px;
    color: var(--muted-foreground);
    font-size: 13px;
    line-height: 1.45;
  }
</style>

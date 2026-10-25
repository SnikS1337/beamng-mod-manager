<script lang="ts">
  import Power from "phosphor-svelte/lib/Power";
  import Trash from "phosphor-svelte/lib/Trash";
  import X from "phosphor-svelte/lib/X";
  import { plural } from "./format";
  import type { BulkPlan } from "./selection";

  let {
    count,
    plan,
    busy = false,
    onEnable,
    onDisable,
    onDelete,
    onClear,
  }: {
    count: number;
    plan: BulkPlan;
    busy?: boolean;
    onEnable: () => void;
    onDisable: () => void;
    onDelete: () => void;
    onClear: () => void;
  } = $props();
</script>

<aside class="bulk" aria-label="Выбранные моды">
  <div class="head">
    <h2 class="display">Выбрано {count} {plural(count, "мод", "мода", "модов")}</h2>
    <button class="close" type="button" aria-label="Снять выбор" onclick={onClear}>
      <X size={15} weight="bold" />
    </button>
  </div>
  <p class="hint">
    Ctrl — добавить или убрать мод, Shift — диапазон, Ctrl+A — все видимые, Esc — снять выбор.
  </p>

  <div class="actions">
    <button class="btn-primary" type="button" disabled={busy || plan.enable.length === 0} onclick={onEnable}>
      <Power size={16} weight="bold" />
      Включить {plan.enable.length > 0 ? plan.enable.length : ""}
    </button>
    <button class="btn-glass" type="button" disabled={busy || plan.disable.length === 0} onclick={onDisable}>
      <Power size={16} />
      Выключить {plan.disable.length > 0 ? plan.disable.length : ""}
    </button>
    <button class="btn-glass danger" type="button" disabled={busy || plan.remove.length === 0} onclick={onDelete}>
      <Trash size={16} />
      Удалить {plan.remove.length > 0 ? plan.remove.length : ""}
    </button>
  </div>
  {#if plan.remove.length < count && plan.remove.length > 0}
    <p class="hint">Моды из репозитория игры удаляются в самой игре — их здесь не тронем.</p>
  {/if}
  <p class="hint">Удалённые можно вернуть: кнопка «Отменить» или раздел «Удалённые».</p>
</aside>

<style>
  .bulk {
    display: flex;
    flex-direction: column;
    gap: 12px;
    height: 100%;
    box-sizing: border-box;
    padding: 20px;
    border-left: 1px solid var(--border);
    background: var(--surface);
    animation: fade-in var(--dur-3) var(--ease-out) both;
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
  }

  h2 {
    margin: 0;
    font-size: 22px;
  }

  .close {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--foreground);
    cursor: default;
  }

  .hint {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 12.5px;
    line-height: 1.45;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .actions button {
    justify-content: flex-start;
    height: 40px;
  }

  .danger {
    color: var(--danger);
  }
</style>

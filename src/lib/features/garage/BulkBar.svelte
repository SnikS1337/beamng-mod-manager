<script lang="ts">
  import Power from "phosphor-svelte/lib/Power";
  import Trash from "phosphor-svelte/lib/Trash";
  import ArrowCounterClockwise from "phosphor-svelte/lib/ArrowCounterClockwise";
  import X from "phosphor-svelte/lib/X";
  import { plural } from "./format";
  import { rollIn } from "#lib/shell/transitions.js";
  import type { BulkPlan } from "./selection";

  let {
    count,
    plan,
    busy = false,
    onEnable,
    onDisable,
    onDelete,
    onRestore,
    onClear,
  }: {
    count: number;
    plan: BulkPlan;
    busy?: boolean;
    onEnable: () => void;
    onDisable: () => void;
    onDelete: () => void;
    onRestore: () => void;
    onClear: () => void;
  } = $props();
</script>

<aside class="bulk" aria-label="Выбранные моды">
  <div class="head">
    <!-- Число выбранных «прокручивается», как счётчик: видно, что выбор изменился. -->
    <h2 class="display">
      Выбрано {#key count}<span class="count" in:rollIn>{count}</span>{/key}
      {plural(count, "мод", "мода", "модов")}
    </h2>
    <button class="close" type="button" aria-label="Снять выбор" onclick={onClear}>
      <X size={15} weight="bold" />
    </button>
  </div>
  <p class="hint">
    Ставьте галочки на карточках; Ctrl — добавить или убрать, Shift — диапазон, Ctrl+A — все видимые, Esc — снять выбор.
  </p>

  <div class="actions">
    <!-- Кнопки без работы не показываем: «Включить» у включённых только сбивает. -->
    {#if plan.enable.length > 0}
      <button class="btn-primary" type="button" disabled={busy} onclick={onEnable}>
        <Power size={16} />
        Включить {plan.enable.length}
      </button>
    {/if}
    {#if plan.disable.length > 0}
      <button
        class={plan.enable.length > 0 ? "btn-glass" : "btn-primary"}
        type="button"
        disabled={busy}
        onclick={onDisable}
      >
        <Power size={16} />
        Выключить {plan.disable.length}
      </button>
    {/if}
    {#if plan.restore.length > 0}
      <button class="btn-primary" type="button" disabled={busy} onclick={onRestore}>
        <ArrowCounterClockwise size={16} />
        Вернуть {plan.restore.length}
      </button>
    {/if}
    {#if plan.remove.length > 0}
      <button class="btn-glass danger" type="button" disabled={busy} onclick={onDelete}>
        <Trash size={16} />
        Удалить {plan.remove.length}
      </button>
    {/if}
  </div>
  {#if plan.enable.length + plan.disable.length + plan.remove.length + plan.restore.length === 0}
    <p class="hint">С выбранными модами нечего делать: они из репозитория игры — ими управляет сама игра.</p>
  {/if}
  {#if plan.remove.length < count && plan.remove.length > 0}
    <p class="hint">Моды из репозитория игры удаляются в самой игре — их здесь не тронем.</p>
  {/if}
  <p class="hint">Удалённые можно вернуть: кнопка «Отменить» или фильтр «Удалённые».</p>
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

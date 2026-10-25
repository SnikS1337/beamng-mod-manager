import type { ModCardDto } from "#lib/ipc/library.js";

/** Что палитра просит у гаража: выбрать мод, включить фильтр, открыть выбор архива. */
export type GarageRequest =
  | { kind: "select"; id: string }
  | { kind: "filter"; filter: "updates" | "unchecked" | "disabled" }
  | { kind: "archive" };

class Palette {
  open = $state(false);
  /** Карточки гаража: их показывает гараж, палитра ищет по ним. */
  mods = $state<ModCardDto[]>([]);
  garageRequest = $state<GarageRequest | null>(null);

  show(): void {
    this.open = true;
  }

  hide(): void {
    this.open = false;
  }

  toggle(): void {
    this.open = !this.open;
  }
}

export const palette = new Palette();

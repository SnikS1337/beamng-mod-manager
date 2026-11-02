/** Подпись переключателя для экранного диктора: действие и какой мод. */
export function toggleLabel(enabled: boolean, name: string): string {
  return `${enabled ? "Выключить" : "Включить"} мод «${name}»`;
}

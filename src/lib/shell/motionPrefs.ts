export interface MotionRoot {
  classList: { contains(name: string): boolean };
}

/** Длительность для JS-анимаций (Svelte transitions) с учётом режима «Быстро». */
export function motionDuration(ms: number, root?: MotionRoot): number {
  const element = root ?? (typeof document === "undefined" ? null : document.documentElement);
  if (!element) return 0;
  if (element.classList.contains("no-motion")) return 0;
  // Пока мод «прилетает» в гараж, окно установки уходит вместе с переходом:
  // своё исчезновение Svelte попало бы в снимок нового вида полупрозрачным.
  if (element.classList.contains("flow-arrive")) return 0;
  return ms;
}

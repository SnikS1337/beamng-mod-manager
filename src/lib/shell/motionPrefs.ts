export interface MotionRoot {
  classList: { contains(name: string): boolean };
}

/** Длительность для JS-анимаций (Svelte transitions) с учётом режима «Быстро». */
export function motionDuration(ms: number, root?: MotionRoot): number {
  const element = root ?? (typeof document === "undefined" ? null : document.documentElement);
  if (!element) return 0;
  return element.classList.contains("no-motion") ? 0 : ms;
}

import { motion } from "./motion.svelte.js";

/** Мягкая кромка волны, px: граница новой темы не режет окно ножом. */
export const WAVE_FEATHER = 120;

/**
 * Откуда и докуда идёт волна новой темы: центр — место нажатия (клавиатура — центр
 * окна), радиус — до самого дальнего угла плюс мягкая кромка, чтобы в конце окно
 * было залито целиком.
 */
export function waveGeometry(
  point: { x: number; y: number } | null,
  size: { width: number; height: number },
): { x: number; y: number; radius: number } {
  const x = point && point.x > 0 ? point.x : size.width / 2;
  const y = point && point.y > 0 ? point.y : size.height / 2;
  const far = Math.max(
    Math.hypot(x, y),
    Math.hypot(size.width - x, y),
    Math.hypot(x, size.height - y),
    Math.hypot(size.width - x, size.height - y),
  );
  return { x, y, radius: Math.ceil(far + WAVE_FEATHER) };
}

/**
 * Сменить тему, акцент или фон волной от нажатого места: новый вид расходится кругом
 * с мягкой кромкой и заливает окно. В «Быстро» — сразу (motion.run сам решает).
 */
export function themeWave(event: MouseEvent | null, apply: () => void): Promise<void> {
  if (typeof document !== "undefined" && typeof window !== "undefined") {
    const { x, y, radius } = waveGeometry(event ? { x: event.clientX, y: event.clientY } : null, {
      width: window.innerWidth,
      height: window.innerHeight,
    });
    const root = document.documentElement;
    root.style.setProperty("--theme-x", `${x}px`);
    root.style.setProperty("--theme-y", `${y}px`);
    root.style.setProperty("--theme-r", `${radius}px`);
  }
  return motion.run(apply, "flow-theme");
}

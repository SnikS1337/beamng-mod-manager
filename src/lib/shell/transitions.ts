import { cubicOut } from "svelte/easing";
import type { TransitionConfig } from "svelte/transition";
import { motionDuration } from "./motionPrefs.js";

type Centering = "both" | "x" | "none";

export interface PopOptions {
  duration?: number;
  y?: number;
  scale?: number;
  /** Как элемент центрирован базовым CSS: `translate(-50%, -50%)`, `translateX(-50%)` или без сдвига. */
  centered?: Centering;
  delay?: number;
}

function centerOf(centered: Centering): string {
  if (centered === "both") return "translate(-50%, -50%) ";
  if (centered === "x") return "translateX(-50%) ";
  return "";
}

/**
 * Пружина: доезжает с перелётом ~1.5% и успокаивается к концу (та же, что
 * `--ease-spring` в CSS). Затухающее колебание, ζ = 0.8, ω = 10; ровно 0 и 1 на краях.
 */
export function spring(t: number): number {
  if (t <= 0) return 0;
  if (t >= 1) return 1;
  const zeta = 0.8;
  const omega = 10;
  const damped = omega * Math.sqrt(1 - zeta * zeta);
  const decay = Math.exp(-zeta * omega * t);
  return 1 - decay * (Math.cos(damped * t) + ((zeta * omega) / damped) * Math.sin(damped * t));
}

/** Появление/исчезновение с лёгким подъёмом и масштабом; сохраняет центрирование окна. */
export function pop(_node: Element, options: PopOptions = {}): TransitionConfig {
  const duration = motionDuration(options.duration ?? 300);
  const y = options.y ?? 8;
  const fromScale = options.scale ?? 0.98;
  const center = centerOf(options.centered ?? "none");
  return {
    duration,
    delay: duration === 0 ? 0 : (options.delay ?? 0),
    css: (t) => {
      // Прозрачность — плавно, без перелёта; сдвиг и масштаб — пружиной.
      const fade = cubicOut(t);
      const moved = spring(t);
      const offset = (1 - moved) * y;
      const scale = fromScale + (1 - fromScale) * moved;
      return `opacity:${fade}; transform:${center}translateY(${offset}px) scale(${scale});`;
    },
  };
}

/** Мягкое проявление для подложек и тостов, которым не нужен сдвиг. */
export function softFade(_node: Element, options: { duration?: number } = {}): TransitionConfig {
  return {
    duration: motionDuration(options.duration ?? 180),
    css: (t) => `opacity:${t};`,
  };
}

/**
 * Новая версия на карточке «прокручивается» снизу, как цифры на счётчике.
 * Старая уходит без анимации: обе в одной строке толкались бы.
 */
export function rollIn(_node: Element, options: { duration?: number } = {}): TransitionConfig {
  const duration = motionDuration(options.duration ?? 420);
  return {
    duration,
    css: (t) => {
      const moved = spring(t);
      return `display:inline-block; opacity:${cubicOut(t)}; transform:translateY(${(1 - moved) * 70}%);`;
    },
  };
}

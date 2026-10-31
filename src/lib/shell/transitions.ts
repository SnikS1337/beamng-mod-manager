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

/** Появление/исчезновение с лёгким подъёмом и масштабом; сохраняет центрирование окна. */
export function pop(_node: Element, options: PopOptions = {}): TransitionConfig {
  const duration = motionDuration(options.duration ?? 240);
  const y = options.y ?? 8;
  const fromScale = options.scale ?? 0.98;
  const center = centerOf(options.centered ?? "none");
  return {
    duration,
    delay: duration === 0 ? 0 : (options.delay ?? 0),
    css: (t) => {
      const eased = cubicOut(t);
      const offset = (1 - eased) * y;
      const scale = fromScale + (1 - fromScale) * eased;
      return `opacity:${eased}; transform:${center}translateY(${offset}px) scale(${scale});`;
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

/**
 * Где на фоне стоит свет для раздела, в процентах окна (50/50 — без сдвига),
 * и насколько он яркий. Свет держится ближе к главному на экране: в гараже —
 * над сеткой, в обновлениях — над списком, в настройках — приглушён.
 */
export interface LightSpot {
  x: number;
  y: number;
  power: number;
}

const SPOTS: Record<string, LightSpot> = {
  "/": { x: 56, y: 48, power: 1 },
  "/updates": { x: 42, y: 50, power: 1 },
  "/search": { x: 30, y: 46, power: 0.95 },
  "/settings": { x: 22, y: 44, power: 0.7 },
};

const FALLBACK: LightSpot = { x: 50, y: 50, power: 0.85 };

export function lightSpot(pathname: string): LightSpot {
  const clean = pathname.length > 1 ? pathname.replace(/\/+$/, "") : pathname;
  return SPOTS[clean] ?? FALLBACK;
}

/**
 * Фон спит, когда его не видно: вкладка скрыта или окно свёрнуто. WebView2 не всегда
 * помечает свёрнутое окно скрытым, поэтому свёрнутость спрашиваем у окна отдельно.
 * Окно просто не в фокусе — видно, не спит.
 */
export function backgroundSleeps(state: { hidden: boolean; minimized: boolean }): boolean {
  return state.hidden || state.minimized;
}

/**
 * Ответ «свёрнуто ли окно» приходит асинхронно. Ответ на старый вопрос не должен
 * перетереть более свежий: окно успели развернуть, пока шёл первый ответ.
 */
export class LatestAnswer {
  #asked = 0;

  ask(): number {
    this.#asked += 1;
    return this.#asked;
  }

  isLatest(ticket: number): boolean {
    return ticket === this.#asked;
  }
}

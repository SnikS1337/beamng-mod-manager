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

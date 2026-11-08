/** Разделы в порядке меню: настройки — внизу. */
export const ROUTE_ORDER = ["/", "/updates", "/search", "/settings"] as const;

function placeOf(path: string | null | undefined): number {
  if (!path) return -1;
  const clean = path.length > 1 ? path.replace(/\/+$/, "") : path;
  return ROUTE_ORDER.findIndex((route) => route === clean || (route !== "/" && clean.startsWith(`${route}/`)));
}

/**
 * Куда ехать новому разделу: ниже в меню — въезжает снизу (1), выше — сверху (-1).
 * Неизвестный раздел — снизу, как обычная смена страницы.
 */
export function routeDirection(from: string | null | undefined, to: string | null | undefined): 1 | -1 {
  const a = placeOf(from);
  const b = placeOf(to);
  if (a < 0 || b < 0) return 1;
  return b < a ? -1 : 1;
}

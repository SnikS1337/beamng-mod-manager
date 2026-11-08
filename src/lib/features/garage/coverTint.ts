/**
 * Лёгкий оттенок панели мода по его обложке. Цвет считается один раз по картинке
 * 16×16 и запоминается: на скорость не влияет. Серые, почти чёрные и почти белые
 * обложки оттенка не дают — панель осталась бы грязно-серой.
 */

/** Насыщенность, ниже которой обложку считаем серой. */
const MIN_SATURATION = 0.18;
const SIDE = 16;
const CACHE_LIMIT = 200;

/** Средний цвет пикселей RGBA; яркие, насыщенные пиксели весят больше серого фона. */
export function tintFromPixels(data: ArrayLike<number>): string | null {
  let red = 0;
  let green = 0;
  let blue = 0;
  let total = 0;
  for (let index = 0; index + 3 < data.length; index += 4) {
    const alpha = data[index + 3]! / 255;
    if (alpha < 0.5) continue;
    const r = data[index]!;
    const g = data[index + 1]!;
    const b = data[index + 2]!;
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const saturation = max === 0 ? 0 : (max - min) / max;
    const weight = alpha * (0.15 + saturation);
    red += r * weight;
    green += g * weight;
    blue += b * weight;
    total += weight;
  }
  if (total === 0) return null;
  const [hue, saturation, lightness] = toHsl(red / total, green / total, blue / total);
  if (saturation < MIN_SATURATION || lightness < 0.08 || lightness > 0.95) return null;
  // Ровная середина: оттенок читается и в тёмной, и в светлой теме, а текст — тоже.
  const s = Math.round(Math.min(saturation, 0.75) * 100);
  const l = Math.round(Math.min(0.6, Math.max(0.42, lightness)) * 100);
  return `hsl(${Math.round(hue)} ${s}% ${l}%)`;
}

function toHsl(r: number, g: number, b: number): [number, number, number] {
  const red = r / 255;
  const green = g / 255;
  const blue = b / 255;
  const max = Math.max(red, green, blue);
  const min = Math.min(red, green, blue);
  const lightness = (max + min) / 2;
  if (max === min) return [0, 0, lightness];
  const delta = max - min;
  const saturation = delta / (1 - Math.abs(2 * lightness - 1));
  let hue: number;
  if (max === red) hue = ((green - blue) / delta) % 6;
  else if (max === green) hue = (blue - red) / delta + 2;
  else hue = (red - green) / delta + 4;
  hue *= 60;
  if (hue < 0) hue += 360;
  return [hue, Math.min(1, saturation), lightness];
}

const cache = new Map<string, Promise<string | null>>();

/** Оттенок обложки по адресу. Не вышло (картинка не открылась, чужой адрес) — `null`. */
export function coverTint(url: string | null | undefined): Promise<string | null> {
  if (!url || typeof document === "undefined") return Promise.resolve(null);
  const known = cache.get(url);
  if (known) return known;
  const request = readTint(url).catch(() => null);
  cache.set(url, request);
  // Не копим без края: самые старые адреса уходят первыми.
  while (cache.size > CACHE_LIMIT) {
    const oldest = cache.keys().next().value;
    if (oldest === undefined) break;
    cache.delete(oldest);
  }
  return request;
}

async function readTint(url: string): Promise<string | null> {
  const image = new Image();
  // Без этого холст «испорчен» чужой картинкой и пиксели не прочитать.
  image.crossOrigin = "anonymous";
  image.decoding = "async";
  image.src = url;
  await image.decode();
  const canvas = document.createElement("canvas");
  canvas.width = SIDE;
  canvas.height = SIDE;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return null;
  context.drawImage(image, 0, 0, SIDE, SIDE);
  return tintFromPixels(context.getImageData(0, 0, SIDE, SIDE).data);
}

/** Для тестов: забыть посчитанное. */
export function forgetTints(): void {
  cache.clear();
}

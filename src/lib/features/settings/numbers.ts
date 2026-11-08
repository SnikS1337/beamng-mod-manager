/**
 * Числовые поля настроек. Бэкенд ждёт целые в i32: «2.5» или «1e12» в поле —
 * ошибка разбора, и настройки не сохранялись бы вовсе. Округляем и держим в тех же
 * пределах, что и бэкенд (`AppSettings::normalize`).
 */
export const NUMBER_RANGES = {
  checkTtlHours: [1, 168],
  autoCheckEveryHours: [0, 168],
  keepBackups: [1, 20],
  maxCardChecksPerSource: [5, 500],
  activeModsWarnAbove: [0, 1000],
} as const;

export type NumberField = keyof typeof NUMBER_RANGES;

export function wholeIn(value: number, field: NumberField): number {
  const [min, max] = NUMBER_RANGES[field];
  return Math.min(max, Math.max(min, Math.round(value)));
}

/** Те же настройки, но все числа — целые в допустимых пределах. Нечисла не трогаем. */
export function wholeNumbers<T extends Partial<Record<NumberField, unknown>>>(value: T): T {
  const result = { ...value };
  for (const field of Object.keys(NUMBER_RANGES) as NumberField[]) {
    const current = result[field];
    if (typeof current === "number" && Number.isFinite(current)) {
      (result as Record<NumberField, unknown>)[field] = wholeIn(current, field);
    }
  }
  return result;
}

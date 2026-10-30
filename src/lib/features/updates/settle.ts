/** Итог вызова бэкенда без исключения: успех со значением или причина отказа. */
export type Settled<T> = { ok: true; value: T } | { ok: false; error: unknown };

/**
 * Выполнить вызов и вернуть итог. Раньше «Обновления» глотали ошибку и всё равно
 * показывали «Следим за страницей», хотя бэкенд отказал.
 */
export async function settle<T>(call: () => Promise<T>): Promise<Settled<T>> {
  try {
    return { ok: true, value: await call() };
  } catch (error) {
    return { ok: false, error };
  }
}

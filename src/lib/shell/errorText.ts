/**
 * Текст ошибки для человека: строка от бэкенда (`AppError` приходит как
 * `{ code, message }`), `Error` или что угодно ещё — тогда `fallback`.
 */
export function errorText(reason: unknown, fallback = "Не удалось выполнить действие."): string {
  if (typeof reason === "string") return reason || fallback;
  if (reason && typeof reason === "object" && "message" in reason) {
    const message = String((reason as { message: unknown }).message ?? "");
    return message.trim() ? message : fallback;
  }
  return fallback;
}

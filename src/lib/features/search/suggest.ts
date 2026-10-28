export type SuggestionSource = "history" | "library";

export interface Suggestion {
  value: string;
  source: SuggestionSource;
}

/**
 * Подсказки для строки поиска без сети: недавние запросы и установленные моды.
 * Совпадение с начала названия важнее совпадения в середине.
 */
export function suggest(
  input: string,
  history: readonly string[],
  library: readonly string[],
  limit = 6,
): Suggestion[] {
  const needle = input.trim().toLowerCase();
  if (needle.length < 2) return [];
  const seen = new Set<string>([needle]);
  const result: Suggestion[] = [];
  const add = (values: readonly string[], source: SuggestionSource) => {
    const found = values
      .map((value, index) => ({ value: value.trim(), index }))
      .filter(({ value }) => value.toLowerCase().includes(needle))
      .sort((left, right) => {
        const leftStart = left.value.toLowerCase().startsWith(needle) ? 0 : 1;
        const rightStart = right.value.toLowerCase().startsWith(needle) ? 0 : 1;
        return leftStart - rightStart || left.index - right.index;
      });
    for (const { value } of found) {
      const key = value.toLowerCase();
      if (!value || seen.has(key)) continue;
      seen.add(key);
      result.push({ value, source });
    }
  };
  add(history, "history");
  add(library, "library");
  return result.slice(0, limit);
}

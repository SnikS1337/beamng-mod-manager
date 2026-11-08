/**
 * Картинки выдачи: адрес на сайте → файл в кеше программы (`bmm.localhost`). Сами
 * файлы лежат на диске (до 300 МБ, старые вытесняются), а здесь — только какой файл
 * какой картинке соответствует. Тогда при возврате в поиск и в следующий запуск
 * картинка встаёт сразу, без запроса и без ожидания очереди к сайту.
 */

const STORE_KEY = "bmm.searchPreviews";
/** Сколько соответствий помнить: файлов в кеше на 300 МБ примерно столько же. */
export const PREVIEW_MEMORY_LIMIT = 4000;

let memory: Map<string, string> | null = null;
let saveTimer: ReturnType<typeof setTimeout> | undefined;

/** Только свои адреса: чужое из испорченного хранилища в `src` не попадёт. */
function ownUrl(value: unknown): value is string {
  return typeof value === "string" && value.startsWith("http://bmm.localhost/search-preview/");
}

function known(): Map<string, string> {
  if (memory) return memory;
  memory = new Map();
  try {
    const raw = globalThis.localStorage?.getItem(STORE_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : null;
    if (parsed && typeof parsed === "object") {
      for (const [remote, local] of Object.entries(parsed as Record<string, unknown>)) {
        if (ownUrl(local)) memory.set(remote, local);
      }
    }
  } catch {
    // Испорченное хранилище — картинки просто запросятся заново.
  }
  return memory;
}

function save(): void {
  if (saveTimer) return;
  saveTimer = setTimeout(() => {
    saveTimer = undefined;
    try {
      globalThis.localStorage?.setItem(STORE_KEY, JSON.stringify(Object.fromEntries(known())));
    } catch {
      // Нет места — в следующий раз запросим.
    }
  }, 600);
}

/** Файл картинки, если она уже скачана. */
export function knownPreview(remote: string): string | undefined {
  return known().get(remote);
}

export function rememberPreview(remote: string, local: string): void {
  if (!ownUrl(local)) return;
  const map = known();
  map.delete(remote);
  map.set(remote, local);
  while (map.size > PREVIEW_MEMORY_LIMIT) {
    const oldest = map.keys().next().value;
    if (oldest === undefined) break;
    map.delete(oldest);
  }
  save();
}

/** Файл пропал (кеш почистили) — забыть, чтобы картинку запросили снова. */
export function forgetPreview(local: string): void {
  const map = known();
  for (const [remote, value] of map) if (value === local) map.delete(remote);
  save();
}

/** «Очистить картинки сайтов»: файлов больше нет — забываем все соответствия. */
export function clearPreviewMemory(): void {
  known().clear();
  try {
    globalThis.localStorage?.removeItem(STORE_KEY);
  } catch {
    // Не страшно: адреса без файлов отсеются при показе.
  }
}

/** Для тестов. */
export function resetPreviewMemory(): void {
  memory = null;
  if (saveTimer) clearTimeout(saveTimer);
  saveTimer = undefined;
}

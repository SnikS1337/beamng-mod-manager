export function formatSize(bytes: number): string {
  if (!bytes || bytes <= 0) return "—";
  if (bytes < 1024) return `${Math.round(bytes)} Б`;
  if (bytes < 1024 * 1024) {
    return `${Math.round(bytes / 1024)} КБ`;
  }
  const mb = bytes / (1024 * 1024);
  if (mb >= 1024) {
    return `${(mb / 1024).toFixed(1).replace(".", ",")} ГБ`;
  }
  return `${Math.round(mb)} МБ`;
}

export function plural(count: number, one: string, few: string, many: string): string {
  const mod100 = Math.abs(count) % 100;
  const mod10 = mod100 % 10;
  if (mod100 >= 11 && mod100 <= 14) return many;
  if (mod10 === 1) return one;
  if (mod10 >= 2 && mod10 <= 4) return few;
  return many;
}

/** «5 000»: разряды обычным пробелом (у `toLocaleString` неразрывный узкий). */
export function groupDigits(count: number): string {
  return count.toLocaleString("ru-RU").replace(/[\u00a0\u202f]/g, " ");
}

/**
 * «21 из 49 модов»: после «из N» слово стоит в родительном падеже по N
 * («из 21 мода», «из 49 модов»), а не по первому числу («21 из 49 мод»).
 */
export function shownOfLabel(shown: number, total: number): string {
  if (shown === total) return modsLabel(total);
  const word = Math.abs(total) % 10 === 1 && Math.abs(total) % 100 !== 11 ? "мода" : "модов";
  return `${groupDigits(shown)} из ${groupDigits(total)} ${word}`;
}

export function modsLabel(count: number): string {
  return `${groupDigits(count)} ${plural(count, "мод", "мода", "модов")}`;
}

/**
 * Подпись под «Гаражом». `count` — моды (копии одного мода — один), `files` — архивы
 * на диске: без него «39 модов» в шапке и «Все 35» в фильтре выглядели как ошибка.
 */
export function headerNote(count: number, updates: number, files = count): string {
  if (count === 0) return "модов пока нет";
  const base =
    files > count
      ? `${modsLabel(count)} (${groupDigits(files)} ${plural(files, "файл", "файла", "файлов")})`
      : modsLabel(count);
  if (updates > 0) {
    return `${base} · ${updates} с обновлением`;
  }
  return base;
}

export function formatDate(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat("ru-RU", {
    day: "numeric",
    month: "long",
    year: "numeric",
  }).format(date);
}

export function formatDateTime(iso: string | null): string {
  if (!iso) return "—";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat("ru-RU", {
    day: "numeric",
    month: "long",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

export function normalizeText(value: string): string {
  return value.toLocaleLowerCase("ru").replaceAll("ё", "е");
}

export function words(value: string): string[] {
  return normalizeText(value)
    .split(/[^\p{L}\p{Nd}]+/u)
    .filter(Boolean);
}

export function matchesQuery(mod: { name: string; fileName: string }, query: string): boolean {
  const needles = words(query);
  if (needles.length === 0) return true;
  const haystack = words(`${mod.name} ${mod.fileName}`);
  return needles.every((needle) => haystack.some((word) => word.startsWith(needle)));
}

/** «18 минут назад», «2 дня назад», «3 недели назад» — как в legacy. */
export function formatAgo(iso: string | null | undefined, now = Date.now()): string {
  if (!iso) return "";
  const at = new Date(iso).getTime();
  if (Number.isNaN(at)) return "";
  const minutes = Math.max(0, Math.floor((now - at) / 60000));
  if (minutes < 1) return "только что";
  if (minutes < 60) return `${minutes} ${plural(minutes, "минуту", "минуты", "минут")} назад`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} ${plural(hours, "час", "часа", "часов")} назад`;
  const days = Math.floor(hours / 24);
  if (days < 14) return `${days} ${plural(days, "день", "дня", "дней")} назад`;
  const weeks = Math.floor(days / 7);
  if (weeks < 9) return `${weeks} ${plural(weeks, "неделю", "недели", "недель")} назад`;
  const months = Math.floor(days / 30);
  return `${months} ${plural(months, "месяц", "месяца", "месяцев")} назад`;
}

/** «сегодня в 3:47», «вчера в 21:10», «12 октября в 9:05». */
export function formatWhen(iso: string | null | undefined, now = new Date()): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const time = `${date.getHours()}:${String(date.getMinutes()).padStart(2, "0")}`;
  const day = (value: Date) => new Date(value.getFullYear(), value.getMonth(), value.getDate()).getTime();
  const diff = Math.round((day(now) - day(date)) / 86400000);
  if (diff === 0) return `сегодня в ${time}`;
  if (diff === 1) return `вчера в ${time}`;
  const label = new Intl.DateTimeFormat("ru-RU", {
    day: "numeric",
    month: "long",
    ...(date.getFullYear() === now.getFullYear() ? {} : { year: "numeric" }),
  }).format(date);
  return `${label} в ${time}`;
}

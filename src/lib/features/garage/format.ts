export function formatSize(bytes: number): string {
  if (!bytes || bytes <= 0) return "—";
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

export function modsLabel(count: number): string {
  return `${count} ${plural(count, "мод", "мода", "модов")}`;
}

export function headerNote(count: number, updates: number): string {
  if (count === 0) return "модов пока нет";
  const base = modsLabel(count);
  if (updates > 0) {
    return `${base} · ${updates} ${plural(updates, "ждёт", "ждут", "ждут")} обновления`;
  }
  return base;
}

export function kindLabel(kind: string): string {
  switch (kind) {
    case "vehicle":
      return "Машина";
    case "level":
      return "Карта";
    case "sound":
      return "Звук";
    case "script":
      return "Скрипт";
    default:
      return "Мод";
  }
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

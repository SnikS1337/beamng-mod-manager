import { archiveBaseName, isArchivePath } from "#lib/ipc/incoming.js";
import { plural } from "#lib/features/garage/format.js";

/** Сколько имён показывать над окном: остальное — «и ещё N». */
export const DROP_NAMES = 3;

/** Что сделать с брошенными файлами: какие архивы проверить, что пропустить. */
export interface DropPlan {
  /** Архивы к проверке — без повторов, без уже открытого и уже ждущих в очереди. */
  add: string[];
  /** Файлы и папки, которые не архивы. */
  skipped: number;
  /** Архивы, которые уже открыты или ждут в очереди. */
  repeats: number;
}

/** Пути Windows регистр не различают: «Car.ZIP» и «car.zip» — один файл. */
function same(left: string, right: string): boolean {
  return left.replaceAll("\\", "/").toLowerCase() === right.replaceAll("\\", "/").toLowerCase();
}

export function dropPlan(paths: string[], open: string | null, queued: string[]): DropPlan {
  const add: string[] = [];
  let skipped = 0;
  let repeats = 0;
  for (const path of paths) {
    if (!path) continue;
    if (!isArchivePath(path)) {
      skipped += 1;
      continue;
    }
    if (add.some((item) => same(item, path))) continue;
    if ((open && same(open, path)) || queued.some((item) => same(item, path))) {
      repeats += 1;
      continue;
    }
    add.push(path);
  }
  return { add, skipped, repeats };
}

export interface DropSummary {
  /** ok — проверим сейчас, queue — после открытого окна, none — брать нечего. */
  tone: "ok" | "queue" | "none";
  title: string;
  names: string[];
  more: number;
  note: string | null;
}

/**
 * Подсказка над окном, пока архив только несут: что будет, когда отпустят.
 * `paths` пусто — имён не знаем (браузерная демо-версия): общий текст.
 */
export function dropSummary(plan: DropPlan | null, dialogOpen: boolean): DropSummary {
  if (!plan) {
    return {
      tone: dialogOpen ? "queue" : "ok",
      title: dialogOpen ? "Отпустите — проверим после текущего архива" : "Отпустите — проверим и предложим установить",
      names: [],
      more: 0,
      note: "zip, rar или 7z",
    };
  }
  const notes: string[] = [];
  if (plan.skipped > 0) {
    notes.push(
      `${plan.skipped} ${plural(plan.skipped, "файл", "файла", "файлов")} — не архив, пропустим`,
    );
  }
  if (plan.repeats > 0 && plan.add.length > 0) {
    notes.push(`${plan.repeats} уже ${plan.repeats === 1 ? "открыт или ждёт" : "открыты или ждут"} — второй раз не добавим`);
  }
  const names = plan.add.slice(0, DROP_NAMES).map((path) => archiveBaseName(path));
  const more = Math.max(0, plan.add.length - DROP_NAMES);
  if (plan.add.length === 0) {
    let title = "Здесь нет архивов";
    if (plan.repeats > 0) {
      title = plan.repeats === 1 ? "Этот архив уже открыт или ждёт очереди" : "Эти архивы уже открыты или ждут очереди";
    } else if (plan.skipped === 1) title = "Это не архив";
    return { tone: "none", title, names: [], more: 0, note: "Нужен zip, rar или 7z" };
  }
  const count = plan.add.length;
  const what = count === 1 ? "" : ` ${count} ${plural(count, "архив", "архива", "архивов")}`;
  return {
    tone: dialogOpen ? "queue" : "ok",
    title: dialogOpen
      ? `Отпустите — проверим${what} после текущего`
      : count === 1
        ? "Отпустите — проверим и предложим установить"
        : `Отпустите — проверим${what}`,
    names,
    more,
    note: notes.length > 0 ? notes.join(" · ") : null,
  };
}

/** Тост после броска, если взять было нечего: молча ничего не делать — непонятно. */
export function dropRefusal(plan: DropPlan): string | null {
  if (plan.add.length > 0) return null;
  if (plan.repeats > 0) return "Этот архив уже открыт или ждёт очереди.";
  if (plan.skipped > 0) return "Это не архив: программа ставит моды из zip, rar или 7z.";
  return null;
}

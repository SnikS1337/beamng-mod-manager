import type { UpdateSite, UpdateSource } from "#lib/ipc/updates.js";

/**
 * Пройденные проверки «вы человек?» и чьи ответы о сайте после этого устарели.
 *
 * Было: «Пройти проверку ModHub» сколько ни проходи — кнопка возвращалась. Проверка,
 * начатая до прохождения, пропускала сайт и в конце писала «просит проверку»,
 * хотя сайт уже пускал. Теперь ответ о сайте от проверки, начатой раньше, чем его
 * прошли, не возвращает кнопку. Вернёт её только то, что сайт попросил снова:
 * сохранённое состояние или проверка, начатая уже после прохождения.
 *
 * Время — порядковые номера событий, а не часы: важен только порядок.
 */
export interface Passes {
  /** Номер последнего события: прохождения или старта проверки. */
  clock: number;
  /** Сайт → номер его последнего прохождения. */
  passed: Record<string, number>;
}

export function noPasses(): Passes {
  return { clock: 0, passed: {} };
}

/** Сайт пустил программу. */
export function markPassed(passes: Passes, site: string): Passes {
  const clock = passes.clock + 1;
  return { clock, passed: { ...passes.passed, [site]: clock } };
}

/** Своя проверка стартовала: номер её начала — для ответов, что она пришлёт. */
export function runStarted(passes: Passes): { passes: Passes; epoch: number } {
  const clock = passes.clock + 1;
  return { passes: { ...passes, clock }, epoch: clock };
}

export function wasPassed(passes: Passes, site: string): boolean {
  return site in passes.passed;
}

/** Ответ проверки, начатой в `epoch`, устарел для сайта: его прошли позже. */
function stale(passes: Passes, site: string, epoch: number): boolean {
  const at = passes.passed[site];
  return at !== undefined && at > epoch;
}

/**
 * Ответы сайтов из проверки, начатой в `epoch`: «просит проверку» у сайта, который
 * прошли после её начала, — уже не так. Фоновая проверка начинается неизвестно когда,
 * её ответы считаем начатыми до любого прохождения (`epoch` = 0): если сайт правда
 * попросил снова, это скажет сохранённое состояние сайта.
 */
export function freshSources(sources: UpdateSource[], passes: Passes, epoch: number): UpdateSource[] {
  return sources.map((source) =>
    source.status === "needsHumanCheck" && stale(passes, source.id, epoch)
      ? { ...source, status: "ok", note: null }
      : source,
  );
}

/** Сайт прошли — в панели он сразу «пускает», не дожидаясь чтения состояния. */
export function sitesAfterPass(sites: UpdateSite[], site: string): UpdateSite[] {
  return sites.map((item) =>
    item.id === site && item.status === "human_check" ? { ...item, status: "ok", note: null } : item,
  );
}

/**
 * Что делать после прохождения: сайт без модов — искать на нём страницы, с модами —
 * проверить снова. Если своя проверка уже идёт, повтор ждёт её конца: раньше он
 * молча не запускался, и кнопка возвращалась вместе с концом старой проверки.
 */
export function afterPass(
  tracked: number,
  checking: boolean,
): "findPages" | "check" | "afterRun" {
  if (checking) return "afterRun";
  return tracked === 0 ? "findPages" : "check";
}

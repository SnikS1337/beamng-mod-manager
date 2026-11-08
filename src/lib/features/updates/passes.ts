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

/** Что запустить, когда сайт пустил: на сайте без модов — поиск страниц, с модами — проверку. */
export type Followup = "check" | "findPages";

export function followupOf(tracked: number): Followup {
  return tracked === 0 ? "findPages" : "check";
}

/**
 * Сайт прошли, пока идёт своя проверка: повтор ждёт её конца. Раньше он молча не
 * запускался, и кнопка возвращалась вместе с концом старой проверки. Сайтов в очереди
 * может быть несколько — прошли ModHub, потом Modland.
 */
export function queuePass(queue: string[], site: string): string[] {
  return queue.includes(site) ? queue : [...queue, site];
}

/** Какой своей проверкой занята страница «Обновлений». */
export type RunKind = Followup | "retry" | { site: string };

/**
 * Началась своя проверка — она начата после прохождения, и сайты, которые она покроет,
 * повторять незачем: проверка всех — сайты с модами, поиск страниц — сайты без модов,
 * проверка одного сайта — только его. «Повторить» одного сайта идёт по суточной
 * очереди и полной проверки не заменяет.
 */
export function coveredBy(queue: string[], kind: RunKind, trackedOf: (site: string) => number): string[] {
  if (kind === "retry") return queue;
  if (typeof kind === "object") return queue.filter((site) => site !== kind.site);
  return queue.filter((site) => followupOf(trackedOf(site)) !== kind);
}

/** Что запустить из очереди: проверку одного сайта или поиск страниц. */
export type QueueRun = { kind: "site"; site: string } | { kind: "findPages" };

/**
 * Своя проверка закончилась: что запустить из очереди. Сайты с модами — по одному,
 * каждый своей проверкой (остальные сайты не трогаем); поиск страниц — один на все
 * сайты без модов, после них.
 */
export function nextFromQueue(
  queue: string[],
  trackedOf: (site: string) => number,
): { run: QueueRun | null; rest: string[] } {
  if (queue.length === 0) return { run: null, rest: [] };
  const site = queue.find((item) => followupOf(trackedOf(item)) === "check");
  if (site !== undefined) return { run: { kind: "site", site }, rest: queue.filter((item) => item !== site) };
  return { run: { kind: "findPages" }, rest: [] };
}

/**
 * Ответ проверки одного сайта: этот сайт — свежий, остальные — как были. Раньше
 * список сайтов заменялся целиком, и после проверки одного сайта остальные пропадали.
 */
export function withSources(current: UpdateSource[], fresh: UpdateSource[]): UpdateSource[] {
  const byId = new Map(fresh.map((source) => [source.id, source]));
  const kept = current.map((source) => byId.get(source.id) ?? source);
  const added = fresh.filter((source) => !current.some((item) => item.id === source.id));
  return [...kept, ...added];
}

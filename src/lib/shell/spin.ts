import { motionDuration } from "./motionPrefs.js";

/** Один оборот на полном ходу, мс. */
export const SPIN_PERIOD = 900;
/** Скорость на полном ходу, градусов в мс. */
export const SPIN_SPEED = 360 / SPIN_PERIOD;

/**
 * Разгон: первые пол-оборота с ускорением. Кривая подобрана так, что в конце разгона
 * скорость ровно как у вращения (наклон конца кривой = WINDUP_MS·скорость/угол),
 * поэтому переход разгон → вращение без рывка. Проверяет тест.
 */
export const WINDUP_TURN = 180;
export const WINDUP_MS = 675;
export const WINDUP_EASING = [0.5, 0, 0.6, 0.4] as const;

/** Угол бесконечного вращения в момент `time` (мс), 0–360. */
export function angleAt(time: number, period = SPIN_PERIOD): number {
  if (!Number.isFinite(time) || time < 0) return 0;
  return ((time % period) / period) * 360;
}

/** Угол поворота из вычисленного `transform` («matrix(a, b, …)»), 0–360. */
export function angleFromMatrix(transform: string | null | undefined): number | null {
  const match = transform?.match(/^matrix\(([^)]+)\)/);
  if (!match) return null;
  const [a, b] = match[1]!.split(",").map((part) => Number(part.trim()));
  if (!Number.isFinite(a) || !Number.isFinite(b)) return null;
  const angle = (Math.atan2(b!, a!) * 180) / Math.PI;
  return Math.round((((angle % 360) + 360) % 360) * 1000) / 1000;
}

/**
 * Где значок и как быстро он крутится в момент остановки. Разгон считаем
 * равноускоренным — для стыковки скоростей этого хватает.
 */
export function motionAt(phase: "windup" | "spin", time: number, iterationStart = 0): { angle: number; speed: number } {
  const t = Number.isFinite(time) && time > 0 ? time : 0;
  if (phase === "windup") {
    const part = Math.min(1, t / WINDUP_MS);
    return { angle: WINDUP_TURN * part * part, speed: SPIN_SPEED * part };
  }
  return { angle: angleAt(t + iterationStart * SPIN_PERIOD), speed: SPIN_SPEED };
}

function bezier(points: readonly number[]): string {
  return `cubic-bezier(${points.map((value) => Math.round(value * 1000) / 1000).join(", ")})`;
}

/**
 * Через сколько градусов значок выглядит так же. У ArrowsClockwise две стрелки —
 * пол-оборота: встать можно на ближайшей половине, а не тянуть целый оборот.
 */
export const REST_STEP = 180;
/** Ближе этого не тормозим — резкий стоп; берём следующую половину. */
export const MIN_COAST = 45;
export const SETTLE_MIN_MS = 320;
export const SETTLE_MAX_MS = 900;

/**
 * Как доехать до покоя: вперёд до ближайшей «половины» (но не ближе MIN_COAST),
 * с ровным торможением: время = 2·путь/скорость — как у колеса, которое
 * отпустили. Кривая — cubic-bezier(1/3, s/3, 2/3, 1): её начальный наклон s
 * равен скорости вращения в момент остановки (без рывка), конец — полная
 * остановка. Если работа заняла миг и значок ещё не разогнался, он всё равно
 * делает плавные пол-оборота: нажатие не остаётся без ответа.
 */
export function settle(angle: number, speed = SPIN_SPEED): { to: number; duration: number; easing: string } {
  const from = ((angle % 360) + 360) % 360;
  let left = REST_STEP - (from % REST_STEP);
  if (left < MIN_COAST) left += REST_STEP;
  const pace = Math.max(0, speed);
  const ideal = pace > 0 ? (2 * left) / pace : Infinity;
  const duration = Math.round(Math.min(SETTLE_MAX_MS, Math.max(SETTLE_MIN_MS, ideal)));
  // Наклон начала кривой в её единицах: скорость · время / путь (не круче 3 — иначе
  // контрольная точка вышла бы за верх кривой).
  const slope = Math.min(3, pace * (duration / left));
  return { to: from + left, duration, easing: bezier([1 / 3, slope / 3, 2 / 3, 1]) };
}

/**
 * Значок крутится, пока идёт работа: мягко разгоняется, а когда работа кончилась —
 * доворачивает до целого оборота и плавно встаёт, а не прыгает в начало, как
 * обычная CSS-анимация. Web Animations: только `transform`, на композиторе.
 */
export function spinWhile(node: HTMLElement, busy: boolean) {
  /** Вращение (в разгоне — уже заведённое с задержкой). */
  let running: Animation | null = null;
  let windup: Animation | null = null;
  let phase: "windup" | "spin" = "windup";
  /** Доворот: пока идёт — `settling`, доиграл и держит угол — `resting`. */
  let settling: Animation | null = null;
  let resting: Animation | null = null;
  /** Угол, на котором значок стоит (0 или 180): с него и начнётся следующий разгон. */
  let rest = 0;

  const live = () => typeof node.animate === "function" && motionDuration(1) > 0;

  const currentAngle = (): number | null =>
    typeof getComputedStyle === "function" ? angleFromMatrix(getComputedStyle(node).transform) : null;

  /** Полный ход с угла `from`, через `delay` мс. */
  const spin = (from: number, delay = 0) =>
    node.animate([{ transform: "rotate(0deg)" }, { transform: "rotate(360deg)" }], {
      duration: SPIN_PERIOD,
      delay,
      iterations: Infinity,
      easing: "linear",
      iterationStart: (((from % 360) + 360) % 360) / 360,
    });

  const start = () => {
    if (running || !live()) return;
    if (settling) {
      // Работа началась снова, пока значок доворачивался: он уже в движении —
      // сразу полный ход с того же угла.
      const from = currentAngle() ?? rest;
      running = spin(from);
      settling.cancel();
      settling = null;
      phase = "spin";
      return;
    }
    phase = "windup";
    const base = rest;
    // Разгон и вращение заводятся сразу, на одной шкале времени: вращение с
    // задержкой вступает ровно в кадр, где кончается разгон. Через onfinish между
    // ними был бы кадр стоянки — заметная запинка.
    const ramp = node.animate(
      [{ transform: `rotate(${base}deg)` }, { transform: `rotate(${base + WINDUP_TURN}deg)` }],
      { duration: WINDUP_MS, easing: bezier(WINDUP_EASING), fill: "forwards" },
    );
    windup = ramp;
    running = spin(base + WINDUP_TURN, WINDUP_MS);
    // Новые анимации уже поверх — старую стоянку снимаем без пустого кадра.
    resting?.cancel();
    resting = null;
    // Вращение вступило поверх — разгон больше не нужен.
    ramp.onfinish = () => {
      ramp.cancel();
      if (windup === ramp) windup = null;
    };
  };

  const stop = () => {
    if (!running) return;
    const time = Number(running.currentTime ?? 0);
    const inWindup = phase === "windup" && time < WINDUP_MS;
    const state = inWindup
      ? motionAt("windup", time)
      : motionAt(
          "spin",
          phase === "windup" ? time - WINDUP_MS : time,
          Number(running.effect?.getTiming?.().iterationStart ?? 0),
        );
    const angle = currentAngle() ?? (inWindup ? rest + state.angle : state.angle);
    const { to, duration, easing } = settle(angle, state.speed);
    rest = to % 360;
    if (!live()) {
      running.cancel();
      windup?.cancel();
      running = null;
      windup = null;
      rest = 0;
      return;
    }
    // Доворот держит последний кадр: значок остаётся ровно на `to`, даже если это
    // пол-оборота, и следующий разгон начнётся оттуда же.
    const settlingNow = node.animate([{ transform: `rotate(${angle}deg)` }, { transform: `rotate(${to}deg)` }], {
      duration,
      easing,
      fill: "forwards",
    });
    running.cancel();
    windup?.cancel();
    running = null;
    windup = null;
    settling = settlingNow;
    settlingNow.onfinish = () => {
      if (settling !== settlingNow) return;
      settling = null;
      resting = settlingNow;
    };
  };

  if (busy) start();
  return {
    update(next: boolean) {
      if (next) start();
      else stop();
    },
    destroy() {
      for (const animation of [running, windup, settling, resting]) animation?.cancel();
      running = windup = settling = resting = null;
    },
  };
}

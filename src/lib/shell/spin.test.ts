import { afterEach, describe, expect, it, vi } from "vitest";
import {
  MIN_COAST,
  REST_STEP,
  SETTLE_MAX_MS,
  SETTLE_MIN_MS,
  SPIN_PERIOD,
  SPIN_SPEED,
  WINDUP_EASING,
  WINDUP_MS,
  WINDUP_TURN,
  angleAt,
  angleFromMatrix,
  motionAt,
  settle,
  spinWhile,
} from "./spin";

/** Наклон кривой cubic-bezier в начале и в конце. */
function slopes(easing: string): { start: number; end: number } {
  const [x1, y1, x2, y2] = easing.match(/[\d.]+/g)!.map(Number) as [number, number, number, number];
  return { start: x1 === 0 ? Infinity : y1 / x1, end: (1 - y2) / (1 - x2) };
}

type Call = { keyframes: Keyframe[]; options: KeyframeAnimationOptions; animation: FakeAnimation };
type FakeAnimation = {
  currentTime: number;
  cancelled: boolean;
  onfinish: (() => void) | null;
  cancel(): void;
  effect: { getTiming(): { iterationStart: number } };
};

function fakeNode(time: number) {
  const calls: Call[] = [];
  const node = {
    animate(keyframes: Keyframe[], options: KeyframeAnimationOptions) {
      const animation: FakeAnimation = {
        currentTime: time,
        cancelled: false,
        onfinish: null,
        cancel() {
          this.cancelled = true;
        },
        effect: { getTiming: () => ({ iterationStart: Number(options.iterationStart ?? 0) }) },
      };
      calls.push({ keyframes, options, animation });
      return animation as unknown as Animation;
    },
  } as unknown as HTMLElement;
  return { node, calls };
}

const motionOn = () => vi.stubGlobal("document", { documentElement: { classList: { contains: () => false } } });

describe("значок крутится и плавно встаёт", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("угол по времени — внутри оборота", () => {
    expect(angleAt(0)).toBe(0);
    expect(angleAt(SPIN_PERIOD / 4)).toBe(90);
    expect(angleAt(SPIN_PERIOD * 3 + SPIN_PERIOD / 2)).toBe(180);
    expect(angleAt(-5)).toBe(0);
    expect(angleAt(Number.NaN)).toBe(0);
  });

  it("разгон кончается ровно на скорости вращения — без рывка", () => {
    const [, , x2, y2] = WINDUP_EASING;
    const endSlope = (1 - y2) / (1 - x2);
    expect(endSlope * (WINDUP_TURN / WINDUP_MS)).toBeCloseTo(SPIN_SPEED, 6);
    expect(WINDUP_EASING[1], "разгон с места").toBe(0);
  });

  it("встаёт вперёд, на ближайшей половине оборота, но не впритык", () => {
    expect(settle(90).to).toBe(180);
    expect(settle(200).to).toBe(360);
    expect(settle(170).to, "до 180 всего 10° — едем до следующей половины").toBe(360);
    expect(settle(0).to, "стоит на месте — пол-оборота, нажатие видно").toBe(180);
    expect(settle(450).to).toBe(540 - 360);
    expect(settle(-90).to).toBe(360);
    for (let angle = 0; angle < 360; angle += 7) {
      const { to } = settle(angle);
      expect(to % REST_STEP).toBe(0);
      expect(to - angle).toBeGreaterThanOrEqual(MIN_COAST);
      expect(to - angle).toBeLessThan(MIN_COAST + REST_STEP);
    }
  });

  it("на ходу — ровное торможение: начинает с той же скорости и только сбавляет", () => {
    for (let angle = 0; angle < 360; angle += 5) {
      const { to, duration, easing } = settle(angle, SPIN_SPEED);
      expect(duration).toBeGreaterThanOrEqual(SETTLE_MIN_MS);
      expect(duration).toBeLessThanOrEqual(SETTLE_MAX_MS);
      const wanted = (SPIN_SPEED * duration) / (to - angle);
      const { start, end } = slopes(easing);
      expect(start, `${angle}°`).toBeCloseTo(wanted, 2);
      // cubic-bezier(1/3, y1, 2/3, 1) не разгоняется при y1 ≥ 0,5 (наклон ≥ 1,5).
      expect(start, `${angle}°: без подхлёста в начале`).toBeGreaterThanOrEqual(1.5);
      expect(end, "в конце — полная остановка").toBe(0);
    }
  });

  it("с места — пол-оборота с мягким стартом и остановкой", () => {
    const quick = settle(0, 0);
    expect(quick.to).toBe(180);
    expect(quick.duration).toBe(SETTLE_MAX_MS);
    expect(slopes(quick.easing).start).toBe(0);
    expect(slopes(settle(90, -1).easing).start).toBe(0);
  });

  it("где значок в разгоне и на ходу", () => {
    expect(motionAt("windup", 0)).toEqual({ angle: 0, speed: 0 });
    expect(motionAt("windup", WINDUP_MS)).toEqual({ angle: WINDUP_TURN, speed: SPIN_SPEED });
    expect(motionAt("windup", WINDUP_MS * 5).angle).toBe(WINDUP_TURN);
    expect(motionAt("windup", Number.NaN).angle).toBe(0);
    expect(motionAt("spin", SPIN_PERIOD / 4, 0.5)).toEqual({ angle: 270, speed: SPIN_SPEED });
  });

  it("без Web Animations ничего не ломается", () => {
    const node = {} as HTMLElement;
    const action = spinWhile(node, true);
    action.update(false);
    action.update(true);
    action.destroy();
  });

  it("разгон и ход заводятся сразу, ход вступает без зазора", () => {
    motionOn();
    const { node, calls } = fakeNode(0);
    const action = spinWhile(node, false);
    expect(calls).toHaveLength(0);
    action.update(true);
    action.update(true);
    expect(calls, "повтор «занято» второй разгон не начинает").toHaveLength(2);
    const [ramp, spin] = calls as [Call, Call];
    expect(ramp.keyframes).toEqual([{ transform: "rotate(0deg)" }, { transform: `rotate(${WINDUP_TURN}deg)` }]);
    expect(ramp.options.fill, "разгон держит последний кадр").toBe("forwards");
    expect(spin.options.delay).toBe(WINDUP_MS);
    expect(spin.options.iterations).toBe(Infinity);
    expect(spin.options.iterationStart, "ход продолжает с конца разгона").toBe(0.5);
    ramp.animation.onfinish!();
    expect(ramp.animation.cancelled, "разгон снят, когда ход уже идёт").toBe(true);
    expect(spin.animation.cancelled).toBe(false);
    action.destroy();
  });

  it("на ходу: доворот с того же угла и держит последний кадр", () => {
    motionOn();
    // Время хода с задержкой: разгон + четверть оборота → 180 + 90 = 270°.
    const { node, calls } = fakeNode(WINDUP_MS + SPIN_PERIOD / 4);
    const action = spinWhile(node, true);
    action.update(false);
    expect(calls[1]!.animation.cancelled).toBe(true);
    const settling = calls[2]!;
    expect(settling.keyframes).toEqual([{ transform: "rotate(270deg)" }, { transform: "rotate(360deg)" }]);
    expect(settling.options.fill).toBe("forwards");
    expect(settling.options.easing).toBe(settle(270, SPIN_SPEED).easing);
    action.update(false);
    expect(calls, "уже стоит — второй раз не доворачиваем").toHaveLength(3);
    action.destroy();
    expect(settling.animation.cancelled).toBe(true);
  });

  it("встал на половине — следующий разгон оттуда, старая стоянка снята", () => {
    motionOn();
    const { node, calls } = fakeNode(WINDUP_MS + SPIN_PERIOD / 8); // 225°
    const action = spinWhile(node, true);
    action.update(false);
    const settling = calls[2]!;
    expect(settling.keyframes.at(-1)).toEqual({ transform: "rotate(360deg)" });
    settling.animation.onfinish!();
    action.update(true);
    expect(settling.animation.cancelled, "стоянку сняли после нового разгона").toBe(true);
    expect(calls[3]!.keyframes[0]).toEqual({ transform: "rotate(0deg)" });

    const half = fakeNode(WINDUP_MS + SPIN_PERIOD * 0.9); // 180 + 324 = 504 → 144°
    const second = spinWhile(half.node, true);
    second.update(false);
    expect(half.calls[2]!.keyframes).toEqual([{ transform: "rotate(144deg)" }, { transform: "rotate(360deg)" }]);
    const short = fakeNode(WINDUP_MS + SPIN_PERIOD * 0.6); // 180 + 216 = 396 → 36°
    const third = spinWhile(short.node, true);
    third.update(false);
    expect(short.calls[2]!.keyframes).toEqual([{ transform: "rotate(36deg)" }, { transform: "rotate(180deg)" }]);
    short.calls[2]!.animation.onfinish!();
    third.update(true);
    expect(short.calls[3]!.keyframes, "разгон с той же половины").toEqual([
      { transform: "rotate(180deg)" },
      { transform: `rotate(${180 + WINDUP_TURN}deg)` },
    ]);
    expect(short.calls[4]!.options.iterationStart).toBe(0);
  });

  it("остановка посреди разгона — тормозит с его скорости, а не с полной", () => {
    motionOn();
    const { node, calls } = fakeNode(WINDUP_MS / 2);
    const action = spinWhile(node, true);
    action.update(false);
    expect(calls[0]!.animation.cancelled).toBe(true);
    expect(calls[1]!.animation.cancelled).toBe(true);
    const settleCall = calls[2]!;
    expect(settleCall.keyframes[0]).toEqual({ transform: `rotate(${WINDUP_TURN / 4}deg)` });
    expect(settleCall.options.easing).toBe(settle(WINDUP_TURN / 4, SPIN_SPEED / 2).easing);
  });

  it("работа заняла миг — всё равно плавные пол-оборота", () => {
    motionOn();
    const { node, calls } = fakeNode(0);
    const action = spinWhile(node, true);
    action.update(false);
    expect(calls[2]!.keyframes).toEqual([{ transform: "rotate(0deg)" }, { transform: "rotate(180deg)" }]);
  });

  it("снова занято во время доворота — полный ход с текущего угла, без разгона", () => {
    motionOn();
    vi.stubGlobal("getComputedStyle", () => ({ transform: "matrix(0, 1, -1, 0, 0, 0)" }));
    const { node, calls } = fakeNode(100);
    const action = spinWhile(node, true);
    action.update(false);
    const settling = calls[2]!;
    action.update(true);
    expect(settling.animation.cancelled).toBe(true);
    const resumed = calls[3]!;
    expect(resumed.options.iterations).toBe(Infinity);
    expect(resumed.options.delay ?? 0).toBe(0);
    expect(resumed.options.iterationStart).toBe(0.25);
  });

  it("в режиме «Быстро» не крутится", () => {
    vi.stubGlobal("document", { documentElement: { classList: { contains: (name: string) => name === "no-motion" } } });
    let animated = false;
    const node = { animate: () => ((animated = true), {}) } as unknown as HTMLElement;
    spinWhile(node, true).destroy();
    expect(animated).toBe(false);
  });
});

describe("угол из transform", () => {
  it("читает поворот из матрицы", () => {
    expect(angleFromMatrix("matrix(1, 0, 0, 1, 0, 0)")).toBe(0);
    expect(angleFromMatrix("matrix(0, 1, -1, 0, 0, 0)")).toBe(90);
    expect(angleFromMatrix("matrix(-1, 0, 0, -1, 0, 0)")).toBe(180);
    expect(angleFromMatrix("matrix(0, -1, 1, 0, 0, 0)")).toBe(270);
  });

  it("нет поворота или мусор — неизвестно", () => {
    expect(angleFromMatrix("none")).toBeNull();
    expect(angleFromMatrix(null)).toBeNull();
    expect(angleFromMatrix("matrix(a, b)")).toBeNull();
  });
});

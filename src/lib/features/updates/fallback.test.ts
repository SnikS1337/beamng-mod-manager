import { describe, expect, it } from "vitest";
import type { UpdateAlternative, UpdateFinding } from "#lib/ipc/updates.js";
import { plural } from "#lib/features/garage/format.js";
import {
  applyBatch,
  askText,
  batchAskText,
  batchNeedsAsk,
  onAlternative,
  planAfterSearch,
  planBatch,
  planUpdate,
} from "./fallback";

const names: Record<string, string> = { modland: "Modland", stmods: "STMods", modhub: "ModHub" };
const siteName = (id: string) => names[id] ?? id;
const blocked = {
  note: "Не открывается из вашей сети: соединение обрывается, а другие сайты отвечают — похоже на блокировку. Попробуйте VPN.",
  reach: "reset" as const,
};

function alt(source: string, version: string | null = "1.3"): UpdateAlternative {
  return { source, url: `https://${source}.test/car`, latestVersion: version };
}

function finding(patch: Partial<UpdateFinding> = {}): UpdateFinding {
  return {
    modId: "car",
    modName: "Car",
    source: "modland",
    status: "likely",
    installedVersion: "1.0",
    latestVersion: "1.3",
    url: "https://modland.test/car",
    dismissed: false,
    previewUrl: null,
    role: "vehicle",
    installedSize: null,
    latestSize: null,
    noticedAt: null,
    authorVersion: null,
    fileVerdict: null,
    fileName: null,
    fileVersion: null,
    fileGone: false,
    siteDown: null,
    alternatives: [],
    ...patch,
  };
}

describe("обновление с сайта, который не открывается", () => {
  it("сайт открывается — сразу открываем, даже если есть другие сайты", () => {
    expect(planUpdate(finding({ alternatives: [alt("stmods")] }))).toEqual({ kind: "open" });
  });

  it("не открывается, а обновление есть на другом сайте — спрашиваем про первый из них", () => {
    const plan = planUpdate(finding({ siteDown: blocked, alternatives: [alt("stmods"), alt("modhub")] }));
    expect(plan).toEqual({ kind: "askAlternative", alternative: alt("stmods") });
    const text = askText(finding({ siteDown: blocked }), plan, siteName);
    expect(text.title).toBe("Modland не открывается");
    expect(text.note).toContain("похоже на блокировку");
    expect(text.offer).toBe("Это обновление есть на STMods — версия 1.3. Скачать оттуда?");
  });

  it("на другом сайте другая версия — так и говорим, а не «это обновление»", () => {
    const plan = planUpdate(finding({ siteDown: blocked, alternatives: [alt("stmods", "1.2")] }));
    expect(askText(finding({ siteDown: blocked }), plan, siteName).offer).toBe(
      "На STMods есть версия 1.2 (здесь — 1.3). Скачать оттуда?",
    );
  });

  it("версия на другом сайте не подписана — без неё", () => {
    const plan = planUpdate(finding({ siteDown: blocked, alternatives: [alt("stmods", null)] }));
    expect(askText(finding({ siteDown: blocked }), plan, siteName).offer).toBe(
      "Это обновление есть на STMods. Скачать оттуда?",
    );
  });

  it("другого сайта нет — предлагаем поискать, а не нашли — так и пишем", () => {
    const down = finding({ siteDown: blocked });
    expect(planUpdate(down)).toEqual({ kind: "askSearch" });
    expect(askText(down, { kind: "askSearch" }, siteName).offer).toContain("Поискать?");
    expect(planAfterSearch(down)).toEqual({ kind: "notFound" });
    expect(askText(down, { kind: "notFound" }, siteName).offer).toBe(
      "На других сайтах это обновление не нашлось.",
    );
    const found = finding({ siteDown: blocked, alternatives: [alt("stmods")] });
    expect(planAfterSearch(found)).toEqual({ kind: "askAlternative", alternative: alt("stmods") });
  });

  it("пока искали, обновление пропало или сайт открылся — просто открываем", () => {
    expect(planAfterSearch(undefined)).toEqual({ kind: "open" });
    expect(planAfterSearch(finding())).toEqual({ kind: "open" });
  });

  it("с другого сайта — его страница и его версия, остальное прежнее", () => {
    const original = finding({ siteDown: blocked, alternatives: [alt("stmods", "1.4")] });
    const moved = onAlternative(original, alt("stmods", "1.4"));
    expect(moved).toMatchObject({
      modId: "car",
      source: "stmods",
      url: "https://stmods.test/car",
      latestVersion: "1.4",
      installedVersion: "1.0",
      siteDown: null,
      alternatives: [],
    });
    expect(onAlternative(original, alt("stmods", null)).latestVersion).toBe("1.3");
    expect(original.source).toBe("modland");
  });
});

describe("«Обновить все» с недоступными сайтами", () => {
  const ok = finding({ modId: "a", source: "stmods", url: "https://stmods.test/a" });
  const swap = finding({ modId: "b", siteDown: blocked, alternatives: [alt("stmods")] });
  const stuck = finding({ modId: "c", siteDown: blocked });

  it("делит моды: с запасным сайтом и без", () => {
    const plan = planBatch([ok, swap, stuck]);
    expect(plan.swap.map((item) => item.finding.modId)).toEqual(["b"]);
    expect(plan.stuck.map((item) => item.modId)).toEqual(["c"]);
    expect(batchNeedsAsk(plan)).toBe(true);
    expect(batchNeedsAsk(planBatch([ok, stuck])), "предложить нечего — не спрашиваем").toBe(false);
    expect(batchNeedsAsk(planBatch([]))).toBe(false);
  });

  it("согласился — меняем страницы только тем, у кого есть запасной сайт", () => {
    const list = [ok, swap, stuck];
    const plan = planBatch(list);
    const used = applyBatch(list, plan, true);
    expect(used.map((item) => item.url)).toEqual([ok.url, "https://stmods.test/car", stuck.url]);
    expect(applyBatch(list, plan, false)).toBe(list);
  });

  it("текст вопроса считает моды", () => {
    const plan = planBatch([ok, swap, stuck]);
    const text = batchAskText(plan, plural);
    expect(text).toContain("1 мод — на сайтах, которые не открываются");
    expect(text).toContain("Ещё 1 мод есть только на недоступных сайтах");
    const many = planBatch([swap, { ...swap, modId: "d" }, { ...swap, modId: "e" }]);
    expect(batchAskText(many, plural)).toContain("3 мода");
    expect(batchAskText(many, plural)).not.toContain("Ещё");
  });
});

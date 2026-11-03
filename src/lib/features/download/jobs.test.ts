import { describe, expect, it } from "vitest";
import type { DownloadJob, DownloadRequest } from "#lib/ipc/download.js";
import { chipText, closeQuestion, jobForPage, jobRows, restarted, track, viewOf } from "./jobs";

function request(name: string): DownloadRequest {
  return {
    modName: name,
    installedVersion: null,
    latestVersion: null,
    pageUrl: `https://stmods.org/mods/${name}/`,
    previewUrl: null,
    role: null,
  };
}

function job(name: string, state: DownloadJob["state"] = "downloading", error: string | null = null): DownloadJob {
  return { path: `C:/Downloads/${name}.zip`, fileName: `${name}.zip`, request: request(name), state, error };
}

const progress = (path: string, received: number, total: number | null = 1000) => ({
  path,
  received,
  total,
  speed: null,
});

describe("несколько загрузок сразу", () => {
  it("ход каждой — в свою строку, полоска не прыгает", () => {
    let known = track({}, progress("a.zip", 100), 1000);
    known = track(known, progress("b.zip", 900), 1100);
    known = track(known, progress("a.zip", 200), 1200);
    expect(known["a.zip"].progress.received).toBe(200);
    expect(known["b.zip"].progress.received).toBe(900);
    expect(known["a.zip"].changedAt).toBe(1200);
    // Те же байты — время движения не сдвигается: стоящая загрузка видна.
    known = track(known, progress("b.zip", 900), 5000);
    expect(known["b.zip"].changedAt).toBe(1100);
  });

  it("в главной полоске — загрузка открытой страницы", () => {
    const jobs = [job("first"), job("second")];
    expect(jobForPage(jobs, request("second").pageUrl)?.path).toBe(jobs[1].path);
    expect(jobForPage(jobs, request("third").pageUrl)).toBeNull();
    expect(jobForPage([job("first", "done")], request("first").pageUrl)).toBeNull();
    expect(jobForPage(jobs, null)).toBeNull();
  });

  it("процент считается по своему файлу", () => {
    const first = job("first");
    const known = track({}, progress(first.path, 500, 1000), 0);
    expect(viewOf(first, known, 0)?.percent).toBe(50);
    expect(viewOf(job("second"), known, 0)).toBeNull();
  });
});

describe("список загрузок в окне", () => {
  const options = (over: Partial<Parameters<typeof jobRows>[1]> = {}) => ({
    currentPage: request("second").pageUrl,
    reviewing: null,
    queued: [],
    seen: new Set<string>(),
    ...over,
  });

  it("первый мод качается, пока открыт второй — первый виден в списке", () => {
    const first = job("first");
    const second = job("second");
    const rows = jobRows([first, second], options());
    expect(rows.map((row) => row.job.path)).toEqual([first.path]);
    expect(rows[0].text).toBe("Качается");
  });

  it("пока проверяем архив, загрузка открытой страницы тоже в списке", () => {
    const first = job("first", "done");
    const second = job("second");
    const rows = jobRows([first, second], options({ reviewing: first.path }));
    expect(rows.map((row) => row.job.path)).toEqual([second.path]);
  });

  it("докачанный ждёт своей очереди, оборванный показывает причину", () => {
    const waiting = job("waiting", "done");
    const reviewed = job("reviewed", "done");
    const broken = job("broken", "failed", "Загрузка зависла");
    const shown = { ...job("second", "failed", "Сайт не отдал файл."), path: "C:/Downloads/here.zip" };
    const rows = jobRows(
      [waiting, reviewed, broken, shown],
      options({ queued: [waiting.path], seen: new Set([shown.path]) }),
    );
    expect(rows.map((row) => row.text)).toEqual([
      "Скачан — проверим следом",
      "Не скачался: Загрузка зависла",
    ]);
  });

  it("обрыв открытой страницы во время проверки другого архива не теряется", () => {
    // В шаги он не попал (там архив) — значит, виден в списке.
    const here = job("second", "failed", "Загрузка зависла");
    const rows = jobRows([here], options({ reviewing: "C:/Downloads/other.zip" }));
    expect(rows.map((row) => row.text)).toEqual(["Не скачался: Загрузка зависла"]);
  });
});

describe("закрытие при идущей загрузке", () => {
  it("терять нечего — не спрашиваем", () => {
    expect(closeQuestion([job("a", "done")], [])).toBeNull();
    expect(closeQuestion([], [])).toBeNull();
  });

  it("называет, что оборвётся и что останется неустановленным", () => {
    expect(closeQuestion([job("Dummy")], [])).toBe(
      "Ещё качается «Dummy» — загрузка оборвётся, файл удалится.",
    );
    expect(closeQuestion([job("a"), job("b")], [job("c", "done")])).toBe(
      "Ещё качаются 2 мода — загрузки оборвутся, файлы удалятся. «c» скачан, но не установлен — архив останется в папке загрузок.",
    );
  });
});

describe("плашка главного окна", () => {
  it("в главном окне видно, что и сколько качается", () => {
    const one = job("Dummy");
    expect(chipText([], {}, 0)).toBeNull();
    expect(chipText([job("a", "done")], {}, 0)).toBeNull();
    expect(chipText([one], {}, 0)).toBe("Скачивается «Dummy»");
    expect(chipText([one], track({}, progress(one.path, 420, 1000), 0), 0)).toBe(
      "Скачивается «Dummy» · 42 %",
    );
    expect(chipText([one, job("b"), job("c")], {}, 0)).toBe("Скачиваются 3 мода");
  });
});

describe("тот же файл скачали снова", () => {
  it("новая загрузка под прежним именем — снова новая", () => {
    const old = job("Dummy", "done");
    const again = job("Dummy");
    expect(restarted([old], [again])).toEqual([again.path]);
    expect(restarted([again], [again]), "та же загрузка идёт дальше").toEqual([]);
    expect(restarted([], [again])).toEqual([again.path]);
    expect(restarted([again], [{ ...again, state: "done" }])).toEqual([]);
  });
});

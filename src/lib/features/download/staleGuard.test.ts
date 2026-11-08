// @ts-expect-error — тесты идут в Node, а типов Node в проекте нет.
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { StaleGuard } from "./staleGuard";

describe("проверка архива и смена мода", () => {
  it("новая проверка вытесняет старую", () => {
    const guard = new StaleGuard();
    const first = guard.begin();
    const second = guard.begin();
    expect(guard.isCurrent(first)).toBe(false);
    expect(guard.isCurrent(second)).toBe(true);
  });
});

describe("окно скачивания", () => {
  const page: string = readFileSync(
    new URL("../../../routes/download/+page.svelte", import.meta.url),
    "utf8",
  );

  it("смена мода не трогает загрузку и проверку прошлого", () => {
    // Было: «Обновить» у второго мода сбрасывал проверку и ход первого, архив
    // первого терялся, а окно закрывалось после установки второго.
    const handler = page.slice(page.indexOf("listenDownloadContext((value) =>"));
    const body = handler.slice(0, handler.indexOf("})"));
    expect(body).not.toContain("checks.begin()");
    expect(body).not.toContain("resetArchive");
  });

  it("архив ставится для своего мода, ход — по своему файлу", () => {
    expect(page).toContain("installRequests([{ request: reviewed, archive, selected }])");
    expect(page).toContain("const owner = job.request ?? context;");
    expect(page).toContain("tracked = track(tracked, value, Date.now())");
  });

  it("после установки окно закрывается, только если больше ничего не качается", () => {
    const install = page.slice(page.indexOf("async function install()"));
    expect(install.slice(0, install.indexOf("} catch"))).toContain("await finishReview(false)");
    const finish = page.slice(page.indexOf("async function finishReview("));
    const body = finish.slice(0, finish.indexOf("\n  }\n"));
    expect(body.indexOf("downloading(jobs).length > 0")).toBeLessThan(body.indexOf("await close()"));
  });

  it("закрытие при идущей загрузке сначала спрашивает", () => {
    const close = page.slice(page.indexOf("async function close()"));
    expect(close.slice(0, close.indexOf("await closeNow()"))).toContain("closeQuestion(jobs, queue)");
    expect(page).toContain("listenDownloadCloseAsked(");
  });
});

describe("окно скачивания для экранного диктора", () => {
  it("иконки скрыты от диктора, адрес — в своей области", () => {
    const page: string = readFileSync(
      new URL("../../../routes/download/+page.svelte", import.meta.url),
      "utf8",
    );
    const icons = page.match(/<[A-Z][A-Za-z]* size=\{[^>]*\/>/g) ?? [];
    expect(icons.length).toBeGreaterThan(0);
    for (const icon of icons) {
      // Своя галочка прячется от диктора сама — проверяем в её файле.
      if (icon.startsWith("<DrawCheck")) continue;
      expect(icon, icon).toContain('aria-hidden="true"');
    }
    const check: string = readFileSync(new URL("../../shell/DrawCheck.svelte", import.meta.url), "utf8");
    expect(check).toMatch(/<svg[^>]*aria-hidden="true"/);
    expect(page).toMatch(/class="urlbar" role="region" aria-label="Адрес страницы"/);
  });
});

describe("окно скачивания знает, какой мод обновляет", () => {
  it("«Обновления» передают мод, а окно умеет спрятать ту же версию", () => {
    const updates: string = readFileSync(
      new URL("../../../routes/(app)/updates/+page.svelte", import.meta.url),
      "utf8",
    );
    expect(updates).toContain("modId: finding.modId");
    const page: string = readFileSync(
      new URL("../../../routes/download/+page.svelte", import.meta.url),
      "utf8",
    );
    expect(page).toContain("sameAsInstalled(found)");
    expect(page).toContain("updatesDismiss(request.modId)");
    expect(page).toContain('emit("download://dismissed"');
  });
});

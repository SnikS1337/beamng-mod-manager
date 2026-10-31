import { describe, expect, it } from "vitest";
import { findPageText } from "./findPageText";

const base = { suggested: false, followed: false, name: "Haleakalā (Maui)", skippedSites: [] };

describe("итог «Найти на сайтах»", () => {
  it("называет сайты, на которых не искали", () => {
    expect(findPageText({ ...base, skippedSites: ["Modland", "ModHub"] })).toBe(
      "Страницу не нашли. Modland и ModHub не ответили — там не искали: пройдите проверку в «Обновлениях» и нажмите ещё раз",
    );
    expect(findPageText({ ...base, skippedSites: ["ModHub"] })).toContain("ModHub не ответил —");
  });

  it("без пропущенных сайтов — просто не нашли", () => {
    expect(findPageText(base)).toBe("Страницу найти не удалось");
  });

  it("нашли или предложили — об этом и говорит", () => {
    expect(findPageText({ ...base, suggested: true, skippedSites: ["Modland"] })).toContain(
      "похожая страница",
    );
    expect(findPageText({ ...base, followed: true })).toBe("Нашли страницу «Haleakalā (Maui)»");
  });
});

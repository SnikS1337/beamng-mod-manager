import { describe, expect, it } from "vitest";
import { DROP_NAMES, dropPlan, dropRefusal, dropSummary } from "./drop";

describe("что брошено в окно", () => {
  it("архивы — к проверке, остальное — мимо", () => {
    const plan = dropPlan(["C:\\m\\car.zip", "C:\\m\\notes.txt", "C:\\m\\folder", "C:\\m\\map.7z"], null, []);
    expect(plan).toEqual({ add: ["C:\\m\\car.zip", "C:\\m\\map.7z"], skipped: 2, repeats: 0 });
  });

  it("один файл дважды и в другом регистре — один раз", () => {
    const plan = dropPlan(["C:\\m\\Car.ZIP", "c:/m/car.zip", "C:\\m\\car.zip"], null, []);
    expect(plan.add).toEqual(["C:\\m\\Car.ZIP"]);
    expect(plan.repeats).toBe(0);
  });

  it("уже открытый и ждущий в очереди архив второй раз не берём", () => {
    const plan = dropPlan(["C:\\m\\open.zip", "C:\\m\\wait.rar", "C:\\m\\new.zip"], "c:/m/OPEN.zip", ["C:\\m\\wait.rar"]);
    expect(plan).toEqual({ add: ["C:\\m\\new.zip"], skipped: 0, repeats: 2 });
  });

  it("пустые пути и скрытые файлы не архивы", () => {
    const plan = dropPlan(["", "C:\\m\\.zip"], null, []);
    expect(plan).toEqual({ add: [], skipped: 1, repeats: 0 });
  });

  it("ничего не брошено — ничего не берём и не ругаемся", () => {
    const plan = dropPlan([], null, []);
    expect(plan).toEqual({ add: [], skipped: 0, repeats: 0 });
    expect(dropRefusal(plan)).toBeNull();
  });
});

describe("подсказка, пока несут", () => {
  it("один архив — его имя", () => {
    const summary = dropSummary(dropPlan(["C:\\m\\rally car.zip"], null, []), false);
    expect(summary).toEqual({
      tone: "ok",
      title: "Отпустите — проверим и предложим установить",
      names: ["rally car.zip"],
      more: 0,
      note: null,
    });
  });

  it("много архивов — первые имена и «ещё N»", () => {
    const paths = Array.from({ length: 7 }, (_, index) => `C:\\m\\mod${index}.zip`);
    const summary = dropSummary(dropPlan(paths, null, []), false);
    expect(summary.title).toBe("Отпустите — проверим 7 архивов");
    expect(summary.names).toHaveLength(DROP_NAMES);
    expect(summary.more).toBe(7 - DROP_NAMES);
  });

  it("окно уже открыто — проверим после текущего", () => {
    expect(dropSummary(dropPlan(["C:\\a.zip"], null, []), true).title).toBe(
      "Отпустите — проверим после текущего",
    );
    expect(dropSummary(dropPlan(["C:\\a.zip", "C:\\b.zip"], null, []), true)).toMatchObject({
      tone: "queue",
      title: "Отпустите — проверим 2 архива после текущего",
    });
  });

  it("вперемешку с не-архивами — говорим, что пропустим", () => {
    const summary = dropSummary(dropPlan(["C:\\a.zip", "C:\\b.txt", "C:\\c.png"], null, []), false);
    expect(summary.note).toBe("2 файла — не архив, пропустим");
  });

  it("повтор рядом с новым — тоже говорим", () => {
    const summary = dropSummary(dropPlan(["C:\\a.zip", "C:\\b.zip"], "C:\\a.zip", []), false);
    expect(summary.names).toEqual(["b.zip"]);
    expect(summary.note).toBe("1 уже открыт или ждёт — второй раз не добавим");
  });

  it("брать нечего — честно, а не молча", () => {
    expect(dropSummary(dropPlan(["C:\\notes.txt"], null, []), false)).toMatchObject({
      tone: "none",
      title: "Это не архив",
      note: "Нужен zip, rar или 7z",
    });
    expect(dropSummary(dropPlan(["C:\\a.txt", "C:\\b"], null, []), false).title).toBe("Здесь нет архивов");
    expect(dropSummary(dropPlan(["C:\\a.zip"], "C:\\a.zip", []), false).title).toBe(
      "Этот архив уже открыт или ждёт очереди",
    );
    expect(dropRefusal(dropPlan(["C:\\notes.txt"], null, []))).toContain("Это не архив");
    expect(dropRefusal(dropPlan(["C:\\a.zip"], "C:\\a.zip", []))).toContain("уже открыт");
    expect(dropRefusal(dropPlan(["C:\\a.zip"], null, []))).toBeNull();
  });

  it("имён не знаем (браузер) — общий текст", () => {
    expect(dropSummary(null, false)).toMatchObject({ tone: "ok", names: [], note: "zip, rar или 7z" });
    expect(dropSummary(null, true).tone).toBe("queue");
  });
});

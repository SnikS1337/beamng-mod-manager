import { describe, expect, it } from "vitest";
import { addonCaption, compatNote, versionLine, cardActions, toggleMessage } from "./cardText";

const compat = (over: Partial<{ modFor: string | null; game: string | null; differs: boolean }>) => ({
  compat: { modFor: "0.38", game: "0.40", text: null, differs: true, ...over },
});

describe("versionLine", () => {
  it("обычная версия и без версии", () => {
    expect(versionLine({ version: "1.2", update: "none", updateVersion: null })).toEqual({ text: "1.2", fresh: false });
    expect(versionLine({ version: null, update: "none", updateVersion: null })).toBeNull();
    expect(versionLine({ version: "  ", update: "none", updateVersion: null })).toBeNull();
  });

  it("на сайте новее — стрелка", () => {
    expect(versionLine({ version: "1.0", update: "likely", updateVersion: "1.2" })).toEqual({
      text: "1.0 → 1.2",
      fresh: true,
    });
    expect(versionLine({ version: null, update: "possible", updateVersion: "2.0" })?.text).toBe("→ 2.0");
  });

  it("та же версия на сайте или проверка снята — без стрелки", () => {
    expect(versionLine({ version: "1.2", update: "likely", updateVersion: "1.2" })?.fresh).toBe(false);
    expect(versionLine({ version: "1.0", update: "none", updateVersion: "1.2" })?.text).toBe("1.0");
  });
});

describe("compatNote", () => {
  it("«для 0.38» с объяснением", () => {
    expect(compatNote(compat({}))).toEqual({ text: "для 0.38", title: "Сделан для игры 0.38, у вас 0.40" });
    expect(compatNote(compat({ game: null }))?.title).toBe("Сделан для игры 0.38");
  });

  it("совпадает или неизвестно — ничего", () => {
    expect(compatNote(compat({ differs: false }))).toBeNull();
    expect(compatNote(compat({ modFor: null }))).toBeNull();
  });
});

describe("addonCaption", () => {
  it("не повторяет метку типа", () => {
    expect(addonCaption({ kind: "sound" })).toBeNull();
    expect(addonCaption({ kind: "script" })).toBeNull();
    expect(addonCaption({ kind: "unknown" })).toBeNull();
  });

  it("говорит то, чего нет в метке", () => {
    expect(addonCaption({ kind: "vehicle" })).toBe("Для машин игры");
    expect(addonCaption({ kind: "level" })).toBe("Для карт игры");
  });
});

describe("быстрые действия на карточке", () => {
  it("есть обновление — «Обновить» и сайт", () => {
    expect(cardActions({ deleted: false, hasPage: true, update: "likely" })).toEqual({ update: true, page: true });
    expect(cardActions({ deleted: false, hasPage: true, update: "possible" })).toEqual({ update: true, page: true });
  });

  it("обновления нет — только сайт", () => {
    expect(cardActions({ deleted: false, hasPage: true, update: "none" })).toEqual({ update: false, page: true });
  });

  it("без страницы и у удалённых — ничего", () => {
    expect(cardActions({ deleted: false, hasPage: false, update: "likely" })).toEqual({ update: false, page: false });
    expect(cardActions({ deleted: true, hasPage: true, update: "likely" })).toEqual({ update: false, page: false });
  });
});

describe("тост после переключения", () => {
  it("две копии — главное, даже если программа ответила своим текстом", () => {
    expect(toggleMessage("Car", true, true, "Включён.")).toBe(
      "Включён «Car» — и другая его копия тоже: игра загрузит обе",
    );
  });

  it("иначе — слова программы, а без них — своё", () => {
    expect(toggleMessage("Car", true, false, "Игра занята: изменение отправлено")).toBe(
      "Игра занята: изменение отправлено",
    );
    expect(toggleMessage("Car", true, false, null)).toBe("Включён «Car»");
    expect(toggleMessage("Car", false, false, undefined)).toBe("Выключен «Car»");
  });

  it("выключение не говорит о двух копиях", () => {
    expect(toggleMessage("Car", false, true, null)).toBe("Выключен «Car»");
  });
});

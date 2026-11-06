import { describe, expect, it } from "vitest";
import { errorText } from "./errorText";

describe("текст ошибки", () => {
  it("строка от бэкенда — как есть", () => {
    expect(errorText("Папка модов не найдена.")).toBe("Папка модов не найдена.");
  });

  it("ошибка программы и AppError — их сообщение", () => {
    expect(errorText(new Error("сеть"))).toBe("сеть");
    expect(errorText({ code: "folder", message: "Нет папки" })).toBe("Нет папки");
  });

  it("пустое или непонятное — запасной текст", () => {
    expect(errorText(undefined)).toBe("Не удалось выполнить действие.");
    expect(errorText(null, "Не удалось выполнить поиск.")).toBe("Не удалось выполнить поиск.");
    expect(errorText(42)).toBe("Не удалось выполнить действие.");
    expect(errorText("")).toBe("Не удалось выполнить действие.");
    expect(errorText({ message: "  " }, "Не удалось проверить архив.")).toBe(
      "Не удалось проверить архив.",
    );
    expect(errorText({ message: null })).toBe("Не удалось выполнить действие.");
  });
});

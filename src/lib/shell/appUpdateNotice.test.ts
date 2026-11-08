import { describe, expect, it } from "vitest";
import type { AppUpdateSnapshot } from "#lib/ipc/appUpdate.js";
import { noteLines, noticeFor, notesPreview, releaseNoteItems } from "./appUpdateNotice";

function snapshot(stage: AppUpdateSnapshot["state"]["stage"], progress = 0): AppUpdateSnapshot {
  return {
    currentVersion: "1.2",
    state: {
      stage,
      manifest: {
        version: "1.3",
        url: "https://example.test/a.zip",
        size: 1,
        sha256: "x",
        notes: "- Первое\n- Второе\n- Третье",
        published: null,
      },
      progress,
      error: stage === "failed" ? "Сеть недоступна" : null,
      checkedAt: null,
    },
  };
}

describe("плашка обновления программы", () => {
  it("появляется, когда есть новая версия", () => {
    expect(noticeFor(snapshot("available"), null)?.stage).toBe("available");
  });

  it("«Позже» прячет эту версию до следующего запуска", () => {
    expect(noticeFor(snapshot("available"), "1.3")).toBeNull();
    // И скачанную, готовую к перезапуску: кнопка «Позже» раньше ничего не делала.
    expect(noticeFor(snapshot("readyToRestart"), "1.3")).toBeNull();
  });

  it("ведёт загрузку и перезапуск", () => {
    expect(noticeFor(snapshot("downloading", 0.4), "1.3")).toMatchObject({
      stage: "downloading",
      progress: 0.4,
    });
    expect(noticeFor(snapshot("readyToRestart"), null)?.stage).toBe("ready");
  });

  it("молчит, когда обновлений нет", () => {
    expect(noticeFor(snapshot("upToDate"), null)).toBeNull();
    expect(noticeFor(null, null)).toBeNull();
  });

  it("берёт из заметок пару строк", () => {
    expect(notesPreview("- Первое\n- Второе\n- Третье")).toEqual(["Первое", "Второе"]);
  });
});

describe("заметки к выпуску с подзаголовками", () => {
  const notes = "### Новое\n\n- Мод прилетает\n- Обложка в панель\n\n### Исправлено\n\n- Поиск\r\n* Звёздочка";

  it("подзаголовки — не пункты", () => {
    expect(noteLines(notes)).toEqual(["Мод прилетает", "Обложка в панель", "Поиск", "Звёздочка"]);
    expect(notesPreview(notes)).toEqual(["Мод прилетает", "Обложка в панель"]);
  });

  it("«Что нового»: все пункты; старый формат в одну строку тоже делится", () => {
    expect(releaseNoteItems(notes)).toHaveLength(4);
    expect(releaseNoteItems("- первое - второе - третье")).toEqual(["первое", "второе", "третье"]);
    expect(releaseNoteItems("Одна строка без пунктов")).toEqual(["Одна строка без пунктов"]);
    expect(releaseNoteItems("   ")).toEqual([]);
    expect(releaseNoteItems(null)).toEqual([]);
    expect(releaseNoteItems("### Только заголовок")).toEqual([]);
  });
});

import { describe, expect, it } from "vitest";
import type { UpdateFinding } from "#lib/ipc/updates.js";
import { reason } from "./reason";

const MB = 1024 * 1024;

function finding(patch: Partial<UpdateFinding>): UpdateFinding {
  return {
    modId: "m",
    modName: "M",
    source: "stmods",
    status: "possible",
    installedVersion: null,
    latestVersion: null,
    url: "https://stmods.org/x/",
    dismissed: false,
    previewUrl: null,
    role: null,
    installedSize: null,
    latestSize: null,
    noticedAt: null,
    authorVersion: null,
    fileVerdict: null,
    fileName: null,
    fileVersion: null,
    ...patch,
  };
}

describe("причина находки", () => {
  it("версию у себя не знаем — не пишем «версия та же»", () => {
    // McLaren7SportSeries_Frix.zip: в имени файла версии нет, на сайте 1.1.
    const text = reason(finding({ latestVersion: "1.1", installedSize: 633 * MB, latestSize: 647 * MB }));
    expect(text).toContain("Версия у вас не указана");
    expect(text).not.toContain("Версия та же");
  });

  it("та же версия и другой размер — перезалив", () => {
    const text = reason(
      finding({ installedVersion: "1.0", latestVersion: "1.0", installedSize: 100 * MB, latestSize: 120 * MB }),
    );
    expect(text).toContain("Версия та же");
  });

  it("у нас alpha, на сайте без пометки", () => {
    const text = reason(finding({ installedVersion: "2.8.0-alpha2", latestVersion: "2.8.0" }));
    expect(text).toContain("2.8.0-alpha2");
    expect(text).toContain("финальная");
  });

  it("версия выше — обновление", () => {
    expect(reason(finding({ status: "likely", installedVersion: "1.0", latestVersion: "1.1" }))).toBe(
      "На сайте вышла новая версия.",
    );
    expect(reason(finding({ status: "likely", installedVersion: null, latestVersion: "0.1" }))).toBe(
      "На сайте вышла версия новее вашего файла.",
    );
  });

  it("номер перезалива выше, чем у автора, — так и говорим", () => {
    // Smoke Dust FX: ModsGaming «1.14.1», у автора на BeamNG 0.14.1.
    const text = reason(
      finding({ source: "modsgaming", installedVersion: "0.14.1", latestVersion: "1.14.1", authorVersion: "0.14.1" }),
    );
    expect(text).toContain("у автора на BeamNG последняя — 0.14.1");
    expect(text).toContain("1.14.1");
  });
});

describe("причина по файлу за кнопкой «Скачать»", () => {
  it("тот же файл, что у вас", () => {
    const text = reason(
      finding({ latestVersion: "1.14.1", fileVerdict: "same_file", fileName: "smoke_dust_fx.zip" }),
    );
    expect(text).toContain("«smoke_dust_fx.zip»");
    expect(text).toContain("тот же, что у вас");
    expect(text).toContain("1.14.1");
  });

  it("сайт пишет новую версию, а файл подписан старой", () => {
    const text = reason(
      finding({
        status: "possible",
        latestVersion: "1.2",
        fileVerdict: "label_older",
        fileName: "Dummy_v1.1.zip",
        fileVersion: "1.1",
      }),
    );
    expect(text).toContain("Сайт пишет 1.2");
    expect(text).toContain("подписан 1.1");
    expect(text).toContain("«Dummy_v1.1.zip»");
  });

  it("подпись прежняя, а файл новее", () => {
    const text = reason(
      finding({ latestVersion: "1.1", fileVerdict: "file_newer", fileName: "Dummy_v1.2.zip", fileVersion: "1.2" }),
    );
    expect(text).toContain("версия 1.2");
    expect(text).toContain("не меняя подписи");
  });

  it("файл важнее пояснения про автора: он говорит о том, что лежит на самом деле", () => {
    const text = reason(
      finding({
        latestVersion: "1.14.1",
        authorVersion: "0.14.1",
        fileVerdict: "same_file",
        fileName: "smoke.zip",
      }),
    );
    expect(text).toContain("тот же, что у вас");
  });
});

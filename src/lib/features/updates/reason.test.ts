import { describe, expect, it } from "vitest";
import type { UpdateFinding } from "$lib/ipc/updates";
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
      "Версия на сайте выше установленной.",
    );
  });
});

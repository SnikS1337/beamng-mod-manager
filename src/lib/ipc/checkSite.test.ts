import { describe, expect, it } from "vitest";
import { checkSiteOf } from "./updates";

describe("какой сайт просит проверку", () => {
  it("узнаёт ModHub по адресу страницы", () => {
    expect(checkSiteOf("https://www.modhub.us/beamng-drive-mods/bmw-m8-cs-039")).toBe("modhub");
  });

  it("остальное — Modland, как раньше", () => {
    expect(checkSiteOf("https://www.modland.net/beamng.drive-mods/cars/x.html")).toBe("modland");
    expect(checkSiteOf("не адрес")).toBe("modland");
  });
});

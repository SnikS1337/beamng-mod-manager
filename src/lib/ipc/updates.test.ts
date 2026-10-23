import { describe, expect, it } from "vitest";
import { updatesDismiss, updatesRestore, updatesSnapshot } from "./updates";

describe("заглушка обновлений без Tauri", () => {
  it("скрыть и вернуть обновление, не теряя сайты и время проверки", async () => {
    const before = await updatesSnapshot();
    const first = before.findings[0];
    const hidden = await updatesDismiss(first.modId);
    expect(hidden.findings.some((item) => item.modId === first.modId)).toBe(false);
    expect(hidden.hidden.some((item) => item.modId === first.modId && item.dismissed)).toBe(true);
    expect(hidden.sites?.length).toBe(before.sites?.length);
    expect(hidden.lastChecked).toBe(before.lastChecked);

    const back = await updatesRestore(first.modId);
    expect(back.findings.some((item) => item.modId === first.modId && !item.dismissed)).toBe(true);
    expect(back.sites).toEqual(before.sites);
  });

  it("снимок — копия: правка снаружи не меняет данные", async () => {
    const one = await updatesSnapshot();
    one.findings[0].modName = "испорчено";
    one.sites?.splice(0);
    const two = await updatesSnapshot();
    expect(two.findings[0].modName).not.toBe("испорчено");
    expect(two.sites?.length).toBeGreaterThan(0);
  });
});

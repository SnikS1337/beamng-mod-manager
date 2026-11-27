import { describe, expect, it } from "vitest";

import {
  appUpdateDownload,
  applyAppUpdateProgress,
  applyAppUpdateSnapshot,
  listenAppUpdateProgress,
  listenAppUpdateState,
  type AppUpdateSnapshot,
} from "./appUpdate.js";

function snapshot(revision: number, stage: AppUpdateSnapshot["state"]["stage"] = "available") {
  return {
    currentVersion: "1.4.5",
    revision,
    state: {
      stage,
      manifest: null,
      progress: 0,
      error: null,
      checkedAt: null,
    },
  } satisfies AppUpdateSnapshot;
}

describe("app update ordering", () => {
  it("не позволяет старому снимку перезаписать новый", () => {
    const current = snapshot(8, "readyToRestart");
    const stale = snapshot(7, "available");

    expect(applyAppUpdateSnapshot(current, stale)).toBe(current);
    expect(applyAppUpdateSnapshot(current, snapshot(8, "downloading")).revision).toBe(8);
    expect(applyAppUpdateSnapshot(current, snapshot(9, "failed")).revision).toBe(9);
  });

  it("принимает прогресс только от текущей ревизии", () => {
    const current = snapshot(8, "downloading");

    expect(applyAppUpdateProgress(current, 0.9, 7)).toBe(current);
    expect(applyAppUpdateProgress(current, 0.9, 8)?.state.progress).toBe(0.9);
    expect(applyAppUpdateProgress(null, 0.9, 8)).toBeNull();
  });

  it("demo сообщает ревизию загрузки до прогресса", async () => {
    const states: AppUpdateSnapshot[] = [];
    const progress: { value: number; revision: number }[] = [];
    const stopState = await listenAppUpdateState((value) => states.push(value));
    const stopProgress = await listenAppUpdateProgress((value, revision) => {
      progress.push({ value, revision });
    });
    try {
      const result = await appUpdateDownload();
      expect(states.map((state) => state.state.stage)).toEqual([
        "downloading",
        "readyToRestart",
      ]);
      expect(progress[0]?.revision).toBe(states[0]?.revision);
      expect(result.revision).toBe(states[1]?.revision);
    } finally {
      stopState();
      stopProgress();
    }
  });
});

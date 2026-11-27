// @vitest-environment jsdom
import { mount, tick, unmount } from "svelte";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import GaragePage from "./GaragePage.svelte";
import { motion } from "#lib/shell/motion.svelte.js";
import { toasts } from "#lib/shell/toasts.svelte.js";
import { updatesBadge } from "#lib/shell/updatesBadge.svelte.js";
import type {
  BulkToggleResult,
  LibrarySnapshot,
  ModCardDto,
  ModDetails,
  ToggleResult,
  VersionRow,
} from "#lib/ipc/library.js";

const ipc = vi.hoisted(() => ({
  libraryLoad: vi.fn(),
  listenLibraryTitles: vi.fn(),
  listenLibraryRefresh: vi.fn(),
  modBindPage: vi.fn(),
  modFixPackaging: vi.fn(),
  modDelete: vi.fn(),
  modDetails: vi.fn(),
  modForgetDeleted: vi.fn(),
  modRestoreDeleted: vi.fn(),
  modRestoreVersion: vi.fn(),
  modCommitRestoreUndo: vi.fn(),
  modToggle: vi.fn(),
  modsToggle: vi.fn(),
  modVersions: vi.fn(),
  modUndelete: vi.fn(),
}));

const updates = vi.hoisted(() => ({
  updatesUnfollow: vi.fn(),
  updatesRefollow: vi.fn(),
  updatesDeclineMod: vi.fn(),
  updatesUndoDecline: vi.fn(),
}));

vi.mock("#lib/ipc/library.js", async (importOriginal) => {
  const actual = await importOriginal<typeof import("#lib/ipc/library.js")>();
  return { ...actual, ...ipc };
});

vi.mock("#lib/ipc/updates.js", async (importOriginal) => {
  const actual = await importOriginal<typeof import("#lib/ipc/updates.js")>();
  return { ...actual, ...updates };
});

function card(id: string, enabled = true): ModCardDto {
  return {
    id,
    name: `Тест ${id}`,
    fileName: `${id}.zip`,
    version: "1.0",
    sizeBytes: 1024,
    enabled,
    subFolder: null,
    kind: "vehicle",
    previewUrl: null,
    compat: { modFor: null, game: "0.39", text: null, differs: false },
    update: "none",
    updateVersion: null,
    unchecked: false,
    pending: "none",
    fromGameRepo: false,
    deleted: false,
    copies: 0,
    savedAt: null,
    problem: null,
    duplicateOf: null,
    group: [],
    groupConflict: false,
    hasPage: true,
    role: "vehicle",
  };
}

function details(mod: ModCardDto): ModDetails {
  return {
    id: mod.id,
    name: mod.name,
    fileName: mod.fileName,
    version: mod.version,
    sizeBytes: mod.sizeBytes,
    enabled: mod.enabled,
    installedAt: "2026-10-20T12:00:00Z",
    folder: "C:\\mods",
    kind: mod.kind,
    compat: mod.compat,
    archive: null,
    previewUrl: null,
    binding: { source: "stmods", url: `https://example.com/${mod.id}`, confirmed: true },
    checkingDeclined: false,
  };
}

function snapshot(mods: ModCardDto[], deleted: ModCardDto[] = []): LibrarySnapshot {
  return {
    mods,
    deleted,
    counts: {
      all: mods.length,
      enabled: mods.filter((mod) => mod.enabled).length,
      updates: 0,
      unchecked: 0,
      deleted: deleted.length,
    },
    folder: "C:\\mods",
    folderError: null,
    resolvedUserFolder: "C:\\",
    gameVersion: "0.39",
    gameRunning: false,
    warning: null,
    manyEnabled: null,
  };
}

let cards: ModCardDto[];
let removed: Map<string, { mod: ModCardDto; index: number }>;
let committed: ModCardDto[];
let versions: VersionRow[];
let toggleResults: ToggleResult[];
let bulkResults: BulkToggleResult[];
let pages: HTMLElement[];
let apps: ReturnType<typeof mount>[];

beforeEach(() => {
  vi.useFakeTimers();
  cards = [card("a"), card("b")];
  removed = new Map();
  committed = [];
  versions = [];
  toggleResults = [];
  bulkResults = [];
  pages = [];
  apps = [];
  document.body.replaceChildren();
  document.documentElement.classList.add("no-motion");
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  );
  vi.stubGlobal("CSS", { escape: (value: string) => value });
  const warn = console.warn.bind(console);
  vi.spyOn(console, "warn").mockImplementation((message?: unknown, ...args: unknown[]) => {
    if (String(message).includes("[svelte] state_proxy_equality_mismatch")) return;
    warn(message, ...args);
  });
  toasts.current = null;

  for (const mock of Object.values(ipc)) mock.mockReset();
  for (const mock of Object.values(updates)) mock.mockReset();

  ipc.libraryLoad.mockImplementation(async () =>
    snapshot(
      cards.map((mod) => ({ ...mod, compat: { ...mod.compat }, group: [...mod.group] })),
      committed.map((mod) => ({ ...mod, deleted: true, enabled: false })),
    ),
  );
  ipc.listenLibraryTitles.mockResolvedValue(() => {});
  ipc.listenLibraryRefresh.mockResolvedValue(() => {});
  ipc.modBindPage.mockResolvedValue(undefined);
  ipc.modFixPackaging.mockResolvedValue(undefined);
  ipc.modDetails.mockImplementation(async (id: string) => {
    const mod = cards.find((item) => item.id === id);
    if (!mod) throw new Error("Мод не найден");
    return details(mod);
  });
  ipc.modToggle.mockImplementation(async (id: string, enabled: boolean) => {
    const result = toggleResults.shift() ?? { outcome: "done", enabled, message: null };
    if (result.outcome !== "error" && result.outcome !== "failed") {
      const mod = cards.find((item) => item.id === id);
      if (mod) {
        mod.enabled = result.enabled;
        mod.pending = result.outcome === "queued" ? "queued" : "none";
      }
    }
    return result;
  });
  ipc.modsToggle.mockImplementation(async (ids: string[], enabled: boolean) => {
    const result = bulkResults.shift() ?? { changed: [...ids], queued: [], failed: [] };
    for (const id of result.changed) {
      const mod = cards.find((item) => item.id === id);
      if (mod) mod.enabled = enabled;
    }
    for (const id of result.queued) {
      const mod = cards.find((item) => item.id === id);
      if (mod) {
        mod.enabled = enabled;
        mod.pending = "queued";
      }
    }
    return result;
  });
  ipc.modDelete.mockImplementation(async (id: string) => {
    const index = cards.findIndex((mod) => mod.id === id);
    const [mod] = cards.splice(index, 1);
    removed.set(id, { mod, index });
    return {
      id,
      fileName: mod.fileName,
      backupId: `backup-${id}`,
      wasInDisabled: !mod.enabled,
      name: mod.name,
    };
  });
  ipc.modUndelete.mockImplementation(async (id: string) => {
    const entry = removed.get(id);
    if (!entry) throw new Error("Мод не найден");
    removed.delete(id);
    cards.splice(Math.min(entry.index, cards.length), 0, entry.mod);
  });
  ipc.modForgetDeleted.mockImplementation(async (id: string) => {
    const entry = removed.get(id);
    if (!entry) throw new Error("Мод не найден");
    removed.delete(id);
    committed.push(entry.mod);
  });
  ipc.modRestoreDeleted.mockResolvedValue(undefined);
  ipc.modRestoreVersion.mockResolvedValue({
    backupId: "undo-current",
    installedToken: "token-current",
  });
  ipc.modCommitRestoreUndo.mockResolvedValue(undefined);
  ipc.modVersions.mockImplementation(async () => versions.map((row) => ({ ...row })));
  updates.updatesUnfollow.mockResolvedValue(undefined);
  updates.updatesRefollow.mockResolvedValue(undefined);
  updates.updatesDeclineMod.mockResolvedValue(undefined);
  updates.updatesUndoDecline.mockResolvedValue(undefined);
});

afterEach(async () => {
  for (const app of apps.splice(0)) unmount(app);
  for (const page of pages.splice(0)) page.remove();
  await vi.runAllTimersAsync();
  toasts.current = null;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function renderPage(): HTMLElement {
  const page = document.createElement("div");
  document.body.append(page);
  pages.push(page);
  apps.push(mount(GaragePage, { target: page }));
  return page;
}

async function waitForCard(page: HTMLElement, id: string): Promise<HTMLElement> {
  let found: HTMLElement | null = null;
  await vi.waitFor(() => {
    found = page.querySelector<HTMLElement>(`[data-mod-id="${id}"]`);
    expect(found).not.toBeNull();
  });
  return found!;
}

function toggleSwitch(page: HTMLElement, id: string): HTMLElement {
  return page.querySelector<HTMLElement>(`[data-mod-id="${id}"] [role="switch"]`)!;
}

async function toggleWithUndo(page: HTMLElement, id = "a"): Promise<() => void | Promise<void>> {
  const mod = await waitForCard(page, id);
  toggleSwitch(page, id).click();
  await vi.waitFor(() => expect(toasts.current?.undo).toBeTypeOf("function"));
  return toasts.current!.undo!;
}

async function selectCard(page: HTMLElement, id = "a"): Promise<void> {
  const mod = await waitForCard(page, id);
  mod.querySelector<HTMLButtonElement>(".select")!.click();
  await tick();
  await vi.waitFor(() => expect(page.querySelector("aside.inspector")).not.toBeNull());
}

function button(page: ParentNode, text: string): HTMLButtonElement {
  const found = [...page.querySelectorAll<HTMLButtonElement>("button")].find(
    (item) => item.textContent?.trim() === text,
  );
  expect(found, `Кнопка «${text}»`).toBeDefined();
  return found!;
}

async function selectAll(page: HTMLElement): Promise<void> {
  await waitForCard(page, "a");
  window.dispatchEvent(
    new KeyboardEvent("keydown", { key: "a", code: "KeyA", ctrlKey: true, bubbles: true }),
  );
  await tick();
  await vi.waitFor(() =>
    expect(page.querySelector('aside[aria-label="Выбранные моды"]')).not.toBeNull(),
  );
}

async function bulkDelete(page: HTMLElement): Promise<void> {
  await selectAll(page);
  button(page, "Удалить 2").click();
  await vi.waitFor(() => expect(toasts.current?.message).toBe("Удалено: 2"));
}

describe("GaragePage: Undo после навигации", () => {
  it("тост из прежнего экземпляра обновляет карточку после remount", async () => {
    const first = renderPage();
    const undo = await toggleWithUndo(first);
    const app = apps.pop()!;
    unmount(app);

    const second = renderPage();
    await waitForCard(second, "a");
    expect(toggleSwitch(second, "a").getAttribute("aria-checked")).toBe("false");

    await undo();

    await vi.waitFor(() =>
      expect(toggleSwitch(second, "a").getAttribute("aria-checked")).toBe("true"),
    );
  });

  it("Undo после remount проходит плавным переходом через event.detail", async () => {
    const first = renderPage();
    const undo = await toggleWithUndo(first);
    const app = apps.pop()!;
    unmount(app);

    const second = renderPage();
    await waitForCard(second, "a");
    const run = vi.spyOn(motion, "run");

    await undo();

    await vi.waitFor(() =>
      expect(toggleSwitch(second, "a").getAttribute("aria-checked")).toBe("true"),
    );
    expect(run).toHaveBeenCalledWith(expect.any(Function), "flow-garage");
  });
});

describe("GaragePage: обновление значка", () => {
  it.each(["Это другой мод", "Не проверять обновления"])(
    "Undo «%s» обновляет badge один раз через событие",
    async (action) => {
      const refresh = vi.spyOn(updatesBadge, "refresh").mockImplementation(() => {});
      updatesBadge.start();
      refresh.mockClear();
      const page = renderPage();
      await selectCard(page);

      button(page, action).click();
      await vi.waitFor(() => expect(toasts.current?.undo).toBeTypeOf("function"));
      expect(refresh).toHaveBeenCalledTimes(1);

      await toasts.undo();

      expect(refresh).toHaveBeenCalledTimes(2);
    },
  );
});

describe("GaragePage: ошибки bulk undo/commit", () => {
  it("ошибка bulk undo показывается тостом", async () => {
    const page = renderPage();
    await bulkDelete(page);
    ipc.modUndelete.mockRejectedValueOnce(new Error("Не удалось вернуть мод"));

    await toasts.undo();

    expect(toasts.current?.message).toBe("Не удалось вернуть мод");
  });

  it("ошибка bulk commit не проглатывается автоматическим закреплением", async () => {
    const page = renderPage();
    await bulkDelete(page);
    ipc.modForgetDeleted.mockRejectedValue(new Error("Не удалось закрепить удаление"));

    await expect(toasts.current!.commit!()).rejects.toThrow("Не удалось закрепить удаление");
  });

  it("ошибка отмены bulk toggle показывается тостом", async () => {
    const page = renderPage();
    await selectAll(page);
    button(page, "Выключить 2").click();
    await vi.waitFor(() => expect(toasts.current?.undo).toBeTypeOf("function"));
    ipc.modsToggle.mockRejectedValueOnce(new Error("Не удалось отменить пачку"));

    await toasts.undo();

    expect(toasts.current?.message).toBe("Не удалось отменить пачку");
  });

  it("failed в BulkToggleResult после Undo показывается тостом", async () => {
    const page = renderPage();
    await selectAll(page);
    button(page, "Выключить 2").click();
    await vi.waitFor(() => expect(toasts.current?.undo).toBeTypeOf("function"));
    bulkResults.push({ changed: [], queued: [], failed: ["a", "b"] });

    await toasts.undo();

    expect(toasts.current?.message).toBe("Не удалось отменить: 2");
  });
});

describe("GaragePage: восстановление версии", () => {
  it("commit-тост освобождает Undo-копию", async () => {
    versions = [
      {
        backupId: "older",
        fileName: "old.zip",
        version: "0.5",
        savedAt: "2026-10-20T12:00:00Z",
        sizeBytes: 1024,
      },
    ];
    const page = renderPage();
    await selectCard(page);
    await vi.waitFor(() => expect(page.textContent).toContain("Версия 0.5"));

    button(page, "Вернуть").click();
    await vi.waitFor(() => expect(toasts.current?.commit).toBeTypeOf("function"));
    await toasts.current!.commit!();

    expect(ipc.modCommitRestoreUndo).toHaveBeenCalledWith("undo-current");
  });

  it("ошибка перечитывания после restore не теряет commit-тост", async () => {
    versions = [
      {
        backupId: "older",
        fileName: "old.zip",
        version: "0.5",
        savedAt: "2026-10-20T12:00:00Z",
        sizeBytes: 1024,
      },
    ];
    const page = renderPage();
    await selectCard(page);
    await vi.waitFor(() => expect(page.textContent).toContain("Версия 0.5"));
    ipc.libraryLoad.mockRejectedValueOnce(new Error("Список недоступен"));

    button(page, "Вернуть").click();

    await vi.waitFor(() => expect(toasts.current?.commit).toBeTypeOf("function"));
    await toasts.current!.commit!();
    expect(ipc.modCommitRestoreUndo).toHaveBeenCalledWith("undo-current");
  });

  it("Undo использует точный backup id из тикета, а не одноимённую строку", async () => {
    versions = [
      {
        backupId: "older",
        fileName: "old.zip",
        version: "0.5",
        savedAt: "2026-10-20T12:00:00Z",
        sizeBytes: 1024,
      },
    ];
    ipc.modRestoreVersion
      .mockResolvedValueOnce({
        backupId: "exact-undo",
        installedToken: "exact-token",
      })
      .mockResolvedValueOnce(null);
    ipc.modVersions.mockResolvedValue([
      {
        backupId: "wrong-copy",
        fileName: "a.zip",
        version: "9.9",
        savedAt: "2026-10-22T12:00:00Z",
        sizeBytes: 1024,
      },
      ...versions,
    ]);
    const page = renderPage();
    await selectCard(page);
    await vi.waitFor(() => expect(page.textContent).toContain("Версия 0.5"));

    button(page, "Вернуть").click();
    await vi.waitFor(() => expect(toasts.current?.undo).toBeTypeOf("function"));
    await toasts.undo();

    expect(ipc.modRestoreVersion).toHaveBeenCalledTimes(2);
    expect(ipc.modRestoreVersion).toHaveBeenNthCalledWith(
      2,
      "a",
      "exact-undo",
      "exact-token",
    );
  });

  it("после Undo перечитывает список прошлых версий", async () => {
    const current: VersionRow = {
      backupId: "current",
      fileName: "a.zip",
      version: "1.0",
      savedAt: "2026-10-21T12:00:00Z",
      sizeBytes: 1024,
    };
    const older: VersionRow = {
      backupId: "older",
      fileName: "old.zip",
      version: "0.5",
      savedAt: "2026-10-20T12:00:00Z",
      sizeBytes: 1024,
    };
    const next: VersionRow = {
      backupId: "next",
      fileName: "next.zip",
      version: "2.0",
      savedAt: "2026-10-22T12:00:00Z",
      sizeBytes: 1024,
    };
    let reads = 0;
    ipc.modVersions.mockImplementation(async () => {
      reads += 1;
      return reads < 3 ? [older, current] : [older, current, next];
    });
    const page = renderPage();
    await selectCard(page);
    await vi.waitFor(() => expect(page.textContent).toContain("Версия 0.5"));

    button(page, "Вернуть").click();
    await vi.waitFor(() => expect(toasts.current?.undo).toBeTypeOf("function"));
    const readsBeforeUndo = reads;
    let releaseLoad!: (value: LibrarySnapshot) => void;
    ipc.libraryLoad.mockImplementationOnce(
      () => new Promise<LibrarySnapshot>((resolve) => (releaseLoad = resolve)),
    );

    try {
      await toasts.undo();
      // versionsKey перечитывает список после восстановления точной копии из тикета.
      await vi.waitFor(() => expect(reads).toBe(readsBeforeUndo + 1));
    } finally {
      releaseLoad(snapshot(cards.map((mod) => ({ ...mod }))));
    }
    await vi.waitFor(() => expect(page.textContent).toContain("Версия 2.0"));
  });
});

describe("GaragePage: результат Undo toggle", () => {
  it("queued показывается как отложенная отмена, а не ошибка", async () => {
    toggleResults.push(
      { outcome: "done", enabled: false, message: null },
      { outcome: "queued", enabled: true, message: "Игра применит отмену после закрытия" },
    );
    const page = renderPage();
    await toggleWithUndo(page);

    await toasts.undo();

    expect(toasts.current?.message).toBe("Игра применит отмену после закрытия");
    await vi.waitFor(() =>
      expect(toggleSwitch(page, "a").getAttribute("aria-checked")).toBe("true"),
    );
  });

  it.each(["error", "failed"])("outcome %s показывает ошибку и не сообщает об успехе", async (outcome) => {
    toggleResults.push(
      { outcome: "done", enabled: false, message: null },
      { outcome, enabled: true, message: "Файл занят игрой" },
    );
    const page = renderPage();
    await toggleWithUndo(page);

    await toasts.undo();

    expect(toasts.current?.message).toBe("Файл занят игрой");
  });
});

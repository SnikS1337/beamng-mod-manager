import { invoke, isTauri } from "@tauri-apps/api/core";
import { themeWave } from "./themeWave.js";
import { settingsLoad, settingsSave, type SettingsDto } from "#lib/ipc/settings.js";

const THEMES = ["night", "deep", "dusk", "forest", "graphite"] as const;
const BACKDROPS = ["accent", "aurora", "sunset", "ocean", "nebula", "mono"] as const;

/** Цвет фона темы для окна: только `#rrggbb`, иначе не трогаем окно. */
export function windowColor(value: string): string | null {
  const color = value.trim().toLowerCase();
  return /^#[0-9a-f]{6}$/.test(color) ? color : null;
}

class Appearance {
  settings = $state<SettingsDto | null>(null);

  async load(): Promise<SettingsDto | null> {
    try {
      this.settings = await settingsLoad();
      this.apply();
    } catch {
      // Настройки не прочитались — оставляем оформление по умолчанию.
    }
    return this.settings;
  }

  /// Показать настройки сразу, не дожидаясь записи на диск.
  preview(next: SettingsDto): void {
    this.settings = next;
    this.apply();
  }

  async save(next: SettingsDto): Promise<SettingsDto> {
    this.settings = await settingsSave(next);
    this.apply();
    return this.settings;
  }

  apply(): void {
    if (typeof document === "undefined" || !this.settings) return;
    const root = document.documentElement;
    const dark =
      this.settings.mode === "dark" ||
      (this.settings.mode === "system" &&
        window.matchMedia("(prefers-color-scheme: dark)").matches);
    root.classList.toggle("dark", dark);
    root.classList.toggle("no-motion", !this.settings.uiAnimations);
    for (const theme of THEMES) {
      root.classList.toggle(`theme-${theme}`, this.settings.theme === theme);
    }
    const background = this.settings.background ?? "accent";
    for (const backdrop of BACKDROPS) {
      root.classList.toggle(`backdrop-${backdrop}`, background === backdrop);
    }
    this.paintWindow(root);
  }

  #painted: string | null = null;

  /**
   * Окно под страницей — цвета фона темы: пока WebView перерисовывает страницу
   * (развернули окно), видно его, а не чёрный прямоугольник.
   */
  private paintWindow(root: HTMLElement): void {
    if (!isTauri()) return;
    const color = windowColor(getComputedStyle(root).getPropertyValue("--background"));
    if (!color || color === this.#painted) return;
    this.#painted = color;
    void invoke("window_background", { color }).catch(() => {
      this.#painted = null;
    });
  }
}

export const appearance = new Appearance();

if (typeof window !== "undefined") {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    // Windows сменил тему сам — та же волна, из центра окна.
    if (appearance.settings?.mode === "system") void themeWave(null, () => appearance.apply());
  });
}

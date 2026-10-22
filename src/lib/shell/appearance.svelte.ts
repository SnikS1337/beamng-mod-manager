import { settingsLoad, settingsSave, type SettingsDto } from "$lib/ipc/settings";

const THEMES = ["night", "deep", "dusk", "forest", "graphite"] as const;

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
    root.classList.toggle("no-glass", !this.settings.liquidGlass);
    root.classList.toggle("no-motion", !this.settings.uiAnimations);
    root.classList.toggle("no-background", !this.settings.animatedBackground);
    for (const theme of THEMES) {
      root.classList.toggle(`theme-${theme}`, this.settings.theme === theme);
    }
  }
}

export const appearance = new Appearance();

if (typeof window !== "undefined") {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (appearance.settings?.mode === "system") appearance.apply();
  });
}

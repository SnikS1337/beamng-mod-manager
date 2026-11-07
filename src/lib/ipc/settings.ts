import { invoke, isTauri } from "@tauri-apps/api/core";

import type { SettingsDto } from "./generated.js";

export type { SettingsDto } from "./generated.js";
export type { BackgroundStyle, BackgroundTheme, UiMode } from "./unions.js";

let mock: SettingsDto | null = null;

export async function appDataFolder(): Promise<string> {
  if (isTauri()) return invoke<string>("app_data_folder");
  return "C:\\Users\\Player\\AppData\\Local\\BeamModManager";
}

export async function settingsLoad(): Promise<SettingsDto> {
  if (isTauri()) return invoke<SettingsDto>("settings_load");
  mock ??= {
    allowDownloads: false,
    modsFolder: "C:\\Users\\Player\\AppData\\Local\\BeamNG\\BeamNG.drive\\current\\mods",
    userFolder: null,
    downloadsFolder: null,
    deleteDownloadsAfterInstall: false,
    checkTtlHours: 6,
    autoCheckOnStart: true,
    checkAppUpdates: true,
    autoCheckEveryHours: 0,
    checkDisabledMods: true,
    disabledSources: [],
    keepBackups: 3,
    maxCardChecksPerSource: 150,
    liquidGlass: true,
    mode: "dark",
    theme: "night",
    background: "accent",
    uiAnimations: true,
    animatedBackground: true,
    fireflies: true,
    backgroundTilt: false,
    activeModsWarnAbove: 30,
    firstRunDone: false,
    watchFolders: [],
  };
  return { ...mock, disabledSources: [...mock.disabledSources], watchFolders: [...mock.watchFolders] };
}

export async function settingsSave(settings: SettingsDto): Promise<SettingsDto> {
  if (isTauri()) return invoke<SettingsDto>("settings_save", { settings });
  mock = {
    ...settings,
    disabledSources: [...settings.disabledSources],
    watchFolders: [...settings.watchFolders],
  };
  return settingsLoad();
}

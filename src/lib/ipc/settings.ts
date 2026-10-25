import { invoke, isTauri } from "@tauri-apps/api/core";

export interface SettingsDto {
  allowDownloads: boolean;
  modsFolder: string | null;
  userFolder: string | null;
  checkTtlHours: number;
  autoCheckOnStart: boolean;
  checkAppUpdates: boolean;
  autoCheckEveryHours: number;
  checkDisabledMods: boolean;
  disabledSources: string[];
  keepBackups: number;
  maxCardChecksPerSource: number;
  liquidGlass: boolean;
  mode: "dark" | "light" | "system";
  theme: "night" | "deep" | "dusk" | "forest" | "graphite";
  uiAnimations: boolean;
  animatedBackground: boolean;
  fireflies: boolean;
  backgroundTilt: boolean;
  activeModsWarnAbove: number;
  firstRunDone: boolean;
  watchFolders: string[];
}

let mock: SettingsDto | null = null;

export async function settingsLoad(): Promise<SettingsDto> {
  if (isTauri()) return invoke<SettingsDto>("settings_load");
  mock ??= {
    allowDownloads: false,
    modsFolder: "C:\\Users\\Player\\AppData\\Local\\BeamNG\\BeamNG.drive\\current\\mods",
    userFolder: null,
    checkTtlHours: 6,
    autoCheckOnStart: true,
    checkAppUpdates: true,
    autoCheckEveryHours: 0,
    checkDisabledMods: true,
    disabledSources: ["modland"],
    keepBackups: 3,
    maxCardChecksPerSource: 150,
    liquidGlass: true,
    mode: "dark",
    theme: "night",
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

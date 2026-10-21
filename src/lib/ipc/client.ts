import { invoke, isTauri } from "@tauri-apps/api/core";
import type { BootstrapDto } from "./types";

export async function bootstrap(): Promise<BootstrapDto> {
  if (isTauri()) {
    return invoke<BootstrapDto>("app_bootstrap");
  }

  return {
    appVersion: "web-preview",
    os: navigator.platform || "web",
    arch: "-",
    dataDir: "%LOCALAPPDATA%\\BeamModManager",
    settingsExists: false,
    libraryExists: false,
  };
}

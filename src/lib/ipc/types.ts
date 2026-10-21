export interface AppError {
  code: string;
  message: string;
}

export interface BootstrapDto {
  appVersion: string;
  os: string;
  arch: string;
  dataDir: string;
  settingsExists: boolean;
  libraryExists: boolean;
}

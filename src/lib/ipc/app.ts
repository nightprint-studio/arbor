import { invoke } from '@tauri-apps/api/core';

export interface AppInfo {
  /** Semantic version, single source of truth from tauri.conf.json. */
  version: string;
  /** Friendly OS family: "Windows", "macOS", "Linux" (or raw OS const fallback). */
  os: string;
  /** CPU architecture, e.g. "x86_64", "aarch64". */
  arch: string;
}

/** Read app metadata from the backend. Used by the About modal. */
export function getAppInfo(): Promise<AppInfo> {
  return invoke('get_app_info');
}

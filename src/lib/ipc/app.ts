import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { platform } from './rpc';

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
  return platform<AppInfo>('get_app_info');
}

/** Open (or focus, if already open) the dedicated File Explorer window — the
 *  same standalone window the global Ctrl+Shift+E shortcut summons. */
export function openExplorerWindow(): Promise<void> {
  return invoke('open_explorer_window');
}

/** Open (or focus, if already open) the dedicated merula window — the standalone
 *  music live-coding DAW shell. */
export function openMerulaWindow(): Promise<void> {
  return invoke('open_merula_window');
}

/** Open (or focus, if already open) the dedicated Tyto window — the standalone
 *  screen-recorder control panel (also summoned by the Ctrl+Shift+R shortcut). */
export function openTytoWindow(): Promise<void> {
  return invoke('open_tyto_window');
}

/** Open (or focus, if already open) the dedicated Corvus window — the Git
 *  product shell (today the Git UI also loads in the main/launcher window). */
export function openCorvusWindow(): Promise<void> {
  return invoke('open_corvus_window');
}

/** Open (or focus, if already open) the dedicated Bennu window — the standalone
 *  Java editor. */
export function openBennuWindow(): Promise<void> {
  return invoke('open_bennu_window');
}

/** Open (or focus, if already open) the dedicated Picus window — the SQL studio
 *  (database client + maintainer of the per-dialect SQL script repository). */
export function openPicusWindow(): Promise<void> {
  return invoke('open_picus_window');
}

/** Open (or focus, if already open) the launcher (Canopy) window. */
export function openLauncherWindow(): Promise<void> {
  return invoke('open_launcher_window');
}

// ── Corvus git backend (corvus-be) liveness ─────────────────────────────────
/** Fired once when the corvus-be git backend process dies (crash / kill). The
 *  Corvus window shows a blocking fatal overlay asking the user to restart —
 *  there is no live respawn yet, so a full app restart is the only recovery. */
export const onCorvusBeDown = (cb: () => void): Promise<UnlistenFn> =>
  listen('arbor://corvus-be-down', () => cb());

// ── Merula audio backend (merula-be) liveness ───────────────────────────────
/** Fired once when the `merula-be` audio backend process dies (crash / kill),
 *  scoped to the Merula window. The Merula window shows a blocking fatal overlay
 *  asking the user to restart — there is no live respawn yet, so a full app
 *  restart is the only recovery (mirrors {@link onCorvusBeDown}). */
export const onMerulaBeDown = (cb: () => void): Promise<UnlistenFn> =>
  listen('arbor://merula-be-down', () => cb());

/** Relaunch the whole app (replaces the running process). Never resolves on the
 *  happy path — the webview is torn down with the process. */
export const restartApp = (): Promise<void> => invoke('restart_app');

/**
 * Map of Canopy product id → the command that opens its real Arbor window.
 * The launcher's primary action delegates here for products that have a real
 * window; everything else runs the prototype's mock state machine.
 */
export const PRODUCT_WINDOW_OPENERS: Record<string, () => Promise<void>> = {
  corvus: openCorvusWindow,   // Git client
  sitta: openExplorerWindow,  // File explorer
  merula: openMerulaWindow,    // Music (merula / Merula)
  tyto: openTytoWindow,        // Screen recorder
  bennu: openBennuWindow,      // Java editor
  picus: openPicusWindow,      // SQL studio
};

// ── Product window running-state (launcher ↔ product windows) ────────────────

/** Product ids that currently have at least one window open. The launcher seeds
 *  its running state with this on mount. */
export function listRunningProducts(): Promise<string[]> {
  return invoke('list_running_products');
}

/** Close every window of a product — the launcher's "Stop" action. */
export function closeProductWindow(id: string): Promise<void> {
  return invoke('close_product_window', { id });
}

export interface ProductState {
  id: string;
  running: boolean;
}

/** Subscribe to product running-state changes (a window opened/closed). Returns
 *  the Tauri unlisten handle. */
export function onProductState(cb: (s: ProductState) => void): Promise<UnlistenFn> {
  return listen<ProductState>('arbor://product-state', (e) => cb(e.payload));
}

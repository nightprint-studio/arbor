/**
 * Shell commands for the frozen-frame region-selection window.
 *
 * These are DIRECT Tauri commands (in `src-tauri`, not the tyto-be rpc bridge),
 * so args are camelCase (Tauri converts a command's direct args) — see
 * feedback_tauri_invoke_camelcase_args.
 */
import { invoke } from '@tauri-apps/api/core';

/** A UI-element rect (monitor-local CSS px) for the overlay's smart pick. */
export interface ElemRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** A whole-window hover target (virtual-desktop CSS px) for the picker's window mode.
 *  `id` = `win-<hwnd>` (matches the capture-source picker). */
export interface WinRect {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** A whole-monitor hover target (virtual-desktop CSS px) for the picker's display mode.
 *  `id` = `mon-<hmonitor>` (matches the capture-source picker). */
export interface MonRect {
  id: string;
  name: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Init payload the region window pulls on mount. */
export interface RegionInit {
  path: string;
  x: number;
  y: number;
  width: number;
  height: number;
  elements: ElemRect[];
  /** Whole-window hover targets (virtual-desktop CSS px), for the `window` picker mode. */
  windows: WinRect[];
  /** Whole-monitor hover targets (virtual-desktop CSS px), for the `display` picker mode. */
  monitors: MonRect[];
  /** Mode the overlay opens in ('rect' | 'free' | 'smart' | 'window' | 'display'). */
  initial_mode: string;
}

/** Open the opaque overlay over `screenshotPath`, covering the given logical bounds.
 *  Top-level invoke args are camelCase; the array element fields stay snake_case
 *  (Tauri doesn't camel-convert nested struct fields). */
export function openRegionSelectorWindow(args: {
  screenshotPath: string;
  x: number;
  y: number;
  width: number;
  height: number;
  elements: ElemRect[];
  windows: WinRect[];
  monitors: MonRect[];
  initialMode: string;
}): Promise<void> {
  return invoke('open_region_selector_window', args);
}

/** The region window pulls its init (screenshot + bounds) on mount. */
export function getRegionInit(): Promise<RegionInit | null> {
  return invoke('get_region_init');
}

/** The outcome of a region selection, polled by the Tyto window. */
export interface RegionResult {
  /** true = confirmed with a rectangle; false = cancelled. */
  confirmed: boolean;
  /** CSS-pixel rectangle (window-local); only meaningful when confirmed. */
  x: number;
  y: number;
  width: number;
  height: number;
  /** Freehand polygon (window-local CSS px) — present only on a freehand confirm,
   *  null/absent for a plain rectangle. Resolved to a physical screenshot mask. */
  points?: number[][] | null;
  /** Picked whole-window id (`win-<hwnd>`) — set only in the `window` picker mode. */
  window_id?: string | null;
  /** Picked whole-monitor id (`mon-<hmonitor>`) — set only in the `display` picker mode. */
  monitor_id?: string | null;
}

/**
 * Pull the region outcome. Resolves `null` while the user is still selecting, then
 * the outcome exactly once (the backend clears it on read). Robust where a pushed
 * event isn't: an outgoing invoke works even while the Tyto window is hidden.
 */
export function takeRegionResult(): Promise<RegionResult | null> {
  return invoke('take_region_result');
}

/** Confirm the CSS-pixel rectangle (window-local) — routes it back to Tyto. In
 *  freehand mode `points` (window-local CSS px) carries the traced polygon; omit it
 *  for a plain rectangle. */
export function regionSelectorConfirm(rect: {
  x: number;
  y: number;
  width: number;
  height: number;
  points?: number[][] | null;
}): Promise<void> {
  return invoke('region_selector_confirm', rect);
}

/** Pick a whole window or monitor from the on-screen picker (no rectangle). `kind`
 *  is `'window'` (→ window_id) or `'display'` (→ monitor_id); routes it back to Tyto. */
export function regionSelectorPick(kind: 'window' | 'display', id: string): Promise<void> {
  return invoke('region_selector_pick', { kind, id });
}

/** Cancel — close the overlay, restore Tyto. */
export function regionSelectorCancel(): Promise<void> {
  return invoke('region_selector_cancel');
}

/**
 * Shell commands for the recording HUD window (`tyto-hud`).
 *
 * DIRECT Tauri commands (in `src-tauri`, not the tyto-be rpc bridge). `open` hides
 * Tyto and shows the always-on-top HUD on the display being recorded; `resize`
 * toggles its compact/expanded
 * layout (the shell owns the size + placement); `close` tears the HUD down, restores
 * Tyto, and posts `tyto://recording-stopped` back so the store can finalize its UI.
 */
import { invoke } from '@tauri-apps/api/core';

/** Fired by the shell after the HUD closes and Tyto is restored. */
export const TYTO_RECORDING_STOPPED = 'tyto://recording-stopped';

/** What the HUD window pulls on mount. */
export interface HudInit {
  target_label: string;
}

/** The display to put the HUD on, in PHYSICAL desktop pixels (straight from
 *  `list_capture_sources`). Null when the target names no display. */
export interface HudDisplay {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Open the HUD; `targetLabel` is what the expanded layout shows as the subject, and
 *  `display` is the screen being recorded — the shell puts the HUD there instead of on
 *  the primary monitor. */
export function openRecordingHud(targetLabel: string, display: HudDisplay | null): Promise<void> {
  return invoke('open_recording_hud', { targetLabel, display });
}

/** Toggle the HUD between compact (pill) and expanded (card) layouts. */
export function resizeRecordingHud(expanded: boolean): Promise<void> {
  return invoke('resize_recording_hud', { expanded });
}

/** The HUD window reads its init (target label) once on mount. */
export function getHudInit(): Promise<HudInit> {
  return invoke('get_hud_init');
}

export function closeRecordingHud(): Promise<void> {
  return invoke('close_recording_hud');
}

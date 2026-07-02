/**
 * Shell commands for the recording HUD window (`tyto-hud`).
 *
 * DIRECT Tauri commands (in `src-tauri`, not the tyto-be rpc bridge). `open` hides
 * Tyto and shows the always-on-top HUD; `resize` toggles its compact/expanded
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

/** Open the HUD; `targetLabel` is what the expanded layout shows as the subject. */
export function openRecordingHud(targetLabel: string): Promise<void> {
  return invoke('open_recording_hud', { targetLabel });
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

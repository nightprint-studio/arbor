/**
 * Shell commands for the main Tyto window (`tyto`) presentation.
 *
 * DIRECT Tauri commands (in `src-tauri`). Tyto's "compact" presentation is the in-window
 * fullscreen Snip selector: the shell grows the window to cover a monitor (or restores
 * the full control panel), and the FE paints the matching surface.
 */
import { invoke } from '@tauri-apps/api/core';

/** Drive the Tyto window in/out of its in-window fullscreen selector (the Snip-style
 *  capture picker). `active` true grows the window to cover the given monitor bounds
 *  (logical px, always-on-top, non-resizable) so it paints the frozen backdrop edge-to-
 *  edge; false restores the full control panel. `x`/`y`/`width`/`height` are the target
 *  monitor's logical bounds (from `freezeScreen`) and are ignored when `active` is false. */
export function setTytoSelection(
  active: boolean,
  x: number,
  y: number,
  width: number,
  height: number,
): Promise<void> {
  return invoke('set_tyto_selection', { active, x, y, width, height });
}

/** Reset the Tyto window to its full control-panel geometry WITHOUT showing/focusing it.
 *  Used on the recording-start error path: the window may still be at its monitor-covering
 *  countdown bounds, so this restores the panel size before the FE re-shows it. */
export function resetTytoBounds(): Promise<void> {
  return invoke('reset_tyto_bounds');
}

/** Take + clear the "opened via global shortcut" intent: true means Tyto should drop
 *  straight into the Snip selector (quick capture) rather than the full panel. Pulled by
 *  the FE on mount (fresh window) and on the `tyto://enter-snip` event (already-open). */
export function takeTytoSnipIntent(): Promise<boolean> {
  return invoke('take_tyto_snip_intent');
}

/** Open the OS privacy settings for screen recording. Resolves `false` when the
 *  platform has no such screen to send the user to — macOS has one, Windows and
 *  Linux don't, and the caller says something useful instead of leaving a button
 *  that quietly does nothing. */
export function openScreenRecordingSettings(): Promise<boolean> {
  return invoke('open_screen_recording_settings');
}

/** The shell's view of the screen-recording permission, plus how Arbor was
 *  launched. Read it when capture is refused: "granted but still refused",
 *  "granted to a different binary" and "granted to the terminal that started
 *  Arbor" are three different problems with three different fixes, and the
 *  recorder's own refusal can't tell them apart. */
export interface ScreenRecordingStatus {
  /** Whether the shell process — the app itself — has the permission. */
  granted: boolean;
  /** Whether the running executable lives inside a `.app` bundle. */
  bundled: boolean;
  /** The running executable's path. */
  executable: string;
  /** The likeliest cause when capture is refused anyway, or null. */
  hint: string | null;
}

export function screenRecordingStatus(): Promise<ScreenRecordingStatus> {
  return invoke('screen_recording_status');
}

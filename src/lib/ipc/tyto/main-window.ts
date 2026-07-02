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

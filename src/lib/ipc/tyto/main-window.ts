/**
 * Shell command for the main Tyto window (`tyto`) presentation.
 *
 * DIRECT Tauri command (in `src-tauri`). Switches the window between its compact
 * "mini" toolbar (Snip-like, small + always-on-top, top-center) and the full control
 * panel — the shell owns the size/placement so it lives in one spot.
 */
import { invoke } from '@tauri-apps/api/core';

/** Resize/place the Tyto window for compact (mini toolbar) or full mode. */
export function setTytoCompact(compact: boolean): Promise<void> {
  return invoke('set_tyto_compact', { compact });
}

/** Grow the compact mini toolbar to host the inline method menu (its panel can't paint
 *  outside the 56px-tall window), then shrink it back when the menu closes. `height` is
 *  the exact target height in logical px (bar + measured menu) so the grown window hugs
 *  the menu with no empty strip; omitted/`null` on close. No-op unless in mini mode. */
export function setTytoMiniMenu(open: boolean, height?: number): Promise<void> {
  return invoke('set_tyto_mini_menu', { open, height: height ?? null });
}

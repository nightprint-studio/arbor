/**
 * Resizable-panel size persistence — shared by the main (Arbor) window and the
 * nemus window. Panel widths/heights are *ephemeral UI chrome* (per the Arbor
 * working agreement, the one localStorage exception alongside the active
 * sidebar section), so they live in localStorage rather than the typed config.
 *
 * Sizes are stored as a *ratio* of the viewport (width for side panels, height
 * for bottom panels), not absolute pixels — so a panel keeps its proportional
 * footprint when the window is resized between sessions. On load the ratio is
 * re-projected onto the current viewport and clamped to the panel's [min, max].
 */

/** Restore a persisted panel size (px), or `defaultPx` when absent/invalid. */
export function loadPixels(
  key: string,
  defaultPx: number,
  min: number,
  max: number,
  useHeight = false,
): number {
  try {
    const ratio = parseFloat(localStorage.getItem(key) ?? '');
    if (!isNaN(ratio) && ratio > 0) {
      const ref = useHeight ? window.innerHeight : window.innerWidth;
      return Math.max(min, Math.min(max, Math.round(ratio * ref)));
    }
  } catch { /* ignore */ }
  return defaultPx;
}

/** Persist a panel size as a viewport ratio (survives window resizes). */
export function saveRatio(key: string, px: number, useHeight = false) {
  try {
    const ref = useHeight ? window.innerHeight : window.innerWidth;
    localStorage.setItem(key, String(px / ref));
  } catch { /* ignore */ }
}

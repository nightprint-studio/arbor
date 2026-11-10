/**
 * The workspace colour palette — the global `--ws-color-0 … --ws-color-11` theme vars, one
 * source for every product that tints workspaces (Bennu, Merula, …) so they stay visually
 * consistent.
 */

/** Palette size. */
export const WS_COLOR_COUNT = 12;

/** CSS var reference for a palette index (clamped) — feeds `<Monogram color=…>`. */
export function wsColorVar(idx: number): string {
  const i = Number.isFinite(idx) ? Math.max(0, Math.min(WS_COLOR_COUNT - 1, Math.floor(idx))) : 0;
  return `var(--ws-color-${i})`;
}

/**
 * Per-lane heights for {@link ArrangementView} — drag the bottom edge of a track
 * header to make that lane taller (the piano-roll spreads vertically, so high/low
 * notes separate). Pure session UI state (like {@link arrViewOptions}); keyed by
 * the BE-stable strip index, so it survives a re-eval. No persistence — resets to
 * the default on reload.
 */

export const DEFAULT_LANE_H = 72;
export const MIN_LANE_H = 40;
export const MAX_LANE_H = 320;

const clampH = (h: number) => Math.max(MIN_LANE_H, Math.min(MAX_LANE_H, Math.round(h)));

function createLaneSizes() {
  // track index → height in px. Absent = the default.
  let heights = $state<Record<number, number>>({});

  return {
    /** This lane's height in px (the override, else the default). */
    height(track: number): number {
      return heights[track] ?? DEFAULT_LANE_H;
    },
    /** Whether this lane has been resized away from the default. */
    isCustom(track: number): boolean {
      return heights[track] != null;
    },
    setHeight(track: number, px: number) {
      heights = { ...heights, [track]: clampH(px) };
    },
    /** Reset one lane to the default height. */
    reset(track: number) {
      if (heights[track] == null) return;
      const next = { ...heights };
      delete next[track];
      heights = next;
    },
    /** Reset every lane to the default height. */
    resetAll() {
      heights = {};
    },
    /** Whether any lane has been resized (for an enabled/disabled reset action). */
    get anyCustom() {
      return Object.keys(heights).length > 0;
    },
  };
}

export const laneSizes = createLaneSizes();

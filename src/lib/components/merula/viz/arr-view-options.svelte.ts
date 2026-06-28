/**
 * Arrangement view options — ephemeral display toggles for {@link ArrangementView}
 * (the toolbar above the ruler). Pure session UI state: how the timeline is
 * *drawn*, not what it contains (that's {@link arrangementStore}). No persistence
 * — these reset to their defaults on reload, like a DAW's view chrome.
 *
 *  - `waveform`  — render audio regions (sample / drum / `audio()` lanes) as a
 *                  synthesized waveform instead of clean blocks. Default OFF, so
 *                  the arrangement reads as a clean block grid out of the box.
 *  - `follow`    — auto-scroll to keep the playhead in view while playing.
 *  - `grid`      — draw the bar grid lines / ruler ticks.
 *  - `labels`    — print note / sound names on blocks when they're wide enough.
 *  - `minimap`   — show the overview strip + viewport box below the timeline.
 *  - `zoom`      — horizontal scale multiplier on the base pixels-per-cycle.
 *  - `velocity`  — colour each event by its gain (a velocity / dynamics heatmap):
 *                  attenuated events dim, full-gain events stay vivid.
 */

/** Horizontal-zoom bounds + step (multiplicative, so each press scales evenly). */
export const MIN_ZOOM = 0.25;
export const MAX_ZOOM = 6;
export const ZOOM_STEP = 1.25;
const clampZoom = (z: number) => Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, z));

function createArrViewOptions() {
  let waveform = $state(false);
  let follow   = $state(true);
  let grid     = $state(true);
  let labels   = $state(true);
  let minimap  = $state(true);
  let velocity = $state(false);
  let zoom     = $state(1);

  return {
    get waveform() { return waveform; },
    get follow()   { return follow; },
    get grid()     { return grid; },
    get labels()   { return labels; },
    get minimap()  { return minimap; },
    get velocity() { return velocity; },
    get zoom()     { return zoom; },
    toggleWaveform() { waveform = !waveform; },
    toggleFollow()   { follow   = !follow; },
    toggleGrid()     { grid     = !grid; },
    toggleLabels()   { labels   = !labels; },
    toggleMinimap()  { minimap  = !minimap; },
    toggleVelocity() { velocity = !velocity; },
    setZoom(z: number)   { zoom = clampZoom(z); },
    zoomBy(factor: number) { zoom = clampZoom(zoom * factor); },
    zoomIn()    { zoom = clampZoom(zoom * ZOOM_STEP); },
    zoomOut()   { zoom = clampZoom(zoom / ZOOM_STEP); },
    zoomReset() { zoom = 1; },
  };
}

export const arrViewOptions = createArrViewOptions();

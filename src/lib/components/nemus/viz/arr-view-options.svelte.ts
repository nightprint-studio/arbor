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
 */

function createArrViewOptions() {
  let waveform = $state(false);
  let follow   = $state(true);
  let grid     = $state(true);
  let labels   = $state(true);

  return {
    get waveform() { return waveform; },
    get follow()   { return follow; },
    get grid()     { return grid; },
    get labels()   { return labels; },
    toggleWaveform() { waveform = !waveform; },
    toggleFollow()   { follow   = !follow; },
    toggleGrid()     { grid     = !grid; },
    toggleLabels()   { labels   = !labels; },
  };
}

export const arrViewOptions = createArrViewOptions();

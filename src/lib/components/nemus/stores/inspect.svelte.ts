/**
 * Selected-event store — the single hap (note / sample hit / signal) the user
 * clicked in the arrangement, surfaced in the Inspector's "Selected event"
 * section. Track-level selection still lives in the mixer store (index-keyed);
 * this is the finer-grained "which event inside that track" pointer.
 *
 * Purely ephemeral UI state (a selection highlight) — no persistence. The
 * arrangement re-queries on every eval, so the picked hap can go stale; we keep
 * only a plain snapshot of the fields the Inspector renders (plus the owning
 * track) rather than a live reference into `arrangementStore`, and clear it when
 * the track selection moves elsewhere.
 */

import type { NemusQueryHap } from '$lib/ipc/nemus';

/** The fields the Inspector needs to describe a picked event. A snapshot, not a
 *  live ref — the underlying query array is replaced wholesale on each eval. */
export interface SelectedHap {
  /** Owning arrangement-strip / lane index. */
  track: number;
  start: number;
  end: number;
  has_onset: boolean;
  sound: string | null;
  note: number | null;
  gain: number | null;
}

function snapshot(track: number, h: NemusQueryHap): SelectedHap {
  return {
    track,
    start: h.start,
    end: h.end,
    has_onset: h.has_onset,
    sound: h.sound,
    note: h.note,
    gain: h.gain,
  };
}

function createInspectStore() {
  let selected = $state<SelectedHap | null>(null);

  return {
    get selected() { return selected; },
    /** Pick an event (from an arrangement click). */
    select(track: number, h: NemusQueryHap) { selected = snapshot(track, h); },
    /** Drop the event selection (e.g. the owning track changed). */
    clear() { selected = null; },
    /** Clear the pick if it no longer belongs to `track` — called when the
     *  track selection moves, so the Inspector never shows a stale cross-track
     *  event. */
    clearIfNotTrack(track: number | null) {
      if (selected && selected.track !== track) selected = null;
    },
  };
}

export const inspectStore = createInspectStore();

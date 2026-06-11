/**
 * Arrangement viz data — the real timeline behind {@link ArrangementView}.
 *
 * The live event streams (transport / meters / active-haps) are owned by the
 * engine store; this one owns the *one* pull the arrangement needs: `grove_query`
 * over `[0, VIEW_CYCLES)` returns every hap of the last-evaluated `Tracks` off
 * the audio thread (see the Step-1 gate — viz data is a query, not the mock).
 *
 * It is a viz-local store so the view consumes it instead of invoking IPC inline:
 * `schedule()` is fired from the eval/diagnostics trigger (debounced so the
 * inline `grove_eval` result + the `grove:diagnostics` echo coalesce into one
 * query); the arrangement is static between evals, so the playhead never re-queries.
 */

import { groveQuery, type GroveQueryHap } from '$lib/ipc/grove';

/** Cycle window queried + drawn (matches the arrangement grid width). */
export const VIEW_CYCLES = 96;

/** One lane = one mixer strip = one `track` index in the query. */
export interface VizLane {
  /** Mixer-strip / arrangement index (0-based) — the stable key into the
   *  shared mute/solo store and the mixer (Step 3b). */
  track: number;
  /** This lane's haps, sorted by onset. */
  haps: GroveQueryHap[];
  /** Distinct sound names, most-frequent first (sample/drum character). */
  sounds: string[];
  /** Lowest / highest MIDI note across the lane, or null when unpitched. */
  noteLo: number | null;
  noteHi: number | null;
  /** A continuous signal (no onset) is present — drawn as a band, not blocks. */
  hasContinuous: boolean;
  /** Count of pitched haps (for the lane subtitle). */
  noteCount: number;
}

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

/** MIDI → scientific pitch name (C4 = 60), for lane subtitles / tooltips. */
export function noteName(midi: number): string {
  const n = Math.round(midi);
  return `${NOTE_NAMES[((n % 12) + 12) % 12]}${Math.floor(n / 12) - 1}`;
}

/** Group the flat hap list into per-track lanes, deriving each lane's character
 *  (sounds / pitch range) so the header + roll can render without re-scanning. */
function buildLanes(haps: GroveQueryHap[]): VizLane[] {
  const byTrack = new Map<number, GroveQueryHap[]>();
  for (const h of haps) {
    const arr = byTrack.get(h.track);
    if (arr) arr.push(h);
    else byTrack.set(h.track, [h]);
  }

  const lanes: VizLane[] = [];
  for (const track of [...byTrack.keys()].sort((a, b) => a - b)) {
    const hs = byTrack.get(track)!.slice().sort((a, b) => a.start - b.start);
    const counts = new Map<string, number>();
    let noteLo: number | null = null;
    let noteHi: number | null = null;
    let hasContinuous = false;
    let noteCount = 0;
    for (const h of hs) {
      if (h.sound) counts.set(h.sound, (counts.get(h.sound) ?? 0) + 1);
      if (h.note != null) {
        noteCount++;
        noteLo = noteLo == null ? h.note : Math.min(noteLo, h.note);
        noteHi = noteHi == null ? h.note : Math.max(noteHi, h.note);
      }
      if (!h.has_onset) hasContinuous = true;
    }
    const sounds = [...counts.entries()].sort((a, b) => b[1] - a[1]).map((e) => e[0]);
    lanes.push({ track, haps: hs, sounds, noteLo, noteHi, hasContinuous, noteCount });
  }
  return lanes;
}

function createArrangementStore() {
  let haps    = $state<GroveQueryHap[]>([]);
  let loading = $state(false);
  let loaded  = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  const lanes      = $derived(buildLanes(haps));
  const contentEnd = $derived(haps.reduce((m, h) => Math.max(m, h.end), 0));

  async function run(cycles: number) {
    loading = true;
    try {
      haps = (await groveQuery(cycles)).haps;
      loaded = true;
    } catch {
      haps = [];
    } finally {
      loading = false;
    }
  }

  return {
    get haps()       { return haps; },
    get lanes()      { return lanes; },
    get loading()    { return loading; },
    get loaded()     { return loaded; },
    get empty()      { return haps.length === 0; },
    /** Onset of the last hap in cycles (0 when empty) — the "song end" marker. */
    get contentEnd() { return contentEnd; },

    /** Re-query the arrangement now. */
    refresh(cycles = VIEW_CYCLES) { return run(cycles); },

    /** Coalesced re-query — call from the eval/diagnostics trigger. The inline
     *  `grove_eval` result and the `grove:diagnostics` echo both bump the trigger;
     *  the debounce folds them into a single query. */
    schedule(cycles = VIEW_CYCLES) {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => { timer = null; void run(cycles); }, 140);
    },
  };
}

export const arrangementStore = createArrangementStore();

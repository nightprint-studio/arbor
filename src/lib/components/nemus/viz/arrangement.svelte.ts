/**
 * Arrangement viz data — the real timeline behind {@link ArrangementView}.
 *
 * The live event streams (transport / meters / active-haps) are owned by the
 * engine store; this one owns the *one* pull the arrangement needs: `nemus_query`
 * over `[0, VIEW_CYCLES)` returns every hap of the last-evaluated `Tracks` off
 * the audio thread (see the Step-1 gate — viz data is a query, not the mock).
 *
 * It is a viz-local store so the view consumes it instead of invoking IPC inline:
 * `schedule()` is fired from the eval/diagnostics trigger (debounced so the
 * inline `nemus_eval` result + the `nemus:diagnostics` echo coalesce into one
 * query); the arrangement is static between evals, so the playhead never re-queries.
 */

import { nemusQuery, type NemusQueryHap, type NemusQuerySection } from '$lib/ipc/nemus';

/** Cycle window queried + drawn (matches the arrangement grid width). */
export const VIEW_CYCLES = 96;

/** One lane = one mixer strip = one `track` index in the query. */
export interface VizLane {
  /** Mixer-strip / arrangement index (0-based) — the stable key into the
   *  shared mute/solo store and the mixer (Step 3b). */
  track: number;
  /** This lane's haps, sorted by onset. */
  haps: NemusQueryHap[];
  /** This lane's named section bands (tiled across the window), by start. */
  sections: NemusQuerySection[];
  /** Distinct sound names, most-frequent first (sample/drum character). */
  sounds: string[];
  /** Lowest / highest MIDI note across the lane, or null when unpitched. */
  noteLo: number | null;
  noteHi: number | null;
  /** A continuous signal (no onset) is present — drawn as a band, not blocks. */
  hasContinuous: boolean;
  /** Count of pitched haps (for the lane subtitle). */
  noteCount: number;
  /** Max simultaneous voices in the lane (peak polyphony) — the static "voice
   *  cost" of the track, used for the heaviest-track / budget readout. */
  polyphony: number;
}

/** Peak simultaneous voices across a lane's haps: a sweep line over onsets/offsets
 *  (offset before onset at the same instant, so touching notes don't overlap),
 *  plus a floor of one sustained voice per continuous signal. */
function maxPolyphony(haps: NemusQueryHap[]): number {
  let sustained = 0;
  const edges: { t: number; d: number }[] = [];
  for (const h of haps) {
    if (h.has_onset) {
      edges.push({ t: h.start, d: 1 }, { t: h.end, d: -1 });
    } else {
      sustained++;
    }
  }
  edges.sort((a, b) => a.t - b.t || a.d - b.d);
  let cur = 0;
  let max = 0;
  for (const e of edges) { cur += e.d; if (cur > max) max = cur; }
  return max + sustained;
}

const NOTE_NAMES = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];

/** MIDI → scientific pitch name (C4 = 60), for lane subtitles / tooltips. */
export function noteName(midi: number): string {
  const n = Math.round(midi);
  return `${NOTE_NAMES[((n % 12) + 12) % 12]}${Math.floor(n / 12) - 1}`;
}

/** Group the flat hap list into per-track lanes, deriving each lane's character
 *  (sounds / pitch range) so the header + roll can render without re-scanning.
 *  Named section bands are attached to their owning lane (by track index). */
function buildLanes(haps: NemusQueryHap[], allSections: NemusQuerySection[]): VizLane[] {
  const byTrack = new Map<number, NemusQueryHap[]>();
  for (const h of haps) {
    const arr = byTrack.get(h.track);
    if (arr) arr.push(h);
    else byTrack.set(h.track, [h]);
  }
  const secByTrack = new Map<number, NemusQuerySection[]>();
  for (const s of allSections) {
    const arr = secByTrack.get(s.track);
    if (arr) arr.push(s);
    else secByTrack.set(s.track, [s]);
    // A section-only track (e.g. a silent intro) still deserves a lane.
    if (!byTrack.has(s.track)) byTrack.set(s.track, []);
  }

  const lanes: VizLane[] = [];
  for (const track of [...byTrack.keys()].sort((a, b) => a - b)) {
    const hs = byTrack.get(track)!.slice().sort((a, b) => a.start - b.start);
    const sections = (secByTrack.get(track) ?? []).slice().sort((a, b) => a.start - b.start);
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
    lanes.push({ track, haps: hs, sections, sounds, noteLo, noteHi, hasContinuous, noteCount, polyphony: maxPolyphony(hs) });
  }
  return lanes;
}

function createArrangementStore() {
  let haps     = $state<NemusQueryHap[]>([]);
  let sections = $state<NemusQuerySection[]>([]);
  let loopCycles = $state(0);
  // The arrangement's effective render tempo (cps), captured from the query so a
  // passive estimate stays correct without the transport running. null until an
  // eval that set a tempo/cps; the consumer falls back to the configured default.
  let cps      = $state<number | null>(null);
  let loading  = $state(false);
  let loaded   = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;

  const lanes      = $derived(buildLanes(haps, sections));
  const contentEnd = $derived(haps.reduce((m, h) => Math.max(m, h.end), 0));
  /** The section bands of the first lane that has any — a representative song
   *  structure for the ruler chip strip (per-lane bands draw on each lane). */
  const rulerSections = $derived(lanes.find((l) => l.sections.length)?.sections ?? []);

  async function run(cycles: number) {
    loading = true;
    try {
      const res = await nemusQuery(cycles);
      haps = res.haps;
      sections = res.sections;
      loopCycles = res.loop_cycles;
      cps = res.cps;
      loaded = true;
    } catch {
      haps = [];
      sections = [];
      loopCycles = 0;
      cps = null;
    } finally {
      loading = false;
    }
  }

  return {
    get haps()       { return haps; },
    get lanes()      { return lanes; },
    /** Representative section bands for the ruler chip strip. */
    get rulerSections() { return rulerSections; },
    get loading()    { return loading; },
    get loaded()     { return loaded; },
    get empty()      { return haps.length === 0; },
    /** Onset of the last hap in cycles (0 when empty) — the "song end" marker. */
    get contentEnd() { return contentEnd; },
    /** Loop period of the arrangement in cycles (0 when empty/not loaded) — the
     *  natural render-length default and duration/size estimate base. */
    get loopCycles() { return loopCycles; },
    /** Effective render tempo (cps) of the evaluated arrangement, or null when the
     *  script set neither tempo nor cps (use the configured default). */
    get cps() { return cps; },

    /** Re-query the arrangement now. */
    refresh(cycles = VIEW_CYCLES) { return run(cycles); },

    /** Coalesced re-query — call from the eval/diagnostics trigger. The inline
     *  `nemus_eval` result and the `nemus:diagnostics` echo both bump the trigger;
     *  the debounce folds them into a single query. */
    schedule(cycles = VIEW_CYCLES) {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => { timer = null; void run(cycles); }, 140);
    },
  };
}

export const arrangementStore = createArrangementStore();

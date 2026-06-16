/**
 * Offline level analysis — the **accurate** half of clip detection, on demand.
 *
 * Unlike the runtime clip lights (live meters, only while playing) and the static
 * gain lint (authored boosts only), this measures the *real* per-track post-fader
 * peaks by rendering the loop **silently** in the backend (`nemus_analyze_levels`),
 * so it catches the sum-of-voices overloads the others can't — without the user
 * starting playback. It's a **snapshot**: run it from "Check levels", and the
 * result (mixer clip LEDs + the in-editor red underlines on the clipping notes)
 * persists until the next edit invalidates it (see {@link clear}).
 *
 * The editor marks are derived by intersecting the snapshot's clip windows (cycle
 * ranges per track) with the live query haps (which carry source spans), so the
 * offending notes underline without the backend having to attribute spans.
 */

import { nemusAnalyzeLevels, type NemusClipWindow } from '$lib/ipc/nemus';
import { arrangementStore } from '../viz/arrangement.svelte';

/** A clip-risk editor mark: a source byte-range + a hover message. */
interface ClipMark { from: number; to: number; message: string; }

function createLevelAnalysisStore() {
  let clips = $state<NemusClipWindow[]>([]);
  let trackPeaks = $state<number[]>([]);
  let running = $state(false);
  let ran = $state(false);

  const clippedTracks = $derived(new Set(clips.map((c) => c.track)));

  // Editor marks: the source span of every hap that sounds inside a clip window on
  // its own track. Derived from the snapshot clips + the live haps, deduped by span.
  const marks = $derived.by<ClipMark[]>(() => {
    if (!clips.length) return [];
    const haps = arrangementStore.haps;
    const seen = new Set<string>();
    const out: ClipMark[] = [];
    for (const h of haps) {
      if (h.span_start == null || h.span_end == null) continue;
      const w = clips.find((c) => c.track === h.track && h.start < c.end && h.end > c.start);
      if (!w) continue;
      const key = `${h.span_start}:${h.span_end}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const over = 20 * Math.log10(w.peak);
      out.push({ from: h.span_start, to: h.span_end, message: `Clips here — ${over.toFixed(1)} dB over full scale` });
    }
    return out;
  });

  return {
    get clips() { return clips; },
    get trackPeaks() { return trackPeaks; },
    get running() { return running; },
    /** True once an analysis has been run (vs. never) — for the result summary. */
    get ran() { return ran; },
    /** Clip-risk marks for the editor underline (clip windows ∩ live haps). */
    get marks() { return marks; },
    /** Did track `i` clip anywhere in the analysed loop? */
    isClipped(i: number): boolean { return clippedTracks.has(i); },
    /** How many distinct tracks clipped. */
    get clippedCount(): number { return clippedTracks.size; },

    /** Run the offline analysis on `source` (no playback). Concurrent calls are
     *  ignored while one is in flight. */
    async analyze(source: string, projectDir?: string): Promise<void> {
      if (running) return;
      running = true;
      try {
        const res = await nemusAnalyzeLevels(source, projectDir);
        clips = res.clips;
        trackPeaks = res.track_peaks;
        ran = true;
      } catch {
        clips = [];
        trackPeaks = [];
      } finally {
        running = false;
      }
    },
    /** Drop the snapshot (the LEDs + underlines clear) — called when the source
     *  changes, so a stale analysis never lingers over edited code. */
    clear(): void {
      if (!clips.length && !ran) return;
      clips = [];
      trackPeaks = [];
      ran = false;
    },
  };
}

export const levelAnalysisStore = createLevelAnalysisStore();

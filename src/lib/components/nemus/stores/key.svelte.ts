/**
 * Live key/scale detection for the editor — a weighted pitch-class histogram fit
 * to the best-covering scale, plus the notes that fall *outside* it.
 *
 * It is **derived from data the FE already holds**: the arrangement query
 * (`arrangementStore.haps`, each with a MIDI `note` and its source byte-span) and
 * the scale catalogue (`scalesStore.modes`, with one-octave intervals). So it
 * re-runs reactively on every eval without a second backend evaluation — no IPC,
 * no extra cost on the audio side.
 *
 * The candidate-mode set mirrors the importer's L2 detector
 * (`arbor-nemus-import/src/key.rs`) — the same curated shortlist, duplicated as
 * {@link DETECT_MODES}; keep the two in sync. The *fit metric* is stronger here,
 * though: instead of bare coverage it scores a light Krumhansl-style tonal
 * profile (tonic ≫ fifth > third > other), so it picks the right tonic on the
 * short, ambiguous fragments the live editor sees — where coverage alone ties
 * between scales centred on different roots.
 */

import type { NemusQueryHap, NemusScaleMode } from '$lib/ipc/nemus';
import { arrangementStore } from '../viz/arrangement.svelte';
import { scalesStore } from './scales.svelte';

/** Candidate modes tried during detection — the importer's curated set (the
 *  non-Western pentatonics included so a pentatonic piece isn't forced Western).
 *  Filtering the catalogue to these avoids over-fitting to an obscure mode (and
 *  excludes `chromatic`, which trivially "covers" everything). */
const DETECT_MODES = [
  'major', 'minor', 'dorian', 'phrygian', 'lydian', 'mixolydian', 'locrian',
  'harmonicminor', 'melodicminor', 'majpent', 'minpent', 'hirajoshi', 'insen',
  'iwato', 'kumoi',
];

/** Pitch-class → `.scale(...)` spec letter (matches the backend's `PC_NAMES`). */
const PC_SPEC = ['c', 'cs', 'd', 'ef', 'e', 'f', 'fs', 'g', 'af', 'a', 'bf', 'b'];
/** Pitch-class → human label (sharps/flats picked to read musically). */
const PC_LABEL = ['C', 'C♯', 'D', 'E♭', 'E', 'F', 'F♯', 'G', 'A♭', 'A', 'B♭', 'B'];

/** Tie-break prior: most common (Western tonal) scales first. When two scales
 *  cover the material equally well *and* are the same size, the more common one
 *  wins — so an ambiguous chord like Cm(maj7) plus a chromatic note reads as the
 *  expected harmonic / melodic minor, not a modal scale that merely happens to
 *  cover the same notes. Genuinely modal/pentatonic pieces still win on coverage. */
const MODE_PRIOR = [
  'major', 'minor', 'harmonicminor', 'melodicminor', 'dorian', 'mixolydian',
  'lydian', 'phrygian', 'locrian', 'majpent', 'minpent', 'hirajoshi', 'insen',
  'iwato', 'kumoi',
];
function modeRank(name: string): number {
  const i = MODE_PRIOR.indexOf(name);
  return i < 0 ? MODE_PRIOR.length : i;
}

/** Human display name for a mode (the catalogue keys read awkwardly). */
const MODE_LABEL: Record<string, string> = {
  harmonicminor: 'harmonic minor',
  melodicminor: 'melodic minor',
  majpent: 'major pentatonic',
  minpent: 'minor pentatonic',
};
const modeLabel = (m: string) => MODE_LABEL[m] ?? m;

const pc = (midi: number) => (((Math.round(midi) % 12) + 12) % 12);
/** MIDI → scientific note label (`61` → `C♯4`), for the hover message. */
const noteLabel = (midi: number) => `${PC_LABEL[pc(midi)]}${Math.floor(Math.round(midi) / 12) - 1}`;

/** One note outside the detected scale, with its source byte-range (for the
 *  editor underline) and the offending pitch. */
export interface OffScaleNote {
  /** Source byte-range start (UTF-8) — converted to a CM offset by the editor. */
  from: number;
  /** Source byte-range end (UTF-8). */
  to: number;
  /** MIDI pitch of the offending note. */
  note: number;
  /** Hover message, e.g. `C♯4 isn't in C harmonic minor`. */
  message: string;
}

/** The result of one detection pass. `spec` is null when there's no pitched
 *  material (a drums-only arrangement, or nothing loaded). */
export interface KeyAnalysis {
  /** `.scale(...)` spec, e.g. `"ef:dorian"`, or null when undetected. */
  spec: string | null;
  /** Mode name (`"dorian"`), or null. */
  mode: string | null;
  /** Human label (`"E♭ dorian"`), or null. */
  label: string | null;
  /** Weighted fraction of note-time inside the detected scale (0..1). */
  coverage: number;
  /** Pitched onsets considered. */
  noteCount: number;
  /** Distinct out-of-scale notes (deduped by source span). */
  offScale: OffScaleNote[];
}

const EMPTY: KeyAnalysis = { spec: null, mode: null, label: null, coverage: 0, noteCount: 0, offScale: [] };

/** Pitch classes a scale admits, given its root pc and one-octave intervals. */
function scalePcs(root: number, intervals: number[]): Set<number> {
  return new Set(intervals.map((iv) => (((root + iv) % 12) + 12) % 12));
}

/** Fit the best scale to the pattern's pitched material and collect the notes
 *  that fall outside it. Pure — feed it the query haps + the scale catalogue. */
export function detectKey(haps: NemusQueryHap[], modes: NemusScaleMode[]): KeyAnalysis {
  // Weighted pitch-class histogram over discrete pitched onsets (weight = duration).
  const hist = new Array<number>(12).fill(0);
  let total = 0;
  let noteCount = 0;
  for (const h of haps) {
    if (!h.has_onset || h.note == null) continue;
    const w = Math.max(1e-6, h.end - h.start);
    hist[pc(h.note)] += w;
    total += w;
    noteCount++;
  }
  if (total <= 0) return EMPTY;

  const candidates = modes.filter((m) => DETECT_MODES.includes(m.name));
  if (!candidates.length) return EMPTY;

  // Best fit by a **tonal-profile score** (a light Krumhansl-style weighting),
  // not bare coverage: each in-scale degree contributes its histogram weight times
  // a tonal-function weight (tonic ≫ fifth > third > other). This picks the right
  // *tonic*, so a Cm(maj7) phrase reads as C minor — whose root/third/fifth are
  // present and heavy — rather than some other scale that merely covers the same
  // pitch classes (e.g. F minor or C phrygian, which also contain 4 of the 5
  // notes but don't centre on C). Ties → smaller (more specific) scale; then the
  // more common scale (`MODE_PRIOR`). `coverage` is still computed, for display.
  type Fit = { root: number; mode: string; intervals: number[]; score: number; coverage: number; rank: number };
  const better = (c: Fit, b: Fit): boolean => {
    if (c.score > b.score + 1e-9) return true;
    if (c.score < b.score - 1e-9) return false;
    if (c.intervals.length !== b.intervals.length) return c.intervals.length < b.intervals.length;
    return c.rank < b.rank;
  };
  let best: Fit | null = null;
  for (const m of candidates) {
    const rank = modeRank(m.name);
    for (let root = 0; root < 12; root++) {
      let score = 0;
      let covSum = 0;
      for (const iv of m.intervals) {
        const p = (((root + iv) % 12) + 12) % 12;
        // Tonal function of this degree: tonic, perfect fifth, the third, else a
        // plain scale tone.
        const fn = iv === 0 ? 5 : iv === 7 ? 3 : iv === 3 || iv === 4 ? 2 : 1;
        score += hist[p] * fn;
        covSum += hist[p];
      }
      const cand: Fit = { root, mode: m.name, intervals: m.intervals, score, coverage: covSum / total, rank };
      if (!best || better(cand, best)) best = cand;
    }
  }
  if (!best) return EMPTY;

  const label = `${PC_LABEL[best.root]} ${modeLabel(best.mode)}`;

  // Out-of-scale notes, deduped by source span (a looped query repeats each note).
  const inScale = scalePcs(best.root, best.intervals);
  const seen = new Set<string>();
  const offScale: OffScaleNote[] = [];
  for (const h of haps) {
    if (!h.has_onset || h.note == null || h.span_start == null || h.span_end == null) continue;
    if (inScale.has(pc(h.note))) continue;
    const key = `${h.span_start}:${h.span_end}`;
    if (seen.has(key)) continue;
    seen.add(key);
    offScale.push({
      from: h.span_start,
      to: h.span_end,
      note: h.note,
      message: `${noteLabel(h.note)} isn't in ${label}`,
    });
  }

  return { spec: `${PC_SPEC[best.root]}:${best.mode}`, mode: best.mode, label, coverage: best.coverage, noteCount, offScale };
}

function createKeyStore() {
  // Reactive over the arrangement query + the scale catalogue — recomputed on
  // every eval (the haps are re-queried) with no extra backend work.
  const analysis = $derived.by<KeyAnalysis>(() =>
    detectKey(arrangementStore.haps, scalesStore.modes),
  );

  return {
    get analysis() { return analysis; },
    /** Out-of-scale note spans for the editor underline. */
    get offScale() { return analysis.offScale; },
    get hasOffScale() { return analysis.offScale.length > 0; },
  };
}

export const keyStore = createKeyStore();

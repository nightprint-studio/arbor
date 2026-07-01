/**
 * Static clip-risk lint — the cheap, **instant** half of clip detection.
 *
 * True clipping is the *sum* of simultaneous voices through the real synthesis +
 * FX, which only exists once the engine renders audio — that's what the runtime
 * clip lights (metersStore) and the offline level analysis measure. From the
 * source alone we can only flag the unambiguous code smell: an event whose
 * **authored gain is boosted well above unity** (`.gain(3)`), which asks for far
 * more level than headroom allows. Low-noise by design — it never guesses at
 * voice amplitudes, so it won't false-positive on ordinary chords; the offline
 * pass catches the polyphony-driven cases this can't see.
 *
 * Derived from the arrangement query (`arrangementStore.haps`, each with a
 * resolved `gain` and its source byte-span), so it re-runs reactively per eval
 * with no extra cost.
 */

import type { MerulaQueryHap } from '$lib/ipc/merula/merula';
import { arrangementStore } from '../viz/arrangement.svelte';

/** Authored gain at/above this (≈ +3.5 dB over unity) is flagged as clip-risk. */
const GAIN_HOT = 1.5;

/** One clip-risk event with its source byte-range + a hover message. */
export interface ClipRiskMark {
  from: number;
  to: number;
  message: string;
}

/** Flag events whose authored gain is boosted well above unity. Deduped by span
 *  (a looped query repeats each event). */
export function detectClipRisk(haps: MerulaQueryHap[]): ClipRiskMark[] {
  const seen = new Set<string>();
  const out: ClipRiskMark[] = [];
  for (const h of haps) {
    if (h.gain == null || h.gain < GAIN_HOT) continue;
    if (h.span_start == null || h.span_end == null) continue;
    const key = `${h.span_start}:${h.span_end}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const db = 20 * Math.log10(h.gain);
    out.push({
      from: h.span_start,
      to: h.span_end,
      message: `gain ${h.gain.toFixed(2)} (+${db.toFixed(1)} dB) — high level, may clip`,
    });
  }
  return out;
}

function createClipLintStore() {
  const marks = $derived.by<ClipRiskMark[]>(() => detectClipRisk(arrangementStore.haps));
  return {
    get marks() { return marks; },
    get hasRisk() { return marks.length > 0; },
  };
}

export const clipLintStore = createClipLintStore();

/**
 * Tempo store — live **tap-tempo** + **nudge** on top of the engine clock.
 *
 * Both push a live cps override through `merulaEngine.setCps` (staged on the BE at
 * the next cycle boundary). Like the mixer gain/pan overrides — and mirroring the
 * BE, which re-applies the script's `cps(...)` on every eval — the override is a
 * **live tweak released on the next eval** ({@link reset}, wired from the eval
 * signal in MerulaShell). The source stays authoritative; nothing is written back.
 *
 * Musicians think in BPM, the engine clock in cps (cycles/sec). One cycle = one
 * bar = 4 beats, so `bpm = cps · 4 · 60 = cps · 240`.
 */

import { merulaEngine, transportStore } from './engine.svelte';
import { arrangementStore } from '../viz/arrangement.svelte';
import { configStore } from './config.svelte';

export const BEATS_PER_CYCLE = 4;
export const MIN_BPM = 20;
export const MAX_BPM = 300;

export function cpsToBpm(cps: number): number {
  return cps * BEATS_PER_CYCLE * 60;
}
export function bpmToCps(bpm: number): number {
  return bpm / (BEATS_PER_CYCLE * 60);
}

// A tap sequence resets after this idle gap; the tempo is averaged over the last
// few inter-tap intervals (longer = steadier, slower to react).
const TAP_RESET_MS = 2000;
const TAP_WINDOW = 5; // intervals averaged → up to TAP_WINDOW + 1 taps kept

const clampBpm = (bpm: number): number => Math.max(MIN_BPM, Math.min(MAX_BPM, bpm));

function createTempoStore() {
  // Live cps override; null = follow the engine / evaluated source tempo.
  let override = $state<number | null>(null);
  // Tap timestamps (ms, monotonic). Plain array — not reactive (no display use).
  let taps: number[] = [];

  /** The cps the controls act on: the override if set, else the live transport
   *  tempo while playing, else the evaluated arrangement's cps, else the default. */
  function baseCps(): number {
    if (override != null) return override;
    if (transportStore.playing) return transportStore.cps;
    return arrangementStore.cps ?? configStore.defaultCps;
  }

  function applyBpm(bpm: number) {
    const cps = bpmToCps(clampBpm(bpm));
    override = cps;
    void merulaEngine.setCps(cps);
  }

  return {
    /** The effective BPM to display (the caller rounds for the readout). */
    get bpm() { return cpsToBpm(baseCps()); },
    /** Whether a live override is in effect (vs following the source tempo). */
    get overridden() { return override != null; },

    /** Nudge the tempo by `deltaBpm` (±), relative to the current effective tempo. */
    nudge(deltaBpm: number) { applyBpm(cpsToBpm(baseCps()) + deltaBpm); },

    /** Register one tap. From the second tap on, sets the tempo to the averaged
     *  inter-tap interval; a gap longer than {@link TAP_RESET_MS} starts fresh. */
    tap() {
      const now = performance.now();
      if (taps.length && now - taps[taps.length - 1] > TAP_RESET_MS) taps = [];
      taps.push(now);
      if (taps.length > TAP_WINDOW + 1) taps = taps.slice(-(TAP_WINDOW + 1));
      if (taps.length >= 2) {
        const avgMs = (taps[taps.length - 1] - taps[0]) / (taps.length - 1);
        if (avgMs > 0) applyBpm(60_000 / avgMs);
      }
    },

    /** Drop the override (follow the source tempo again) and clear the tap run.
     *  Called on each eval (the source is authoritative again) and from the
     *  control's reset / command-palette entry. */
    reset() { override = null; taps = []; },
  };
}

export const tempoStore = createTempoStore();

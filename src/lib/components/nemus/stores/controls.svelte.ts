/**
 * Track-controls store — the **source literal** values behind the room / delay
 * knobs (Fase 4 · nemus_set_literal).
 *
 * Unlike gain / pan (live ephemeral overrides pushed to the audio session), room
 * and delay have no track-level audio command — they are *code-first*. So their
 * knobs reflect the literal currently in the `.nemus` source and commit straight
 * back to it. This store parses the active source with the shared Tree-sitter
 * parser and extracts each track's `room` / `delay` (and `gain`/`pan`) literals,
 * re-parsing on every eval (symmetric with {@link arrangementStore}).
 *
 * It carries only the *seed* values; the commit itself is the editor's job
 * (`nemus-edit.ts` → `NemusEditor.commitControls`, relayed via `nemusStore`).
 */

import { parseNemus } from '../editor/nemus-lang';
import {
  extractTrackControls,
  type TrackControls, type DelayValues, type CompValues, type EqBandValue,
} from '../editor/nemus-edit';
import { projectStore } from './project.svelte';

/** Which controls can be calculated (non-literal → not committable). */
export type ControlName = 'gain' | 'pan' | 'room' | 'delay' | 'comp';

function createControlsStore() {
  let byTrack = $state<Map<number, TrackControls>>(new Map());
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function run() {
    const src = projectStore.activeSource;
    try {
      const tree = await parseNemus(src);
      byTrack = tree ? extractTrackControls(tree) : new Map();
    } catch {
      byTrack = new Map(); // grammar wasm missing → no seeds (commits still no-op safely)
    }
  }

  return {
    /** Current gain literal for track `i`, or null when absent / calculated.
     *  Used by the mixer to snapshot a track's pre-mute gain before writing
     *  `.gain(0)` (mute), so unmute can restore it. */
    gain(i: number): number | null { return byTrack.get(i)?.gain ?? null; },
    /** Current room send literal for track `i` (0 when absent). */
    room(i: number): number { return byTrack.get(i)?.room ?? 0; },
    /** Whether track `i` has a room literal in source (vs. absent / calculated). */
    hasRoom(i: number): boolean { return byTrack.get(i)?.room != null; },
    /** Current delay literals for track `i`, or null when absent. */
    delay(i: number): DelayValues | null { return byTrack.get(i)?.delay ?? null; },
    /** Current parametric-EQ bands for track `i` (source order), or empty. */
    eq(i: number): EqBandValue[] { return byTrack.get(i)?.eq ?? []; },
    /** Current compressor literals for track `i`, or null when absent. */
    comp(i: number): CompValues | null { return byTrack.get(i)?.comp ?? null; },
    /** Whether control `k` of track `i` is a calculated (non-literal) argument —
     *  committing it would be a no-op, so the UI marks it read-only. */
    isCalculated(i: number, k: ControlName): boolean {
      return byTrack.get(i)?.calculated.has(k) ?? false;
    },

    /** Re-parse the active source now. */
    refresh() { return run(); },
    /** Coalesced re-parse — call from the eval trigger (debounced; folds the
     *  inline eval result + the diagnostics echo into one parse). */
    schedule() {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => { timer = null; void run(); }, 160);
    },
  };
}

export const controlsStore = createControlsStore();

/**
 * Track-controls store — the **source literal** values behind the room / delay
 * knobs (Fase 4 · grove_set_literal).
 *
 * Unlike gain / pan (live ephemeral overrides pushed to the audio session), room
 * and delay have no track-level audio command — they are *code-first*. So their
 * knobs reflect the literal currently in the `.grove` source and commit straight
 * back to it. This store parses the active source with the shared Tree-sitter
 * parser and extracts each track's `room` / `delay` (and `gain`/`pan`) literals,
 * re-parsing on every eval (symmetric with {@link arrangementStore}).
 *
 * It carries only the *seed* values; the commit itself is the editor's job
 * (`grove-edit.ts` → `GroveEditor.commitControls`, relayed via `groveStore`).
 */

import { parseGrove } from '../editor/grove-lang';
import { extractTrackControls, type TrackControls, type DelayValues } from '../editor/grove-edit';
import { projectStore } from './project.svelte';

/** Which controls can be calculated (non-literal → not committable). */
export type ControlName = 'gain' | 'pan' | 'room' | 'delay';

function createControlsStore() {
  let byTrack = $state<Map<number, TrackControls>>(new Map());
  let timer: ReturnType<typeof setTimeout> | null = null;

  async function run() {
    const src = projectStore.activeSource;
    try {
      const tree = await parseGrove(src);
      byTrack = tree ? extractTrackControls(tree) : new Map();
    } catch {
      byTrack = new Map(); // grammar wasm missing → no seeds (commits still no-op safely)
    }
  }

  return {
    /** Current room send literal for track `i` (0 when absent). */
    room(i: number): number { return byTrack.get(i)?.room ?? 0; },
    /** Whether track `i` has a room literal in source (vs. absent / calculated). */
    hasRoom(i: number): boolean { return byTrack.get(i)?.room != null; },
    /** Current delay literals for track `i`, or null when absent. */
    delay(i: number): DelayValues | null { return byTrack.get(i)?.delay ?? null; },
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

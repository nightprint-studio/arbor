/**
 * Clip launcher — scene grid state, live selection, and launch quantization.
 *
 * `scene(...)` declarations are exposed by the backend (`merula_scenes`): the base
 * track names (columns) and the declared scenes (rows). This store fetches that
 * metadata (refreshed on every eval) and owns the **live selection** — which
 * scene's clip each track is currently playing.
 *
 * `applied` is the **UI truth** and updates immediately on every launch/stop, so
 * the grid always reflects what you clicked (regardless of whether the song is
 * playing). The backend swap is a separate concern: while the song plays it's
 * deferred to the next quantization grid line (1/2/4 cycles) so it lands in time,
 * and the cells whose backend swap hasn't landed yet **pulse** (`queued`). While
 * stopped, launching applies on the next play (via {@link resync}) and also starts
 * the song so the clip is audible right away (Ableton-style).
 */

import { merulaScenes, merulaLaunch, type MerulaScene, type MerulaClipSelection } from '$lib/ipc/merula';
import { transportStore, merulaEngine } from './engine.svelte';

/** Allowed launch grids, in cycles. */
const QUANTA = [1, 2, 4] as const;

type Sel = Record<number, string>;

function mapsEqual(a: Sel, b: Sel): boolean {
  const ak = Object.keys(a);
  const bk = Object.keys(b);
  if (ak.length !== bk.length) return false;
  return ak.every((k) => a[+k] === b[+k]);
}

/** Track indices whose scene differs between two selections. */
function diffTracks(a: Sel, b: Sel): number[] {
  const keys = new Set<number>([...Object.keys(a), ...Object.keys(b)].map(Number));
  return [...keys].filter((i) => a[i] !== b[i]);
}

/** The next quantization grid line strictly ahead of the current cycle. */
function nextBoundary(cycle: number, quantum: number): number {
  return (Math.floor(cycle / quantum) + 1) * quantum;
}

function createLauncherStore() {
  let tracks = $state<string[]>([]);
  let scenes = $state<MerulaScene[]>([]);
  let loading = $state(false);

  // The live selection — the UI truth (track index → scene). Set immediately on
  // every launch/stop. Absent = base pattern.
  let applied = $state<Sel>({});
  // Tracks whose backend swap is still queued for the next grid line (they pulse).
  let queued = $state<Set<number>>(new Set());
  // The selection + grid line the deferred backend push targets. Plain (not
  // reactive): only the transport tick reads them.
  let pushSel: Sel | null = null;
  let pushAt = 0;
  let quantum = $state<number>(1);

  /** Push a concrete selection to the backend (boundary-quantized there too). */
  function push(sel: Sel): void {
    const payload: MerulaClipSelection[] = Object.entries(sel).map(([track, scene]) => ({
      track: Number(track),
      scene,
    }));
    void merulaLaunch(payload);
  }

  /** The resolved track indices a scene declares clips for (skips inert clips). */
  function sceneTracks(name: string): number[] {
    const s = scenes.find((x) => x.name === name);
    if (!s) return [];
    return s.clips.map((c) => c.track_index).filter((i): i is number => i != null);
  }

  /** Apply `desired` to the UI **immediately**, then route the backend swap by
   *  transport state: deferred to the grid line while playing (the changed cells
   *  pulse meanwhile); applied on the next play while stopped (which it also
   *  starts). A stop/clear while stopped just updates the selection. */
  function arm(desired: Sel): void {
    if (mapsEqual(desired, applied)) return;
    const changed = diffTracks(applied, desired);
    applied = desired; // ← UI truth, updated synchronously so the grid always reflects the click
    if (!transportStore.playing) {
      queued = new Set();
      pushSel = null;
      if (Object.keys(desired).length) void merulaEngine.play();
      return;
    }
    // Playing: pulse the changed cells, defer the backend swap to the next grid line.
    queued = new Set([...queued, ...changed]);
    pushSel = { ...desired };
    pushAt = nextBoundary(transportStore.cycle, quantum);
  }

  return {
    /** Base track names — the launcher columns, in mixer order. */
    get tracks() { return tracks; },
    /** Declared scenes — the launcher rows. */
    get scenes() { return scenes; },
    /** Whether any scene is declared (else the panel shows an empty state). */
    get hasScenes() { return scenes.length > 0; },
    /** Whether any clip is currently launched (for the "stop all" affordance). */
    get anyActive() { return Object.keys(applied).length > 0; },
    /** The current launch grid, in cycles (1 / 2 / 4). */
    get quantum() { return quantum; },

    /** The scene whose clip track `i` is currently playing, or null (base). */
    activeOf(i: number): string | null { return applied[i] ?? null; },
    /** Is track `i` playing scene `name`'s clip right now? */
    isActive(i: number, name: string): boolean { return applied[i] === name; },
    /** Is track `i`'s backend swap still queued for the grid line (so it pulses
     *  until the audio actually changes)? */
    isQueued(i: number): boolean { return queued.has(i); },
    /** Are all of scene `name`'s clips currently launched (full-row highlight)? */
    isSceneActive(name: string): boolean {
      const ts = sceneTracks(name);
      return ts.length > 0 && ts.every((i) => applied[i] === name);
    },

    // ── Actions ────────────────────────────────────────────────────────────────

    /** Launch one clip: track `i` plays scene `name`'s clip. */
    launchClip(i: number, name: string): void {
      const next = { ...applied };
      next[i] = name;
      arm(next);
    },
    /** Stop track `i`: back to its base pattern. */
    stopTrack(i: number): void {
      if (!(i in applied)) return;
      const next = { ...applied };
      delete next[i];
      arm(next);
    },
    /** Launch a whole scene (row) as the complete picture: every track that has a
     *  clip in it plays that clip, and **every other track returns to base** — an
     *  empty cell in the row stops whatever that track was playing (Ableton scene
     *  semantics). To mix clips across scenes, launch individual cells instead. */
    launchScene(name: string): void {
      const ts = sceneTracks(name);
      if (!ts.length) return;
      const next: Sel = {};
      for (const i of ts) next[i] = name;
      arm(next);
    },
    /** Stop everything: all tracks back to base. */
    stopAll(): void { arm({}); },

    /** Cycle the launch quantization 1 → 2 → 4 → 1. */
    cycleQuantum(): void {
      const idx = QUANTA.indexOf(quantum as (typeof QUANTA)[number]);
      quantum = QUANTA[(idx + 1) % QUANTA.length];
    },
    /** Set the launch quantization directly. */
    setQuantum(q: number): void { if (QUANTA.includes(q as (typeof QUANTA)[number])) quantum = q; },

    /** Re-send the applied selection (called on play-start, since staging the base
     *  on Play would otherwise overwrite a selection set while stopped). */
    resync(): void {
      if (Object.keys(applied).length) push(applied);
    },

    /** Transport stopped: drop the live selection so the cells go idle and the next
     *  play starts from the base. The one transport Stop clears the launcher too. */
    onStop(): void {
      if (!Object.keys(applied).length && queued.size === 0) return;
      applied = {};
      queued = new Set();
      pushSel = null;
    },

    /** Transport tick (driven by MerulaShell): fire a deferred backend swap at the
     *  grid line and clear the pulse once it lands. The UI (`applied`) is already
     *  up to date — this only governs the audio timing + the pulse. */
    onTransport(cycle: number, playing: boolean): void {
      if (pushSel == null && queued.size === 0) return;
      if (!playing) { pushSel = null; queued = new Set(); return; }
      // Fire one cycle before the target so the backend's own next boundary is the
      // target line; clear the pulse when the line is actually crossed.
      if (pushSel != null && cycle >= pushAt - 1) { push(pushSel); pushSel = null; }
      if (queued.size && cycle >= pushAt) { queued = new Set(); }
    },

    /** (Re)fetch the scene grid from the last evaluation, pruning any selection
     *  entries the new code no longer supports. Cheap; called on eval. */
    async load(): Promise<void> {
      if (loading) return;
      loading = true;
      try {
        const res = await merulaScenes();
        tracks = res.tracks;
        scenes = res.scenes;
        // A re-eval invalidates any queued swap (the grid may have changed).
        queued = new Set();
        pushSel = null;
        // Drop applied entries whose scene/clip no longer exists, re-syncing the
        // backend if the live override changed underneath the new code.
        const valid = (i: number, name: string) => {
          const s = res.scenes.find((x) => x.name === name);
          return !!s && s.clips.some((c) => c.track_index === i);
        };
        let changed = false;
        const next: Sel = {};
        for (const [k, name] of Object.entries(applied)) {
          const i = Number(k);
          if (valid(i, name)) next[i] = name;
          else changed = true;
        }
        if (changed) { applied = next; push(next); }
      } catch {
        /* leave the previous grid in place on a transient failure */
      } finally {
        loading = false;
      }
    },
    /** Clear the grid + selection (e.g. on project close). */
    clear(): void {
      tracks = [];
      scenes = [];
      applied = {};
      queued = new Set();
      pushSel = null;
    },
  };
}

export const launcherStore = createLauncherStore();

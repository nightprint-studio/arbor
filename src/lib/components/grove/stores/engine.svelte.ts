/**
 * grove engine stores — **the FROZEN FE contract** the Step 2/3 fan-outs build
 * on. Each store wraps one live BE event stream from `$lib/ipc/grove.ts` in a
 * Svelte 5 rune store (the canonical factory + getters pattern). Subscriptions
 * are explicit: call `groveEngine.subscribe()` on mount and the returned
 * `UnlistenFn` on teardown — never auto-wired at module load (each grove window
 * is its own JS context and must own its listener lifetime).
 *
 * Frozen surface (do not reshape; extend additively):
 *   transportStore   ← grove:transport   (playing / cycle / frame / cps / sr)
 *   metersStore      ← grove:meters       (master + per-track peak / voices / dsp)
 *   diagnosticsStore ← grove:diagnostics  (+ the grove_eval inline result)
 *   activeHapsStore  ← grove:active_haps  (sounding source ranges, on-change)
 *   logStore         ← grove:log          (threshold-gated console lines)
 *   audioErrorStore  ← grove:audio_error  (device open failure)
 *   groveEngine      — transport actions (eval / run / stop / seek / setCps)
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  groveEval, grovePlay, groveStop, groveSeek, groveSetCps,
  onGroveTransport, onGroveMeters, onGroveDiagnostics, onGroveActiveHaps, onGroveLog,
  onGroveAudioError,
  type GroveDiagnostic, type GroveActiveHap, type GroveStereoPeak,
} from '$lib/ipc/grove';

// ── Transport ─────────────────────────────────────────────────────────────────

function createTransportStore() {
  let playing    = $state(false);
  let cycle      = $state(0);
  let frame      = $state(0);
  let cps        = $state(0.5);
  let sampleRate = $state(48_000);

  return {
    get playing()    { return playing; },
    get cycle()      { return cycle; },
    get frame()      { return frame; },
    get cps()        { return cps; },
    get sampleRate() { return sampleRate; },
    /** Cycle position formatted `cycle.beat` (4 beats/cycle), for the footer. */
    get position()   { return `${Math.floor(cycle)}.${Math.floor((cycle % 1) * 4) + 1}`; },
    subscribe(): Promise<UnlistenFn> {
      return onGroveTransport((t) => {
        playing = t.playing; cycle = t.cycle; frame = t.frame;
        cps = t.cps; sampleRate = t.sample_rate;
      });
    },
  };
}

// ── Meters / telemetry ──────────────────────────────────────────────────────────

function createMetersStore() {
  let master  = $state<GroveStereoPeak>([0, 0]);
  let tracks  = $state<GroveStereoPeak[]>([]);
  let voices  = $state(0);
  let dspLoad = $state(0);

  return {
    get master()  { return master; },
    get tracks()  { return tracks; },
    get voices()  { return voices; },
    get dspLoad() { return dspLoad; },
    /** Peak `[l,r]` for a track index, or `[0,0]` when absent. */
    peak(track: number): GroveStereoPeak { return tracks[track] ?? [0, 0]; },
    subscribe(): Promise<UnlistenFn> {
      return onGroveMeters((m) => {
        master = m.master; tracks = m.tracks; voices = m.voices; dspLoad = m.dsp_load;
      });
    },
  };
}

// ── Diagnostics (Problems) ──────────────────────────────────────────────────────

function createDiagnosticsStore() {
  let errors = $state<GroveDiagnostic[]>([]);

  return {
    get errors()    { return errors; },
    get hasErrors() { return errors.some((e) => e.severity === 'error'); },
    get count()     { return errors.length; },
    /** Replace the set (the `grove_eval` inline result feeds this too). */
    set(next: GroveDiagnostic[]) { errors = next; },
    clear() { errors = []; },
    subscribe(): Promise<UnlistenFn> {
      return onGroveDiagnostics((d) => { errors = d.errors; });
    },
  };
}

// ── Active haps (live editor highlight) ─────────────────────────────────────────

function createActiveHapsStore() {
  let haps = $state<GroveActiveHap[]>([]);

  return {
    get haps() { return haps; },
    /** True when a source byte-range is currently sounding (editor highlight). */
    isActive(start: number, end: number): boolean {
      return haps.some((h) => h.start === start && h.end === end);
    },
    subscribe(): Promise<UnlistenFn> {
      return onGroveActiveHaps((h) => { haps = h.haps; });
    },
  };
}

// ── Log (Console) ────────────────────────────────────────────────────────────────

/** One console line — the wire `{level,message}` plus a local monotonic id. */
export interface GroveLogEntry { id: number; level: string; message: string; }

/** Ring-buffer cap so a long session can't grow the console unbounded. */
const LOG_CAP = 2_000;

function createLogStore() {
  let lines = $state<GroveLogEntry[]>([]);
  let nextId = 0; // monotonic, not Date-based (deterministic, cheap)

  return {
    get lines() { return lines; },
    get count() { return lines.length; },
    clear() { lines = []; },
    subscribe(): Promise<UnlistenFn> {
      return onGroveLog((l) => {
        const next = [...lines, { id: nextId++, level: l.level, message: l.message }];
        lines = next.length > LOG_CAP ? next.slice(next.length - LOG_CAP) : next;
      });
    },
  };
}

// ── Audio error ──────────────────────────────────────────────────────────────────

function createAudioErrorStore() {
  let message = $state<string | null>(null);

  return {
    get message() { return message; },
    clear() { message = null; },
    subscribe(): Promise<UnlistenFn> {
      return onGroveAudioError((e) => { message = e.message; });
    },
  };
}

export const transportStore   = createTransportStore();
export const metersStore       = createMetersStore();
export const diagnosticsStore = createDiagnosticsStore();
export const activeHapsStore  = createActiveHapsStore();
export const logStore         = createLogStore();
export const audioErrorStore  = createAudioErrorStore();

// ── Engine controller (transport actions + one-call subscription) ────────────────

function createGroveEngine() {
  return {
    /** Whether the scheduler is running (delegates to the transport stream). */
    get running() { return transportStore.playing; },

    /** Wire every live stream; returns one `UnlistenFn` that detaches them all.
     *  Call on mount, invoke the result on teardown. */
    async subscribe(): Promise<UnlistenFn> {
      const uns = await Promise.all([
        transportStore.subscribe(),
        metersStore.subscribe(),
        diagnosticsStore.subscribe(),
        activeHapsStore.subscribe(),
        logStore.subscribe(),
        audioErrorStore.subscribe(),
      ]);
      return () => { for (const un of uns) un(); };
    },

    /** Evaluate `source`, push diagnostics, and return them. Does not play. */
    async eval(source: string, projectDir?: string): Promise<GroveDiagnostic[]> {
      const d = await groveEval(source, projectDir);
      diagnosticsStore.set(d.errors);
      return d.errors;
    },

    /** Evaluate then start playback (opens the audio device on first call).
     *  A failed eval leaves the last good arrangement staged, so play still
     *  sounds the previous result. */
    async run(source: string, projectDir?: string): Promise<void> {
      await this.eval(source, projectDir);
      await grovePlay();
    },

    /** Stop playback (the clock keeps its position). */
    async stop(): Promise<void> { await groveStop(); },

    /** Run if stopped, stop if running. */
    async toggleRun(source: string, projectDir?: string): Promise<void> {
      if (transportStore.playing) await this.stop();
      else await this.run(source, projectDir);
    },

    seek(cycle: number)  { return groveSeek(cycle); },
    setCps(cps: number)  { return groveSetCps(cps); },

    /** Jump the playhead back to cycle 0 (music-player skip-to-start). */
    seekToStart() { return groveSeek(0); },
    /** Jump the playhead to the last cycle of the arrangement (skip-to-end).
     *  `totalCycles` comes from the arrangement viz (its content end). */
    seekToEnd(totalCycles: number) { return groveSeek(Math.max(0, totalCycles)); },
  };
}

export const groveEngine = createGroveEngine();

/**
 * nemus engine stores — **the FROZEN FE contract** the Step 2/3 fan-outs build
 * on. Each store wraps one live BE event stream from `$lib/ipc/nemus.ts` in a
 * Svelte 5 rune store (the canonical factory + getters pattern). Subscriptions
 * are explicit: call `nemusEngine.subscribe()` on mount and the returned
 * `UnlistenFn` on teardown — never auto-wired at module load (each nemus window
 * is its own JS context and must own its listener lifetime).
 *
 * Frozen surface (do not reshape; extend additively):
 *   transportStore   ← nemus:transport   (playing / cycle / frame / cps / sr)
 *   metersStore      ← nemus:meters       (master + per-track peak / voices / dsp)
 *   diagnosticsStore ← nemus:diagnostics  (+ the nemus_eval inline result)
 *   activeHapsStore  ← nemus:active_haps  (sounding source ranges, on-change)
 *   logStore         ← nemus:log          (threshold-gated console lines)
 *   audioErrorStore  ← nemus:audio_error  (device open failure)
 *   nemusEngine      — transport actions (eval / run / stop / seek / setCps)
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  nemusEval, nemusPlay, nemusStop, nemusSeek, nemusSetCps,
  nemusPlaySnippet, nemusStopSnippet,
  onNemusTransport, onNemusMeters, onNemusDiagnostics, onNemusActiveHaps, onNemusLog,
  onNemusAudioError,
  type NemusDiagnostic, type NemusActiveHap, type NemusStereoPeak,
} from '$lib/ipc/nemus';

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
      return onNemusTransport((t) => {
        playing = t.playing; cycle = t.cycle; frame = t.frame;
        cps = t.cps; sampleRate = t.sample_rate;
      });
    },
  };
}

// ── Meters / telemetry ──────────────────────────────────────────────────────────

function createMetersStore() {
  let master  = $state<NemusStereoPeak>([0, 0]);
  let tracks  = $state<NemusStereoPeak[]>([]);
  let voices  = $state(0);
  let dspLoad = $state(0);

  return {
    get master()  { return master; },
    get tracks()  { return tracks; },
    get voices()  { return voices; },
    get dspLoad() { return dspLoad; },
    /** Peak `[l,r]` for a track index, or `[0,0]` when absent. */
    peak(track: number): NemusStereoPeak { return tracks[track] ?? [0, 0]; },
    subscribe(): Promise<UnlistenFn> {
      return onNemusMeters((m) => {
        master = m.master; tracks = m.tracks; voices = m.voices; dspLoad = m.dsp_load;
      });
    },
  };
}

// ── Diagnostics (Problems) ──────────────────────────────────────────────────────

function createDiagnosticsStore() {
  let errors = $state<NemusDiagnostic[]>([]);

  return {
    get errors()    { return errors; },
    get hasErrors() { return errors.some((e) => e.severity === 'error'); },
    get count()     { return errors.length; },
    /** Replace the set (the `nemus_eval` inline result feeds this too). */
    set(next: NemusDiagnostic[]) { errors = next; },
    clear() { errors = []; },
    subscribe(): Promise<UnlistenFn> {
      return onNemusDiagnostics((d) => { errors = d.errors; });
    },
  };
}

// ── Active haps (live editor highlight) ─────────────────────────────────────────

function createActiveHapsStore() {
  let haps = $state<NemusActiveHap[]>([]);

  return {
    get haps() { return haps; },
    /** True when a source byte-range is currently sounding (editor highlight). */
    isActive(start: number, end: number): boolean {
      return haps.some((h) => h.start === start && h.end === end);
    },
    subscribe(): Promise<UnlistenFn> {
      return onNemusActiveHaps((h) => { haps = h.haps; });
    },
  };
}

// ── Log (Console) ────────────────────────────────────────────────────────────────

/** One console line — the wire `{level,message}` plus a local monotonic id. */
export interface NemusLogEntry { id: number; level: string; message: string; }

/** Ring-buffer cap so a long session can't grow the console unbounded. */
const LOG_CAP = 2_000;

function createLogStore() {
  let lines = $state<NemusLogEntry[]>([]);
  let nextId = 0; // monotonic, not Date-based (deterministic, cheap)

  return {
    get lines() { return lines; },
    get count() { return lines.length; },
    clear() { lines = []; },
    subscribe(): Promise<UnlistenFn> {
      return onNemusLog((l) => {
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
      return onNemusAudioError((e) => { message = e.message; });
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

function createNemusEngine() {
  // Monotonic token: only the most recent eval's inline diagnostics may win.
  // Debounced edits can put two evals in flight (a fast failing one and a slow
  // succeeding one that stages samples), and they can resolve out of order — an
  // older result must never clobber a newer one's lint.
  let evalSeq = 0;

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

    /** Evaluate `source`, push diagnostics, and return them. Does not play.
     *  Stale inline results (a newer eval started meanwhile) are dropped so they
     *  can't overwrite fresher lint. */
    async eval(source: string, projectDir?: string): Promise<NemusDiagnostic[]> {
      const seq = ++evalSeq;
      const d = await nemusEval(source, projectDir);
      if (seq === evalSeq) diagnosticsStore.set(d.errors);
      return d.errors;
    },

    /** Evaluate then start playback (opens the audio device on first call).
     *  A failed eval leaves the last good arrangement staged, so play still
     *  sounds the previous result. */
    async run(source: string, projectDir?: string): Promise<void> {
      await this.eval(source, projectDir);
      await nemusPlay();
    },

    /** Stop playback (the clock keeps its position). */
    async stop(): Promise<void> { await nemusStop(); },

    /** Run if stopped, stop if running. */
    async toggleRun(source: string, projectDir?: string): Promise<void> {
      if (transportStore.playing) await this.stop();
      else await this.run(source, projectDir);
    },

    seek(cycle: number)  { return nemusSeek(cycle); },
    setCps(cps: number)  { return nemusSetCps(cps); },

    /** Play an arbitrary `.nemus` chunk **one-shot** on the audition bus (right-
     *  click→Play, Outline Play, Scratch panel). It sounds once over its detected
     *  period and stops on its own; the song transport is untouched. A blank /
     *  malformed snippet is a no-op. */
    playSnippet(source: string, projectDir?: string): Promise<void> {
      if (!source.trim()) return Promise.resolve();
      return nemusPlaySnippet(source, projectDir);
    },
    /** Stop an in-flight snippet preview early (clears the audition bus only). */
    stopSnippet(): Promise<void> { return nemusStopSnippet(); },

    /** Jump the playhead back to cycle 0 (music-player skip-to-start). */
    seekToStart() { return nemusSeek(0); },
    /** Jump the playhead to the last cycle of the arrangement (skip-to-end).
     *  `totalCycles` comes from the arrangement viz (its content end). */
    seekToEnd(totalCycles: number) { return nemusSeek(Math.max(0, totalCycles)); },
  };
}

export const nemusEngine = createNemusEngine();

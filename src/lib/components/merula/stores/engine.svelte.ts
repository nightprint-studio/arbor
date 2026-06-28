/**
 * merula engine stores — **the FROZEN FE contract** the Step 2/3 fan-outs build
 * on. Each store wraps one live BE event stream from `$lib/ipc/merula.ts` in a
 * Svelte 5 rune store (the canonical factory + getters pattern). Subscriptions
 * are explicit: call `merulaEngine.subscribe()` on mount and the returned
 * `UnlistenFn` on teardown — never auto-wired at module load (each merula window
 * is its own JS context and must own its listener lifetime).
 *
 * Frozen surface (do not reshape; extend additively):
 *   transportStore   ← merula:transport   (playing / cycle / frame / cps / sr)
 *   metersStore      ← merula:meters       (master + per-track peak / voices / dsp)
 *   diagnosticsStore ← merula:diagnostics  (+ the merula_eval inline result)
 *   activeHapsStore  ← merula:active_haps  (sounding source ranges, on-change)
 *   logStore         ← merula:log          (threshold-gated console lines)
 *   audioErrorStore  ← merula:audio_error  (device open failure)
 *   merulaEngine      — transport actions (eval / run / stop / seek / setCps)
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  merulaEval, merulaPlay, merulaStop, merulaSeek, merulaSetCps,
  merulaPlaySnippet, merulaStopSnippet,
  onMerulaTransport, onMerulaMeters, onMerulaDiagnostics, onMerulaActiveHaps, onMerulaLog,
  onMerulaAudioError,
  type MerulaDiagnostic, type MerulaActiveHap, type MerulaStereoPeak,
} from '$lib/ipc/merula';

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
      return onMerulaTransport((t) => {
        playing = t.playing; cycle = t.cycle; frame = t.frame;
        cps = t.cps; sampleRate = t.sample_rate;
      });
    },
  };
}

// ── Meters / telemetry ──────────────────────────────────────────────────────────

/** Peak at/over 0 dBFS (full scale) — a clip / overload. Per-track peaks are
 *  post-fader (pre-master-limiter) so they genuinely exceed this; the master is
 *  limited and rarely will. The indicator LATCHES (a DAW clip light) until reset. */
const CLIP_LEVEL = 1.0;

function createMetersStore() {
  let master  = $state<MerulaStereoPeak>([0, 0]);
  let tracks  = $state<MerulaStereoPeak[]>([]);
  let voices  = $state(0);
  let dspLoad = $state(0);
  let gainReduction = $state(0);
  // Latched clip state — set on any frame that reaches full scale, held until a
  // manual reset (or the next playthrough). A transient over-0 dBFS can show in a
  // single meter frame, so latching is what makes it catchable.
  let masterClipped = $state(false);
  let clippedTracks = $state<Set<number>>(new Set());

  const clips = (p: MerulaStereoPeak) => p[0] >= CLIP_LEVEL || p[1] >= CLIP_LEVEL;

  return {
    get master()  { return master; },
    get tracks()  { return tracks; },
    get voices()  { return voices; },
    get dspLoad() { return dspLoad; },
    /** Master limiter gain reduction `0..1` (`0` = none, larger = more ducking). */
    get gainReduction() { return gainReduction; },
    /** Peak `[l,r]` for a track index, or `[0,0]` when absent. */
    peak(track: number): MerulaStereoPeak { return tracks[track] ?? [0, 0]; },
    // ── Clip latch (over 0 dBFS) ──────────────────────────────────────────────
    /** Has the master output latched a clip since the last reset? */
    get masterClipped() { return masterClipped; },
    /** Has track `i` clipped (post-fader) since the last reset? */
    isClipped(i: number): boolean { return clippedTracks.has(i); },
    /** Any clip latched at all (master or a track). */
    get anyClipped(): boolean { return masterClipped || clippedTracks.size > 0; },
    /** Distinct latched sources, for the footer tooltip. */
    get clipCount(): number { return clippedTracks.size + (masterClipped ? 1 : 0); },
    /** Clear every latched clip (the clip-light reset). */
    resetClips() { masterClipped = false; clippedTracks = new Set(); },
    subscribe(): Promise<UnlistenFn> {
      return onMerulaMeters((m) => {
        master = m.master; tracks = m.tracks; voices = m.voices;
        dspLoad = m.dsp_load; gainReduction = m.gain_reduction;
        // Latch clips. Reassign the Set only when a *new* index trips, so steady
        // playback never churns reactive state frame after frame.
        if (clips(master)) masterClipped = true;
        let next: Set<number> | null = null;
        for (let i = 0; i < tracks.length; i++) {
          if (clips(tracks[i]) && !clippedTracks.has(i)) (next ??= new Set(clippedTracks)).add(i);
        }
        if (next) clippedTracks = next;
      });
    },
  };
}

// ── Diagnostics (Problems) ──────────────────────────────────────────────────────

function createDiagnosticsStore() {
  let errors = $state<MerulaDiagnostic[]>([]);

  return {
    get errors()    { return errors; },
    get hasErrors() { return errors.some((e) => e.severity === 'error'); },
    get count()     { return errors.length; },
    /** Replace the set (the `merula_eval` inline result feeds this too). */
    set(next: MerulaDiagnostic[]) { errors = next; },
    clear() { errors = []; },
    subscribe(): Promise<UnlistenFn> {
      return onMerulaDiagnostics((d) => { errors = d.errors; });
    },
  };
}

// ── Active haps (live editor highlight) ─────────────────────────────────────────

function createActiveHapsStore() {
  let haps = $state<MerulaActiveHap[]>([]);

  return {
    get haps() { return haps; },
    /** True when a source byte-range is currently sounding (editor highlight). */
    isActive(start: number, end: number): boolean {
      return haps.some((h) => h.start === start && h.end === end);
    },
    subscribe(): Promise<UnlistenFn> {
      return onMerulaActiveHaps((h) => { haps = h.haps; });
    },
  };
}

// ── Log (Console) ────────────────────────────────────────────────────────────────

/** One console line — the wire `{level,message}` plus a local monotonic id. */
export interface MerulaLogEntry { id: number; level: string; message: string; }

/** Ring-buffer cap so a long session can't grow the console unbounded. */
const LOG_CAP = 2_000;

function createLogStore() {
  let lines = $state<MerulaLogEntry[]>([]);
  let nextId = 0; // monotonic, not Date-based (deterministic, cheap)

  return {
    get lines() { return lines; },
    get count() { return lines.length; },
    clear() { lines = []; },
    subscribe(): Promise<UnlistenFn> {
      return onMerulaLog((l) => {
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
      return onMerulaAudioError((e) => { message = e.message; });
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

function createMerulaEngine() {
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
    async eval(source: string, projectDir?: string): Promise<MerulaDiagnostic[]> {
      const seq = ++evalSeq;
      const d = await merulaEval(source, projectDir);
      if (seq === evalSeq) diagnosticsStore.set(d.errors);
      return d.errors;
    },

    /** Evaluate then start playback (opens the audio device on first call).
     *  A failed eval leaves the last good arrangement staged, so play still
     *  sounds the previous result. */
    async run(source: string, projectDir?: string): Promise<void> {
      metersStore.resetClips(); // each playthrough starts with a clean clip light
      await this.eval(source, projectDir);
      await merulaPlay();
    },

    /** Stop playback (the clock keeps its position). */
    async stop(): Promise<void> { await merulaStop(); },

    /** Start playback of the already-staged arrangement from the current playhead,
     *  WITHOUT re-evaluating (used by play-from-cursor after a seek). A no-op when
     *  nothing is staged / no session is open. */
    async play(): Promise<void> { metersStore.resetClips(); await merulaPlay(); },

    /** Run if stopped, stop if running. */
    async toggleRun(source: string, projectDir?: string): Promise<void> {
      if (transportStore.playing) await this.stop();
      else await this.run(source, projectDir);
    },

    seek(cycle: number)  { return merulaSeek(cycle); },
    setCps(cps: number)  { return merulaSetCps(cps); },

    /** Play an arbitrary `.merula` chunk **one-shot** on the audition bus (right-
     *  click→Play, Outline Play, Scratch panel). It sounds once over its detected
     *  period and stops on its own; the song transport is untouched. A blank /
     *  malformed snippet is a no-op. */
    playSnippet(source: string, projectDir?: string): Promise<void> {
      if (!source.trim()) return Promise.resolve();
      return merulaPlaySnippet(source, projectDir);
    },
    /** Stop an in-flight snippet preview early (clears the audition bus only). */
    stopSnippet(): Promise<void> { return merulaStopSnippet(); },

    /** Jump the playhead back to cycle 0 (music-player skip-to-start). */
    seekToStart() { return merulaSeek(0); },
    /** Jump the playhead to the last cycle of the arrangement (skip-to-end).
     *  `totalCycles` comes from the arrangement viz (its content end). */
    seekToEnd(totalCycles: number) { return merulaSeek(Math.max(0, totalCycles)); },
  };
}

export const merulaEngine = createMerulaEngine();

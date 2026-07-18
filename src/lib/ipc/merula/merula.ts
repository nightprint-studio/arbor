/**
 * merula IPC — the **frozen BE↔FE contract**.
 *
 * Types only + thin `merula(...)` / `listen` wrappers — **no UI, no state**. This
 * is the seam the merula window builds on: every payload here mirrors a serde
 * struct in `crates/merula/be/src/` 1:1, **field-for-field in snake_case** (the
 * Rust wire shape is authoritative; do not camelCase the payloads).
 *
 * - Commands route through the generic Model-D `rpc` bridge to **`merula-be`**
 *   (the out-of-process audio backend) via the bound {@link merula} helper:
 *   `merula('<handler>', params)`. The `<handler>` is the exact merula-be handler
 *   name (= the Rust fn name, e.g. `merula_eval`, `set_merula_config`), and the
 *   `params` keys are the handler's argument names in **snake_case** — they're
 *   forwarded verbatim inside the opaque `params` object (Tauri's
 *   camelCase→snake_case conversion only applies to a command's direct args, not
 *   to a nested JSON payload). So nested option bags (`RenderOpts` / `ImportOpts`)
 *   are emitted with snake_case keys too (see {@link renderOptsWire} /
 *   {@link importOptsWire}).
 * - Events (merula-window scoped): `merula:diagnostics` / `active_haps` / `meters`
 *   / `transport` / `log` / `pack_progress` / `audio_error`. These still arrive
 *   over `listen` (emitted by the shell, scoped to the merula window). The audio
 *   thread throttles `transport`/`meters` to ~30 fps and emits `active_haps` only
 *   when the sounding set changes; `log` is gated at the source by the threshold.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { merula } from '../rpc';

// ── Event name constants (mirror `events.rs`) ─────────────────────────────────

export const MERULA_EVENTS = {
  diagnostics: 'merula:diagnostics',
  activeHaps:  'merula:active_haps',
  meters:      'merula:meters',
  transport:   'merula:transport',
  log:         'merula:log',
  packProgress:'merula:pack_progress',
  audioError:  'merula:audio_error',
} as const;

// ── Event payloads (mirror the serde structs) ─────────────────────────────────

/** Severity of a {@link MerulaDiagnostic} (today the evaluator emits only `error`). */
export type MerulaSeverity = 'error' | 'warning' | 'info';

/** One located diagnostic; `start`/`end` are byte offsets into the source. */
export interface MerulaDiagnostic {
  message:  string;
  severity: MerulaSeverity;
  start:    number | null;
  end:      number | null;
}

/** `merula:diagnostics` payload (also the `merula_eval` return). Empty = success. */
export interface MerulaDiagnostics {
  errors: MerulaDiagnostic[];
}

/** `merula:transport` payload: playhead position, tempo, run state. */
export interface MerulaTransport {
  /** Whether the scheduler is running. */
  playing: boolean;
  /** Fractional cycle position at the playhead. */
  cycle: number;
  /** Absolute output frame at the playhead. */
  frame: number;
  /** Tempo in cycles-per-second in force at the playhead. */
  cps: number;
  /** Output sample rate (frames/s) of the live session (constant per session). */
  sample_rate: number;
}

/** Linear stereo peak `[left, right]`, `0.0..~1.0`. */
export type MerulaStereoPeak = [number, number];

/** `merula:meters` payload: audio-engine telemetry sampled at the tick rate. */
export interface MerulaMeters {
  /** Master output peak (post-limiter). */
  master: MerulaStereoPeak;
  /** Per-track post-fader peak, indexed by mixer strip (arrangement order). */
  tracks: MerulaStereoPeak[];
  /** Currently sounding voice count. */
  voices: number;
  /** DSP load `0.0..~1.0` (1.0 ≈ the audio callback is using its whole budget). */
  dsp_load: number;
  /** Master limiter gain reduction `0.0..1.0` (`0` = none, larger = more ducking). */
  gain_reduction: number;
}

/** One sounding source range, for the live editor highlight. */
export interface MerulaActiveHap {
  /** Source byte-range start. */
  start: number;
  /** Source byte-range end. */
  end: number;
  /** Mixer-strip index owning it (so the highlight can be coloured per track). */
  track: number;
}

/** `merula:active_haps` payload: every source range sounding at the playhead. */
export interface MerulaActiveHaps {
  haps: MerulaActiveHap[];
}

/** `merula:log` payload: one threshold-gated log line. */
export interface MerulaLogLine {
  /** `trace` | `debug` | `info` | `warn` | `error`. */
  level: string;
  message: string;
}

/** `merula:pack_progress` payload; `pct` is `-1` when the total is unknown. */
export interface MerulaPackProgress {
  job_id: string;
  /** Which pack is installing (`vsco` | `dirt-samples` | `drum-machines` | …). */
  pack_id: string;
  /** `downloading` | `extracting`. */
  phase: string;
  done: number;
  total: number;
  pct: number;
}

/** `merula:audio_error` payload: the audio device could not be opened. */
export interface MerulaAudioError {
  message: string;
}

// ── Command request / response types ──────────────────────────────────────────

/** One downloadable sample pack with its install status (`merula_packs`). */
export interface MerulaPack {
  /** Stable id used in commands (`vsco` | `dirt-samples` | `drum-machines` | …). */
  id: string;
  /** Human label for the UI. */
  name: string;
  /** One-line description of the pack's contents. */
  description: string;
  /** Rough download size in bytes, for a pre-install estimate (`~N MB`). */
  approx_bytes: number;
  installed: boolean;
  /** Whether the installed pack is enabled for the active profile (its voices are
   *  resolvable / contribute to the live registry). Always `false` until installed. */
  active: boolean;
  path: string;
  size_bytes: number;
  sha256: string | null;
  instrument_count: number;
}

/** Offline-render defaults (`[merula].render`). */
export interface MerulaRenderConfig {
  sample_rate: number;
  /** `int24` | `float32`. */
  bit_depth: string;
  tail_max_secs: number;
  /** `wav` | `ogg` — the remembered default export format. */
  format: string;
}

/** Persisted merula settings (`[merula]` in the global config). */
export interface MerulaConfig {
  default_octave: number;
  default_cps: number;
  /** `trace` | `debug` | `info` | `warn` | `error`. */
  log_threshold: string;
  render: MerulaRenderConfig;
  vsco_dir: string | null;
  packs_dir: string | null;
  /** Chosen audio output device (cpal name), or null for the host default. */
  output_device: string | null;
  /** Transport step-back/forward distance in cycles (bars). Default 1. */
  skip_step_cycles: number;
}

/** One selectable audio output device (`merula_audio_devices`). */
export interface MerulaAudioDevice {
  name: string;
  is_default: boolean;
}

/** Options for `merula_render`. `cycles` is required (a Pattern has no length). */
export interface MerulaRenderOpts {
  cycles: number;
  /** First cycle of the bounce window (a region export). Defaults to 0 (whole
   *  arrangement) when omitted. */
  start_cycle?: number;
  /** `int24` | `float32` — overrides the config default when set. */
  bit_depth?: string;
  tail_max_secs?: number;
  sample_rate?: number;
  /** `wav` | `ogg` — output container/codec. Defaults to WAV. */
  format?: string;
  /** Target integrated loudness (LUFS) to normalize the bounce to, or omitted to
   *  leave levels untouched (the default). A per-export choice, never persisted. */
  normalize_lufs?: number;
}

/** Serialize {@link MerulaRenderOpts} to merula-be's snake_case `RenderOpts` wire
 *  shape. The bag is nested inside the opaque `params`, so it is NOT subject to
 *  Tauri's camelCase→snake_case conversion — we convert here. Omitted keys stay
 *  omitted (the BE fills its own defaults). */
function renderOptsWire(opts: MerulaRenderOpts): Record<string, unknown> {
  const wire: Record<string, unknown> = { cycles: opts.cycles };
  if (opts.start_cycle !== undefined) wire.start_cycle = opts.start_cycle;
  if (opts.bit_depth !== undefined) wire.bit_depth = opts.bit_depth;
  if (opts.tail_max_secs !== undefined) wire.tail_max_secs = opts.tail_max_secs;
  if (opts.sample_rate !== undefined) wire.sample_rate = opts.sample_rate;
  if (opts.format !== undefined) wire.format = opts.format;
  if (opts.normalize_lufs !== undefined) wire.normalize_lufs = opts.normalize_lufs;
  return wire;
}

/** Result of `merula_export_midi`: tracks + notes written to the `.mid`. */
export interface MerulaMidiExport {
  tracks: number;
  notes: number;
}

/** One clip window from `merula_analyze_levels`: a track + cycle range over 0 dBFS. */
export interface MerulaClipWindow {
  track: number;
  /** Window start in cycles (absolute timeline). */
  start: number;
  /** Window end in cycles. */
  end: number;
  /** Deepest post-fader peak in the window, linear (1.0 = 0 dBFS). */
  peak: number;
}

/** Result of `merula_analyze_levels`: per-track post-fader peak (linear) + clips. */
export interface MerulaLevelAnalysis {
  track_peaks: number[];
  clips: MerulaClipWindow[];
}

/** A `merula_transport` verb. */
export type MerulaTransportAction = 'play' | 'stop' | 'seek' | 'set_cps';

// ── Commands ──────────────────────────────────────────────────────────────────

/**
 * Evaluate `.merula` source and stage it as the live arrangement. Returns
 * diagnostics inline (also emitted as `merula:diagnostics`); a language error is
 * a diagnostic, not a rejection, so this resolves `Ok` with non-empty `errors`.
 * Does **not** open the audio device — that happens on the first `play`.
 */
export function merulaEval(source: string, projectDir?: string): Promise<MerulaDiagnostics> {
  return merula('merula_eval', { source, project_dir: projectDir ?? null });
}

/** Low-level transport command. Prefer the named helpers below. */
export function merulaTransport(action: MerulaTransportAction, value?: number): Promise<void> {
  return merula('merula_transport', { action, value: value ?? null });
}

/** Start playback (opens the audio device on first call). */
export function merulaPlay(): Promise<void> {
  return merulaTransport('play');
}

/** Stop and release all voices (the clock keeps its position). */
export function merulaStop(): Promise<void> {
  return merulaTransport('stop');
}

/** Jump the cycle clock so `cycle` aligns with the current frame. */
export function merulaSeek(cycle: number): Promise<void> {
  return merulaTransport('seek', cycle);
}

/** Change tempo (applied quantized at the next cycle boundary). */
export function merulaSetCps(cps: number): Promise<void> {
  return merulaTransport('set_cps', cps);
}

/**
 * Render `source` to a WAV at `path` over `opts.cycles` cycles, on a background
 * job. Returns the job id immediately; completion flows through the Jobs overlay.
 */
export function merulaRender(
  source: string,
  path: string,
  opts: MerulaRenderOpts,
  projectDir?: string,
): Promise<string> {
  return merula('merula_render', {
    source,
    project_dir: projectDir ?? null,
    path,
    opts: renderOptsWire(opts),
  });
}

/**
 * Render `source` to per-track **stems** (one WAV/OGG per track) in `dir`, on a
 * background job. Returns the job id immediately; progress + completion flow
 * through the Jobs / Downloads & Exports overlay (same as {@link merulaRender}).
 */
export function merulaRenderStems(
  source: string,
  dir: string,
  opts: MerulaRenderOpts,
  projectDir?: string,
): Promise<string> {
  return merula('merula_render_stems', {
    source,
    project_dir: projectDir ?? null,
    dir,
    opts: renderOptsWire(opts),
  });
}

/**
 * Export `source` to a Standard MIDI File at `path`, baking the arrangement's
 * natural loop period (one pass of the song). Note-only (no audio), so it
 * resolves with the written {@link MerulaMidiExport} summary directly — no job.
 */
export function merulaExportMidi(
  source: string,
  path: string,
  projectDir?: string,
): Promise<MerulaMidiExport> {
  return merula('merula_export_midi', { source, project_dir: projectDir ?? null, path });
}

/**
 * Analyze `source` for clipping **without playing it** — a silent offline render
 * over the loop period that reports per-track post-fader peaks + the cycle windows
 * over 0 dBFS. A bad snippet resolves to an empty result.
 */
export function merulaAnalyzeLevels(
  source: string,
  projectDir?: string,
): Promise<MerulaLevelAnalysis> {
  return merula('merula_analyze_levels', { source, project_dir: projectDir ?? null });
}

/** List every downloadable sample pack with its install status. */
export function merulaPacks(): Promise<MerulaPack[]> {
  return merula('merula_packs');
}

/** Start downloading + installing a sample pack by id (job-tracked). Returns job id. */
export function merulaPackDownload(packId: string): Promise<string> {
  return merula('merula_pack_download', { pack_id: packId });
}

/** Enable / disable an installed pack for the active profile. Persisted; takes
 *  effect on the next eval / run. Re-read packs + sounds afterwards. */
export function merulaPackSetActive(packId: string, active: boolean): Promise<void> {
  return merula('merula_pack_set_active', { pack_id: packId, active });
}

/**
 * Re-index an installed pack: rebuild its registry from the extracted files on
 * disk (no re-download), refreshing the instruments it exposes. Returns the
 * updated pack status. Re-read packs + sounds afterwards.
 */
export function merulaPackReindex(packId: string): Promise<MerulaPack> {
  return merula('merula_pack_reindex', { pack_id: packId });
}

/** Delete an installed sample pack from disk. Re-read packs + sounds afterwards. */
export function merulaPackDelete(packId: string): Promise<void> {
  return merula('merula_pack_delete', { pack_id: packId });
}

/** Read the merula config (merula's own `config.toml`). */
export function getMerulaConfig(): Promise<MerulaConfig> {
  return merula('get_merula_config');
}

/** Persist a new merula config. Takes effect for the next session / render. */
export function setMerulaConfig(config: MerulaConfig): Promise<void> {
  return merula('set_merula_config', { config });
}

/** List the host's audio output devices (name + whether it's the system default). */
export function merulaAudioDevices(): Promise<MerulaAudioDevice[]> {
  return merula('merula_audio_devices');
}

/** Choose the audio output device (cpal name, or null for the host default).
 *  Persists the choice and switches a live session to it immediately. */
export function merulaSetOutputDevice(device: string | null): Promise<void> {
  return merula('merula_set_output_device', { device });
}

// ── Event subscriptions ───────────────────────────────────────────────────────
//
// Each returns the Tauri `UnlistenFn` to detach. Scope listeners to the merula
// window's lifetime (subscribe on mount, call the returned fn on teardown).

/** Subscribe to evaluation diagnostics. */
export function onMerulaDiagnostics(cb: (d: MerulaDiagnostics) => void): Promise<UnlistenFn> {
  return listen<MerulaDiagnostics>(MERULA_EVENTS.diagnostics, (e) => cb(e.payload));
}

/** Subscribe to the active-hap highlight set (emitted on change). */
export function onMerulaActiveHaps(cb: (h: MerulaActiveHaps) => void): Promise<UnlistenFn> {
  return listen<MerulaActiveHaps>(MERULA_EVENTS.activeHaps, (e) => cb(e.payload));
}

/** Subscribe to audio meters / telemetry (~30 fps). */
export function onMerulaMeters(cb: (m: MerulaMeters) => void): Promise<UnlistenFn> {
  return listen<MerulaMeters>(MERULA_EVENTS.meters, (e) => cb(e.payload));
}

/** Subscribe to transport position / tempo (~30 fps). */
export function onMerulaTransport(cb: (t: MerulaTransport) => void): Promise<UnlistenFn> {
  return listen<MerulaTransport>(MERULA_EVENTS.transport, (e) => cb(e.payload));
}

/** Subscribe to script log lines (already threshold-gated at the source). */
export function onMerulaLog(cb: (l: MerulaLogLine) => void): Promise<UnlistenFn> {
  return listen<MerulaLogLine>(MERULA_EVENTS.log, (e) => cb(e.payload));
}

/** Subscribe to sample-pack install progress (carries `pack_id`). */
export function onMerulaPackProgress(cb: (p: MerulaPackProgress) => void): Promise<UnlistenFn> {
  return listen<MerulaPackProgress>(MERULA_EVENTS.packProgress, (e) => cb(e.payload));
}

/** Subscribe to a fatal audio-device error (the session thread exited). */
export function onMerulaAudioError(cb: (e: MerulaAudioError) => void): Promise<UnlistenFn> {
  return listen<MerulaAudioError>(MERULA_EVENTS.audioError, (ev) => cb(ev.payload));
}

// ════════════════════════════════════════════════════════════════════════════
// Additive surface (Fase 4 · Step 1) — extends the frozen contract WITHOUT
// breaking it: new commands only, same snake_case discipline. These feed the
// Step 2/3 fan-outs (arrangement viz, sound bank, mixer) + the project model.
// ════════════════════════════════════════════════════════════════════════════

// ── merula_query: the whole arrangement timeline (off-thread Pattern query) ─────
//
// `active_haps` only reports what sounds *now*; the arrangement view needs the
// full timeline. `merula_query` queries the last-evaluated `Tracks` over
// `[0, cycles)` off the audio thread and returns every hap. Empty when nothing
// has been evaluated yet.

/** One hap of the queried arrangement. `start`/`end` are in cycles (absolute
 *  timeline); `has_onset` is false for continuous signals (no `whole`). */
export interface MerulaQueryHap {
  /** Mixer-strip / arrangement-lane index (0-based). */
  track: number;
  /** Onset in cycles (the hap's `whole.begin`, or `part.begin` if continuous). */
  start: number;
  /** End in cycles (`whole.end`, or `part.end` if continuous). */
  end: number;
  /** True for a discrete event (has a `whole`); false for a continuous signal. */
  has_onset: boolean;
  /** Source byte-range start (for editor mapping), or null. */
  span_start: number | null;
  /** Source byte-range end, or null. */
  span_end: number | null;
  /** Sound name (`bd`, …) if any. */
  sound: string | null;
  /** MIDI pitch (C4 = 60) if any. */
  note: number | null;
  /** Per-hap gain if any. */
  gain: number | null;
}

/** One named arrangement section, tiled to an absolute cycle range within the
 *  queried window (the arrangement loops, so a section repeats every period). */
export interface MerulaQuerySection {
  /** Owning mixer-strip / arrangement-lane index (0-based). */
  track: number;
  /** Section label (`section("INTRO", …)`). */
  name: string;
  /** Start cycle (absolute, inclusive). */
  start: number;
  /** End cycle (absolute, exclusive). */
  end: number;
}

/** `merula_query` result: every hap + every named section over the window. */
export interface MerulaQueryHaps {
  haps: MerulaQueryHap[];
  /** Named section bands (empty unless a track uses `arrange(section(...))`). */
  sections: MerulaQuerySection[];
  /** Period (in cycles) after which the whole arrangement repeats — the natural
   *  render length. `0` only when there are no haps at all. */
  loop_cycles: number;
  /** Effective render tempo (cycles/s): the arrangement's starting `tempo(...)`
   *  point, else its `cps(...)`. `null` when the script set neither (fall back to
   *  the configured default). Mirrors how `merula_render` picks the bounce tempo. */
  cps: number | null;
}

/** Query the last-evaluated arrangement over `[0, cycles)`. Empty until an eval
 *  has succeeded. Off the audio thread — safe to call while playing. */
export function merulaQuery(cycles: number): Promise<MerulaQueryHaps> {
  return merula('merula_query', { cycles });
}

// ── merula_scenes: clip-launcher scene metadata ─────────────────────────────────

/** One clip in a scene: the base track it targets (by name) and the resolved
 *  column index — `null` when no base track carries that name (an inert clip). */
export interface MerulaSceneClip {
  track: string;
  track_index: number | null;
}

/** One launchable scene: its label (launcher row) and the clips it fires. */
export interface MerulaScene {
  name: string;
  clips: MerulaSceneClip[];
}

/** `merula_scenes` result: the base track names (launcher columns, mixer order)
 *  and the declared `scene(...)` rows. Empty until an arrangement with a
 *  `scene(...)` has been evaluated. */
export interface MerulaScenes {
  tracks: string[];
  scenes: MerulaScene[];
}

/** Read the launchable scenes of the last-evaluated arrangement (the clip-launcher
 *  grid). Off the audio thread — safe to call while playing. */
export function merulaScenes(): Promise<MerulaScenes> {
  return merula('merula_scenes');
}

/** One entry of a launch selection: base track `track` plays the clip that scene
 *  `scene` declares for it. Tracks absent from the selection keep their base. */
export interface MerulaClipSelection {
  track: number;
  scene: string;
}

/** Fire the launcher's current selection: re-stage the last-evaluated tracks with
 *  the chosen scenes' clips substituted into their same-named base tracks,
 *  quantized to the next cycle boundary. An empty selection restores every track
 *  to its base (stop all). No-op until an eval has succeeded. */
export function merulaLaunch(selection: MerulaClipSelection[]): Promise<void> {
  return merula('merula_launch', { selection });
}

// ── merula_sounds: the resolvable instrument list (registry introspection) ──────

export type MerulaInstrumentKind = 'synth' | 'sample' | 'sfz';

/** One resolvable voice in the sound registry. */
export interface MerulaInstrument {
  /** Dotted registry name (`strings.violin`) or a short bank name (`bd`). */
  name: string;
  kind: MerulaInstrumentKind;
  /** Named articulations the instrument exposes (`.art("…")`), sorted; empty for
   *  synth / sample voices. */
  articulations: string[];
  /** A short one-line description for the sound bank, or null when the catalogue
   *  has no entry for this voice. */
  description: string | null;
  /** Stable id of the sample pack this voice comes from (`dirt-samples`, …), for
   *  per-pack grouping; null for built-in synths. */
  pack: string | null;
  /** Human label of that pack (`Dirt-Samples`, …); null for built-in synths. */
  pack_name: string | null;
}

/** `merula_sounds` result. Always includes the built-in default synth. */
export interface MerulaSoundList {
  instruments: MerulaInstrument[];
}

/** List the instruments the engine can currently resolve (built-in synths + any
 *  installed sample pack). Reflects the real registry, not a static list, so it
 *  tracks what's actually installed. */
export function merulaSounds(): Promise<MerulaSoundList> {
  return merula('merula_sounds');
}

// ── merula_set_track: live mixer overrides (ephemeral; eval re-baselines) ───────
//
// The source stays authoritative: on every eval the arrangement re-establishes
// the baseline. These overrides are live session tweaks on top, applied in
// real-time (smooth knob drag), released at the next eval. Surgical "commit
// knob → source literal" is the future `merula_set_literal`.

/** A live mixer override target. `master_gain` / `reverb` / `metronome` / `count_in`
 *  ignore `track`. */
export type MerulaTrackAction =
  | 'gain'
  | 'pan'
  | 'mute'
  | 'solo'
  | 'master_gain'
  | 'reverb'
  | 'metronome'
  | 'count_in';

/** Push a live mixer override to the running session (no-op when stopped). `value`
 *  is 0..1 for gain/pan/master_gain, 0|1 (off|on) for mute/solo, and decay seconds
 *  for `reverb`. */
export function merulaSetTrack(action: MerulaTrackAction, track: number | null, value: number): Promise<void> {
  return merula('merula_set_track', { action, track: track ?? null, value });
}

/** Set the shared reverb-return decay (procedural IR length, in seconds). A global
 *  mix control like the master gain — session-only, persists across evals. */
export function merulaSetReverb(seconds: number): Promise<void> {
  return merulaSetTrack('reverb', null, seconds);
}

/** Enable / disable the audible metronome click track (a monitoring aid; clicks
 *  bypass the song mixer). Session-only, persists across evals. */
export function merulaSetMetronome(on: boolean): Promise<void> {
  return merulaSetTrack('metronome', null, on ? 1 : 0);
}

/** Set the count-in length in whole bars (`0` = off). On the next play the song is
 *  delayed by this many bars while the metronome clicks the pre-roll. Session-only,
 *  persists across evals. */
export function merulaSetCountIn(bars: number): Promise<void> {
  return merulaSetTrack('count_in', null, Math.max(0, Math.round(bars)));
}

// ── merula_audition_expr: one-off instrument preview from a generated snippet ────

/** Play a one-off instrument preview from a `.merula` snippet. The caller composes
 *  a tiny expression — a note (or chord / scale degree) plus the panel's knob /
 *  chain values, e.g. `n(c4).inst("synth.bass").gain(0.8).room(0.2)` — which the
 *  backend evaluates with the real language and plays one cycle of on a dedicated
 *  audition bus (bypasses the song mixer, so it's heard cleanly whether or not a
 *  song is playing). Opens the audio device on demand; a malformed snippet simply
 *  doesn't sound. The whole language drives the preview — no per-param plumbing. */
export function merulaAuditionExpr(expr: string, projectDir?: string): Promise<void> {
  return merula('merula_audition_expr', { expr, project_dir: projectDir ?? null });
}

// ── Snippet evaluator / mini audio tester ──────────────────────────────────────
//
// Evaluate or play an arbitrary `.merula` chunk (a selection, an outline
// declaration, or pasted scratch text) in isolation — without touching the live
// arrangement. Powers the Scratch panel, right-click→Play, and Outline Play.

/** `merula_eval_snippet` result: an arbitrary chunk evaluated in isolation. Mirrors
 *  {@link MerulaQueryHaps} (events + detected loop period + tempo) plus inline
 *  `diagnostics`. Hap spans are byte offsets relative to the **snippet** text
 *  (offset 0 = start of the snippet), so the caller maps them back by adding the
 *  snippet's origin offset in the document. */
export interface MerulaSnippetEval {
  /** Parse / eval / validation errors (empty on success). Inline only — never
   *  emitted on the `merula:diagnostics` channel (that belongs to the main editor). */
  diagnostics: MerulaDiagnostic[];
  haps: MerulaQueryHap[];
  sections: MerulaQuerySection[];
  /** Detected loop period (cycles), the natural one-shot length. `0` when empty. */
  loop_cycles: number;
  /** Effective render tempo (starting `tempo(...)` point, else `cps(...)`), or null. */
  cps: number | null;
}

/** Evaluate an arbitrary `.merula` chunk in isolation and return the events it
 *  generates (plus its detected loop period + tempo). Does not touch the live
 *  arrangement or the audio device; errors come back inline in the result. The
 *  snippet must be a self-contained program (a `tracks(...)` / pattern expression). */
export function merulaEvalSnippet(source: string, projectDir?: string): Promise<MerulaSnippetEval> {
  return merula('merula_eval_snippet', { source, project_dir: projectDir ?? null });
}

/** Play an arbitrary `.merula` chunk **one-shot** on the audition bus: it sounds
 *  once over its detected loop period and stops on its own, without disturbing the
 *  song transport. Opens the audio device on demand; a malformed snippet simply
 *  doesn't sound (use {@link merulaEvalSnippet} to surface errors). */
export function merulaPlaySnippet(source: string, projectDir?: string): Promise<void> {
  return merula('merula_play_snippet', { source, project_dir: projectDir ?? null });
}

/** **Freeze** a pattern: evaluate a self-contained snippet (the caller prepends the
 *  file's constants/imports) and return its first track materialized over one cycle
 *  to canonical literal source (`n(c4 e4 g4)` / `s(bd sn)`). Empty string when the
 *  snippet doesn't evaluate or yields no onsets. */
export function merulaMaterialize(source: string, projectDir?: string): Promise<string> {
  return merula('merula_materialize', { source, project_dir: projectDir ?? null });
}

/** Stop an in-flight snippet preview early (clears the audition bus only; a playing
 *  song is untouched). No-op when nothing is running. */
export function merulaStopSnippet(): Promise<void> {
  return merula('merula_stop_snippet');
}

// ── Project model: open / create a merula project (folder + merula.toml) ─────────

/** One `.merula` file in a project (source read lazily on the FE via `fs_*`). */
export interface MerulaProjectFile {
  /** Absolute path. */
  path: string;
  /** Project-relative path (forward slashes), e.g. `lib/drums.merula`. */
  rel: string;
  /** File name with extension. */
  name: string;
  /** Listed under `libraries` in merula.toml: imported-only, its `tracks(…)` ignored. */
  library: boolean;
}

/** A merula project manifest (`merula.toml`) + its `.merula` files. */
export interface MerulaProjectInfo {
  /** Absolute project folder. */
  path: string;
  /** `name` from merula.toml (falls back to the folder name). */
  name: string;
  /** `audience` ("for whom") from merula.toml. */
  audience: string;
  files: MerulaProjectFile[];
}

/** Open a merula project folder: parse `merula.toml`, list its `.merula` files. */
export function merulaOpenProject(dir: string): Promise<MerulaProjectInfo> {
  return merula('merula_open_project', { dir });
}

/** Scaffold a new merula project at `dir` (writes `merula.toml` + a starter
 *  `.merula`), returning the opened manifest. */
export function merulaCreateProject(dir: string, name: string, audience: string): Promise<MerulaProjectInfo> {
  return merula('merula_create_project', { dir, name, audience });
}

/** Rename a project — set the root `name` in `merula.toml` (preserves the rest of
 *  the manifest), returning the re-opened project. */
export function merulaSetProjectName(dir: string, name: string): Promise<MerulaProjectInfo> {
  return merula('merula_set_project_name', { dir, name });
}

// ── Persisted merula window state (recents + last project + layout) ─────────────
//
// A dedicated merula state file (NOT localStorage, NOT the per-project merula.toml,
// NOT the typed [merula] settings): recents/last-project are global app state,
// the layout is the window's panel arrangement.

/** Persisted panel layout of the merula window. */
export interface MerulaLayoutState {
  /** `files` | `outline` | `soundbank` | null. */
  left_panel: string | null;
  /** `console` | `problems` | `mixer` | null. */
  bottom_panel: string | null;
  /** `inspector` | `docs` | null. */
  right_panel: string | null;
  /** Arrangement (viz) pane hidden. */
  collapse_viz: boolean;
  /** Editor pane hidden. */
  collapse_editor: boolean;
}

/** One named project workspace — a group of `.merula` projects with a colour. */
export interface MerulaProjectWorkspace {
  /** Stable id (generated on the FE). */
  id: string;
  /** Display name. */
  name: string;
  /** Index into the workspace colour palette. */
  color_idx: number;
  /** Member project folders (absolute paths). */
  project_paths: string[];
}

/** The dedicated merula window state file. */
export interface MerulaWorkspaceState {
  /** Recently-opened project folders, most-recent first. */
  recent_projects: string[];
  /** Project folder to reopen on launch, or null. */
  last_project: string | null;
  layout: MerulaLayoutState;
  /** Sound-bank favourites (instrument names). */
  favorite_sounds: string[];
  /** Recently-used instrument names, most-recent first. */
  recent_sounds: string[];
  /** Named project workspaces (groups of `.merula` projects). */
  workspaces: MerulaProjectWorkspace[];
  /** The active workspace id, or null. */
  active_workspace: string | null;
}

/** Read the persisted merula window state (recents + last project + layout). */
export function getMerulaState(): Promise<MerulaWorkspaceState> {
  return merula('get_merula_state');
}

/** Persist the merula window state. */
export function setMerulaState(state: MerulaWorkspaceState): Promise<void> {
  return merula('set_merula_state', { state });
}

/** A project's open editor tabs, restored when it's reopened (lives in
 *  `<project>/.merula/tabs.json`). */
export interface MerulaProjectTabs {
  open_file_paths: string[];
  active_file_path: string | null;
}

/** Read a project's open-tab snapshot (empty on first open). */
export function getMerulaProjectTabs(projectPath: string): Promise<MerulaProjectTabs> {
  return merula('get_merula_project_tabs', { project_path: projectPath });
}

/** Persist a project's open-tab snapshot under its own `.merula/` folder. */
export function setMerulaProjectTabs(projectPath: string, tabs: MerulaProjectTabs): Promise<void> {
  return merula('set_merula_project_tabs', { project_path: projectPath, tabs });
}

/** A project's persisted master-bus mix (no `.merula` source representation). */
export interface MerulaProjectMix {
  /** Master output gain (0..1, linear). */
  master_gain: number;
  /** Shared reverb-return decay in seconds. */
  reverb_decay: number;
}

/** Read a project's master mix (defaults to unity / 0.5s on first open). */
export function getMerulaProjectMix(projectPath: string): Promise<MerulaProjectMix> {
  return merula('get_merula_project_mix', { project_path: projectPath });
}

/** Persist a project's master mix under its own `.merula/` folder. */
export function setMerulaProjectMix(projectPath: string, mix: MerulaProjectMix): Promise<void> {
  return merula('set_merula_project_mix', { project_path: projectPath, mix });
}

// ── Global sound aliases (`alias → target` name map) ───────────────────────────
//
// A user-defined map resolved by the audio registry so `s("kick")` plays the
// target voice. Global (not per-project / per-file), persisted in the merula data
// dir; the engine re-reads it when building a session registry.

/** Read the global sound-alias map (`alias → target`). */
export function getMerulaAliases(): Promise<Record<string, string>> {
  return merula('get_merula_aliases');
}

/** Persist the global sound-alias map. Takes effect on the next eval / run. */
export function setMerulaAliases(aliases: Record<string, string>): Promise<void> {
  return merula('set_merula_aliases', { aliases });
}

/** One persisted scratch tab (the transient eval result is not saved). */
export interface MerulaScratchTab {
  id: string;
  name: string;
  source: string;
}

/** The persisted scratch workspace (global, in the merula data dir). */
export interface MerulaScratchTabs {
  tabs: MerulaScratchTab[];
  active_id: string | null;
}

/** Read the persisted scratch tabs (empty on first run). */
export function getMerulaScratchTabs(): Promise<MerulaScratchTabs> {
  return merula('get_merula_scratch_tabs');
}

/** Persist the scratch tabs. */
export function setMerulaScratchTabs(tabs: MerulaScratchTabs): Promise<void> {
  return merula('set_merula_scratch_tabs', { tabs });
}

// ── merula_lang_reference: the canonical DSL catalogue (autocomplete + hover) ───
//
// The `.merula` language reference is authored once in Rust (`merula-lang`'s
// `reference()`); the FE loads it once and drives autocomplete, hover docs, and
// the Docs panel off it — so the editor's language intelligence and the
// evaluator can never drift. Mirrors the serde structs in
// `crates/merula/merula-lang/src/reference.rs` field-for-field.

/** Category of a {@link MerulaDslEntry} (matches the serde `snake_case` tag). */
export type MerulaDslKind =
  | 'combinator' | 'generator' | 'signal' | 'signal_method' | 'transform'
  | 'seq_method' | 'island' | 'keyword' | 'log' | 'mini' | 'note';

/** One parameter of a DSL entry. */
export interface MerulaDslParam {
  /** Parameter name as written in the signature. */
  name: string;
  /** Whether the parameter may be omitted. */
  optional: boolean;
  /** Type + range + meaning. */
  summary: string;
  /** Default value when omitted (only present if `optional`). */
  default?: string;
}

/** One catalogue entry: a named piece of the language with its docs. */
export interface MerulaDslEntry {
  /** The bare name as typed (`gain`, `par`, `sine`, `~`). */
  name: string;
  kind: MerulaDslKind;
  /** One-line signature, e.g. `gain(x, pat) -> pat`. */
  signature: string;
  /** 1–2 sentence description. */
  summary: string;
  /** Its parameters in order (empty for nullary forms / operators). */
  params: MerulaDslParam[];
  /** A short, realistic usage snippet. */
  example: string;
  /** What the call returns, when not obvious from the signature. */
  returns?: string;
}

/** Read the full `.merula` DSL reference catalogue (static; load once). */
export function merulaLangReference(): Promise<MerulaDslEntry[]> {
  return merula('merula_lang_reference');
}

/** Reformat `.merula` source to canonical style (the AST pretty-printer). Rejects
 *  with the language error when the source has a syntax error — the caller then
 *  leaves the buffer untouched. The round-trip is semantic, not byte-exact:
 *  comments and incidental whitespace are not preserved. */
export function merulaFormat(source: string): Promise<string> {
  return merula('merula_format', { source });
}

/** One scale mode in the catalogue: canonical name + aliases + ascending semitone
 *  intervals (one octave from the root). */
export interface MerulaScaleMode {
  name: string;
  aliases: string[];
  intervals: number[];
}

/** Read the scale-mode catalogue (`.scale("root:mode")` modes); load once. */
export function merulaScales(): Promise<MerulaScaleMode[]> {
  return merula('merula_scales');
}

// ── External libraries (`[libraries]` in merula.toml → `$lib/…` imports) ────────

/** One declared library's state: its source spec, the pinned commit SHA (when
 *  locked), and whether its cache is present (synced). */
export interface MerulaLibraryStatus {
  name: string;
  source: string;
  sha: string | null;
  synced: boolean;
}

/** The project's declared libraries with their lock / sync state. */
export function merulaLibraries(projectDir: string): Promise<MerulaLibraryStatus[]> {
  return merula('merula_libraries', { project_dir: projectDir });
}

/** Start a background sync of the project's libraries (resolve refs → SHAs,
 *  download missing commits, rewrite `merula.lock`). Returns the job id. */
export function merulaSyncLibraries(projectDir: string): Promise<string> {
  return merula('merula_sync_libraries', { project_dir: projectDir });
}

// ── Audio / MIDI import (WAV → MIDI, MIDI → .merula) ───────────────────────────

/** Options for the import commands (all optional; the backend fills defaults). */
export interface MerulaImportOpts {
  /** Separate stems before pitch detection (ML backends only). */
  splitStems?: boolean;
  /** Tempo (BPM) stamped into the generated MIDI. */
  tempoBpm?: number;
  /** Detect a pitched part. */
  detectPitch?: boolean;
  /** Detect a drum part. */
  detectDrums?: boolean;
  /** Quantisation grid (subdivisions per cycle); `0` keeps raw timing. */
  grid?: number;
  /** Beats per cycle (bar length). */
  beatsPerCycle?: number;
}

/** Serialize {@link MerulaImportOpts} to merula-be's snake_case `ImportOpts` wire
 *  shape. Nested inside the opaque `params`, so it bypasses Tauri's
 *  camelCase→snake_case conversion — translate the keys here. `null` (no opts) is
 *  forwarded as-is so the BE applies its own defaults. */
function importOptsWire(opts?: MerulaImportOpts): Record<string, unknown> | null {
  if (!opts) return null;
  const wire: Record<string, unknown> = {};
  if (opts.splitStems !== undefined) wire.split_stems = opts.splitStems;
  if (opts.tempoBpm !== undefined) wire.tempo_bpm = opts.tempoBpm;
  if (opts.detectPitch !== undefined) wire.detect_pitch = opts.detectPitch;
  if (opts.detectDrums !== undefined) wire.detect_drums = opts.detectDrums;
  if (opts.grid !== undefined) wire.grid = opts.grid;
  if (opts.beatsPerCycle !== undefined) wire.beats_per_cycle = opts.beatsPerCycle;
  return wire;
}

/**
 * D4 — transcribe a WAV and write a `.mid` to `output`. Returns the job id;
 * progress/completion arrive on `arbor://job-progress` / `job-done`.
 */
export function merulaConvertWavToMidi(
  input: string,
  output: string,
  opId?: string,
  opts?: MerulaImportOpts,
): Promise<string> {
  return merula('merula_convert_wav_to_midi', {
    input,
    output,
    op_id: opId ?? null,
    opts: importOptsWire(opts),
  });
}

/**
 * D3 — transcribe a WAV and return idiomatic `.merula` text (the MIDI never
 * touches disk). `opId` correlates the backend progress/done events with a
 * client-side transfer so the UI can show a live bar; open the result in a tab.
 */
export function merulaImportAudioAsMerula(
  input: string,
  opId?: string,
  opts?: MerulaImportOpts,
): Promise<string> {
  return merula('merula_import_audio_as_merula', {
    input,
    op_id: opId ?? null,
    opts: importOptsWire(opts),
  });
}

/** D5 — convert an existing `.mid` to idiomatic `.merula` text (no transcription). */
export function merulaImportMidiAsMerula(input: string, opts?: MerulaImportOpts): Promise<string> {
  return merula('merula_import_midi_as_merula', { input, opts: importOptsWire(opts) });
}

// ── ONNX transcription models (downloaded on-demand) ──────────────────────────

/** State of one downloadable transcription model (mirrors `merula::models`). */
export interface MerulaModelStatus {
  id: string;
  name: string;
  description: string;
  approx_bytes: number;
  installed: boolean;
  path: string;
  size_bytes: number;
}

/** List every transcription model with its install state. */
export function merulaModels(): Promise<MerulaModelStatus[]> {
  return merula('merula_models');
}

/** Start a background download of model `id` (returns the job id; progress on
 *  `arbor://job-progress` / `job-done`). */
export function merulaDownloadModel(id: string): Promise<string> {
  return merula('merula_download_model', { id });
}

/** Delete a downloaded model. */
export function merulaDeleteModel(id: string): Promise<void> {
  return merula('merula_delete_model', { id });
}

// ── Export all ────────────────────────────────────────────────────────────────

/** One `.merula` in the export plan, as `merula_export_plan` resolves it. */
export interface MerulaExportPlanEntry {
  /** Absolute path to the source file. */
  path: string;
  /** Project-relative path, for display. */
  rel: string;
  /** Output file name without extension. */
  stem: string;
  /** `meta.title`, when the file declares one. */
  title: string | null;
  /** Cycles to render. */
  cycles: number;
  /** Which rule produced `cycles`: `meta` | `arrangement` | `default`. */
  cycles_from: string;
  /** Parse/eval error — the file can't be rendered as it stands. */
  error: string | null;
}

/** One file to actually render (the plan entry, possibly with `cycles` overridden). */
export interface MerulaExportEntry {
  path: string;
  stem: string;
  cycles: number;
}

/** Format + quality for the whole batch — one format for every file. */
export interface MerulaExportAllOpts {
  /** `wav` | `ogg`. */
  format: string;
  sample_rate?: number;
  bit_depth?: string;
  tail_max_secs?: number;
  normalize_lufs?: number;
}

/**
 * List a project's `.merula` files with the render length each one declares, so
 * an export dialog can show a checklist without asking for a cycle count per file.
 * Files that fail to parse/evaluate come back with `error` set.
 */
export function merulaExportPlan(dir: string): Promise<MerulaExportPlanEntry[]> {
  return merula('merula_export_plan', { dir });
}

/**
 * Render every chosen file into `outDir`, all to the same format, as ONE background
 * job. Returns the job id immediately; progress spans the whole batch and flows
 * through the Downloads & Exports overlay.
 */
export function merulaExportAll(
  dir: string,
  entries: MerulaExportEntry[],
  outDir: string,
  opts: MerulaExportAllOpts,
): Promise<string> {
  const wire: Record<string, unknown> = { format: opts.format };
  if (opts.sample_rate !== undefined) wire.sample_rate = opts.sample_rate;
  if (opts.bit_depth !== undefined) wire.bit_depth = opts.bit_depth;
  if (opts.tail_max_secs !== undefined) wire.tail_max_secs = opts.tail_max_secs;
  if (opts.normalize_lufs !== undefined) wire.normalize_lufs = opts.normalize_lufs;
  return merula('merula_export_all', { dir, entries, out_dir: outDir, opts: wire });
}

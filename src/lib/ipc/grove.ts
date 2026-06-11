/**
 * grove IPC — the **frozen BE↔FE contract** (Onda 4).
 *
 * Types only + thin `invoke` / `listen` wrappers — **no UI, no state**. This is
 * the seam the grove window (Fase 4) builds on: every payload here mirrors a
 * serde struct in `src-tauri/src/grove/` 1:1, **field-for-field in snake_case**
 * (the Rust wire shape is authoritative; do not camelCase the payloads).
 *
 * - Commands: `grove_eval` / `grove_transport` / `grove_render` / VSCO status +
 *   download / get + set config. Command argument keys are snake_case to match
 *   the Rust parameter names exactly.
 * - Events (grove-window scoped): `grove:diagnostics` / `active_haps` / `meters`
 *   / `transport` / `log` / `vsco_progress` / `audio_error`. The audio thread
 *   throttles `transport`/`meters` to ~30 fps and emits `active_haps` only when
 *   the sounding set changes; `log` is gated at the source by the threshold.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ── Event name constants (mirror `events.rs`) ─────────────────────────────────

export const GROVE_EVENTS = {
  diagnostics: 'grove:diagnostics',
  activeHaps:  'grove:active_haps',
  meters:      'grove:meters',
  transport:   'grove:transport',
  log:         'grove:log',
  vscoProgress:'grove:vsco_progress',
  audioError:  'grove:audio_error',
} as const;

// ── Event payloads (mirror the serde structs) ─────────────────────────────────

/** Severity of a {@link GroveDiagnostic} (today the evaluator emits only `error`). */
export type GroveSeverity = 'error' | 'warning' | 'info';

/** One located diagnostic; `start`/`end` are byte offsets into the source. */
export interface GroveDiagnostic {
  message:  string;
  severity: GroveSeverity;
  start:    number | null;
  end:      number | null;
}

/** `grove:diagnostics` payload (also the `grove_eval` return). Empty = success. */
export interface GroveDiagnostics {
  errors: GroveDiagnostic[];
}

/** `grove:transport` payload: playhead position, tempo, run state. */
export interface GroveTransport {
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
export type GroveStereoPeak = [number, number];

/** `grove:meters` payload: audio-engine telemetry sampled at the tick rate. */
export interface GroveMeters {
  /** Master output peak (post-limiter). */
  master: GroveStereoPeak;
  /** Per-track post-fader peak, indexed by mixer strip (arrangement order). */
  tracks: GroveStereoPeak[];
  /** Currently sounding voice count. */
  voices: number;
  /** DSP load `0.0..~1.0` (1.0 ≈ the audio callback is using its whole budget). */
  dsp_load: number;
}

/** One sounding source range, for the live editor highlight. */
export interface GroveActiveHap {
  /** Source byte-range start. */
  start: number;
  /** Source byte-range end. */
  end: number;
  /** Mixer-strip index owning it (so the highlight can be coloured per track). */
  track: number;
}

/** `grove:active_haps` payload: every source range sounding at the playhead. */
export interface GroveActiveHaps {
  haps: GroveActiveHap[];
}

/** `grove:log` payload: one threshold-gated log line. */
export interface GroveLogLine {
  /** `trace` | `debug` | `info` | `warn` | `error`. */
  level: string;
  message: string;
}

/** `grove:vsco_progress` payload; `pct` is `-1` when the total is unknown. */
export interface GroveVscoProgress {
  job_id: string;
  /** `downloading` | `extracting`. */
  phase: string;
  done: number;
  total: number;
  pct: number;
}

/** `grove:audio_error` payload: the audio device could not be opened. */
export interface GroveAudioError {
  message: string;
}

// ── Command request / response types ──────────────────────────────────────────

/** VSCO 2 sample-bank install status (`grove_vsco_status`). */
export interface GroveVscoStatus {
  installed: boolean;
  path: string;
  size_bytes: number;
  sha256: string | null;
  instrument_count: number;
}

/** Offline-render defaults (`[grove].render`). */
export interface GroveRenderConfig {
  sample_rate: number;
  /** `int24` | `float32`. */
  bit_depth: string;
  tail_max_secs: number;
}

/** Persisted grove settings (`[grove]` in the global config). */
export interface GroveConfig {
  default_octave: number;
  default_cps: number;
  /** `trace` | `debug` | `info` | `warn` | `error`. */
  log_threshold: string;
  render: GroveRenderConfig;
  vsco_dir: string | null;
}

/** Options for `grove_render`. `cycles` is required (a Pattern has no length). */
export interface GroveRenderOpts {
  cycles: number;
  /** `int24` | `float32` — overrides the config default when set. */
  bit_depth?: string;
  tail_max_secs?: number;
  sample_rate?: number;
}

/** A `grove_transport` verb. */
export type GroveTransportAction = 'play' | 'stop' | 'seek' | 'set_cps';

// ── Commands ──────────────────────────────────────────────────────────────────

/**
 * Evaluate `.grove` source and stage it as the live arrangement. Returns
 * diagnostics inline (also emitted as `grove:diagnostics`); a language error is
 * a diagnostic, not a rejection, so this resolves `Ok` with non-empty `errors`.
 * Does **not** open the audio device — that happens on the first `play`.
 */
export function groveEval(source: string, projectDir?: string): Promise<GroveDiagnostics> {
  return invoke('grove_eval', { source, project_dir: projectDir ?? null });
}

/** Low-level transport command. Prefer the named helpers below. */
export function groveTransport(action: GroveTransportAction, value?: number): Promise<void> {
  return invoke('grove_transport', { action, value: value ?? null });
}

/** Start playback (opens the audio device on first call). */
export function grovePlay(): Promise<void> {
  return groveTransport('play');
}

/** Stop and release all voices (the clock keeps its position). */
export function groveStop(): Promise<void> {
  return groveTransport('stop');
}

/** Jump the cycle clock so `cycle` aligns with the current frame. */
export function groveSeek(cycle: number): Promise<void> {
  return groveTransport('seek', cycle);
}

/** Change tempo (applied quantized at the next cycle boundary). */
export function groveSetCps(cps: number): Promise<void> {
  return groveTransport('set_cps', cps);
}

/**
 * Render `source` to a WAV at `path` over `opts.cycles` cycles, on a background
 * job. Returns the job id immediately; completion flows through the Jobs overlay.
 */
export function groveRender(
  source: string,
  path: string,
  opts: GroveRenderOpts,
  projectDir?: string,
): Promise<string> {
  return invoke('grove_render', { source, project_dir: projectDir ?? null, path, opts });
}

/** Read the VSCO 2 sample-bank install status. */
export function groveVscoStatus(): Promise<GroveVscoStatus> {
  return invoke('grove_vsco_status');
}

/** Start downloading + installing the VSCO 2 bank (job-tracked). Returns job id. */
export function groveVscoDownload(): Promise<string> {
  return invoke('grove_vsco_download');
}

/** Read the grove config (`[grove]` in the global config.toml). */
export function getGroveConfig(): Promise<GroveConfig> {
  return invoke('get_grove_config');
}

/** Persist a new grove config. Takes effect for the next session / render. */
export function setGroveConfig(grove: GroveConfig): Promise<void> {
  return invoke('set_grove_config', { grove });
}

// ── Event subscriptions ───────────────────────────────────────────────────────
//
// Each returns the Tauri `UnlistenFn` to detach. Scope listeners to the grove
// window's lifetime (subscribe on mount, call the returned fn on teardown).

/** Subscribe to evaluation diagnostics. */
export function onGroveDiagnostics(cb: (d: GroveDiagnostics) => void): Promise<UnlistenFn> {
  return listen<GroveDiagnostics>(GROVE_EVENTS.diagnostics, (e) => cb(e.payload));
}

/** Subscribe to the active-hap highlight set (emitted on change). */
export function onGroveActiveHaps(cb: (h: GroveActiveHaps) => void): Promise<UnlistenFn> {
  return listen<GroveActiveHaps>(GROVE_EVENTS.activeHaps, (e) => cb(e.payload));
}

/** Subscribe to audio meters / telemetry (~30 fps). */
export function onGroveMeters(cb: (m: GroveMeters) => void): Promise<UnlistenFn> {
  return listen<GroveMeters>(GROVE_EVENTS.meters, (e) => cb(e.payload));
}

/** Subscribe to transport position / tempo (~30 fps). */
export function onGroveTransport(cb: (t: GroveTransport) => void): Promise<UnlistenFn> {
  return listen<GroveTransport>(GROVE_EVENTS.transport, (e) => cb(e.payload));
}

/** Subscribe to script log lines (already threshold-gated at the source). */
export function onGroveLog(cb: (l: GroveLogLine) => void): Promise<UnlistenFn> {
  return listen<GroveLogLine>(GROVE_EVENTS.log, (e) => cb(e.payload));
}

/** Subscribe to VSCO 2 install progress. */
export function onGroveVscoProgress(cb: (p: GroveVscoProgress) => void): Promise<UnlistenFn> {
  return listen<GroveVscoProgress>(GROVE_EVENTS.vscoProgress, (e) => cb(e.payload));
}

/** Subscribe to a fatal audio-device error (the session thread exited). */
export function onGroveAudioError(cb: (e: GroveAudioError) => void): Promise<UnlistenFn> {
  return listen<GroveAudioError>(GROVE_EVENTS.audioError, (ev) => cb(ev.payload));
}

// ════════════════════════════════════════════════════════════════════════════
// Additive surface (Fase 4 · Step 1) — extends the frozen contract WITHOUT
// breaking it: new commands only, same snake_case discipline. These feed the
// Step 2/3 fan-outs (arrangement viz, sound bank, mixer) + the project model.
// ════════════════════════════════════════════════════════════════════════════

// ── grove_query: the whole arrangement timeline (off-thread Pattern query) ─────
//
// `active_haps` only reports what sounds *now*; the arrangement view needs the
// full timeline. `grove_query` queries the last-evaluated `Tracks` over
// `[0, cycles)` off the audio thread and returns every hap. Empty when nothing
// has been evaluated yet.

/** One hap of the queried arrangement. `start`/`end` are in cycles (absolute
 *  timeline); `has_onset` is false for continuous signals (no `whole`). */
export interface GroveQueryHap {
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
export interface GroveQuerySection {
  /** Owning mixer-strip / arrangement-lane index (0-based). */
  track: number;
  /** Section label (`section("INTRO", …)`). */
  name: string;
  /** Start cycle (absolute, inclusive). */
  start: number;
  /** End cycle (absolute, exclusive). */
  end: number;
}

/** `grove_query` result: every hap + every named section over the window. */
export interface GroveQueryHaps {
  haps: GroveQueryHap[];
  /** Named section bands (empty unless a track uses `arrange(section(...))`). */
  sections: GroveQuerySection[];
}

/** Query the last-evaluated arrangement over `[0, cycles)`. Empty until an eval
 *  has succeeded. Off the audio thread — safe to call while playing. */
export function groveQuery(cycles: number): Promise<GroveQueryHaps> {
  return invoke('grove_query', { cycles });
}

// ── grove_sounds: the resolvable instrument list (registry introspection) ──────

export type GroveInstrumentKind = 'synth' | 'sample' | 'sfz';

/** One resolvable voice in the sound registry. */
export interface GroveInstrument {
  /** Dotted registry name (`strings.violin`) or a short bank name (`bd`). */
  name: string;
  kind: GroveInstrumentKind;
  /** Named articulations the instrument exposes (`.art("…")`), sorted; empty for
   *  synth / sample voices. */
  articulations: string[];
}

/** `grove_sounds` result. Always includes the built-in default synth. */
export interface GroveSoundList {
  instruments: GroveInstrument[];
}

/** List the instruments the engine can currently resolve (default synth + any
 *  installed VSCO/manifest entries). Reflects the real registry, not a static
 *  list, so it tracks what's actually installed. */
export function groveSounds(): Promise<GroveSoundList> {
  return invoke('grove_sounds');
}

// ── grove_set_track: live mixer overrides (ephemeral; eval re-baselines) ───────
//
// The source stays authoritative: on every eval the arrangement re-establishes
// the baseline. These overrides are live session tweaks on top, applied in
// real-time (smooth knob drag), released at the next eval. Surgical "commit
// knob → source literal" is the future `grove_set_literal`.

/** A live mixer override target. `master_gain` ignores `track`. */
export type GroveTrackAction = 'gain' | 'pan' | 'mute' | 'solo' | 'master_gain';

/** Push a live mixer override to the running session (no-op when stopped).
 *  `value` is 0..1 for gain/pan/master_gain, and 0|1 (off|on) for mute/solo. */
export function groveSetTrack(action: GroveTrackAction, track: number | null, value: number): Promise<void> {
  return invoke('grove_set_track', { action, track: track ?? null, value });
}

// ── Project model: open / create a grove project (folder + grove.toml) ─────────

/** One `.grove` file in a project (source read lazily on the FE via `fs_*`). */
export interface GroveProjectFile {
  /** Absolute path. */
  path: string;
  /** Project-relative path (forward slashes), e.g. `lib/drums.grove`. */
  rel: string;
  /** File name with extension. */
  name: string;
  /** Listed under `libraries` in grove.toml: imported-only, its `tracks(…)` ignored. */
  library: boolean;
}

/** A grove project manifest (`grove.toml`) + its `.grove` files. */
export interface GroveProjectInfo {
  /** Absolute project folder. */
  path: string;
  /** `name` from grove.toml (falls back to the folder name). */
  name: string;
  /** `audience` ("for whom") from grove.toml. */
  audience: string;
  files: GroveProjectFile[];
}

/** Open a grove project folder: parse `grove.toml`, list its `.grove` files. */
export function groveOpenProject(dir: string): Promise<GroveProjectInfo> {
  return invoke('grove_open_project', { dir });
}

/** Scaffold a new grove project at `dir` (writes `grove.toml` + a starter
 *  `.grove`), returning the opened manifest. */
export function groveCreateProject(dir: string, name: string, audience: string): Promise<GroveProjectInfo> {
  return invoke('grove_create_project', { dir, name, audience });
}

// ── Persisted grove window state (recents + last project + layout) ─────────────
//
// A dedicated grove state file (NOT localStorage, NOT the per-project grove.toml,
// NOT the typed [grove] settings): recents/last-project are global app state,
// the layout is the window's panel arrangement.

/** Persisted panel layout of the grove window. */
export interface GroveLayoutState {
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

/** The dedicated grove window state file. */
export interface GroveWorkspaceState {
  /** Recently-opened project folders, most-recent first. */
  recent_projects: string[];
  /** Project folder to reopen on launch, or null. */
  last_project: string | null;
  layout: GroveLayoutState;
}

/** Read the persisted grove window state (recents + last project + layout). */
export function getGroveState(): Promise<GroveWorkspaceState> {
  return invoke('get_grove_state');
}

/** Persist the grove window state. */
export function setGroveState(state: GroveWorkspaceState): Promise<void> {
  return invoke('set_grove_state', { state });
}

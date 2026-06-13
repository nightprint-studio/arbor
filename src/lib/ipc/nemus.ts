/**
 * nemus IPC — the **frozen BE↔FE contract** (Onda 4).
 *
 * Types only + thin `invoke` / `listen` wrappers — **no UI, no state**. This is
 * the seam the nemus window (Fase 4) builds on: every payload here mirrors a
 * serde struct in `src-tauri/src/nemus/` 1:1, **field-for-field in snake_case**
 * (the Rust wire shape is authoritative; do not camelCase the payloads).
 *
 * - Commands: `nemus_eval` / `nemus_transport` / `nemus_render` / sample-pack
 *   list + download / get + set config. Command argument keys are **camelCase**:
 *   Tauri maps a camelCase invoke key to the snake_case Rust parameter (e.g.
 *   `packId` → `pack_id`), like the rest of the app's IPC. (Distinct from the
 *   *event payloads* above, which stay snake_case — those are serde structs read
 *   field-for-field, not invoke arguments.)
 * - Events (nemus-window scoped): `nemus:diagnostics` / `active_haps` / `meters`
 *   / `transport` / `log` / `pack_progress` / `audio_error`. The audio thread
 *   throttles `transport`/`meters` to ~30 fps and emits `active_haps` only when
 *   the sounding set changes; `log` is gated at the source by the threshold.
 */

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

// ── Event name constants (mirror `events.rs`) ─────────────────────────────────

export const NEMUS_EVENTS = {
  diagnostics: 'nemus:diagnostics',
  activeHaps:  'nemus:active_haps',
  meters:      'nemus:meters',
  transport:   'nemus:transport',
  log:         'nemus:log',
  packProgress:'nemus:pack_progress',
  audioError:  'nemus:audio_error',
} as const;

// ── Event payloads (mirror the serde structs) ─────────────────────────────────

/** Severity of a {@link NemusDiagnostic} (today the evaluator emits only `error`). */
export type NemusSeverity = 'error' | 'warning' | 'info';

/** One located diagnostic; `start`/`end` are byte offsets into the source. */
export interface NemusDiagnostic {
  message:  string;
  severity: NemusSeverity;
  start:    number | null;
  end:      number | null;
}

/** `nemus:diagnostics` payload (also the `nemus_eval` return). Empty = success. */
export interface NemusDiagnostics {
  errors: NemusDiagnostic[];
}

/** `nemus:transport` payload: playhead position, tempo, run state. */
export interface NemusTransport {
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
export type NemusStereoPeak = [number, number];

/** `nemus:meters` payload: audio-engine telemetry sampled at the tick rate. */
export interface NemusMeters {
  /** Master output peak (post-limiter). */
  master: NemusStereoPeak;
  /** Per-track post-fader peak, indexed by mixer strip (arrangement order). */
  tracks: NemusStereoPeak[];
  /** Currently sounding voice count. */
  voices: number;
  /** DSP load `0.0..~1.0` (1.0 ≈ the audio callback is using its whole budget). */
  dsp_load: number;
}

/** One sounding source range, for the live editor highlight. */
export interface NemusActiveHap {
  /** Source byte-range start. */
  start: number;
  /** Source byte-range end. */
  end: number;
  /** Mixer-strip index owning it (so the highlight can be coloured per track). */
  track: number;
}

/** `nemus:active_haps` payload: every source range sounding at the playhead. */
export interface NemusActiveHaps {
  haps: NemusActiveHap[];
}

/** `nemus:log` payload: one threshold-gated log line. */
export interface NemusLogLine {
  /** `trace` | `debug` | `info` | `warn` | `error`. */
  level: string;
  message: string;
}

/** `nemus:pack_progress` payload; `pct` is `-1` when the total is unknown. */
export interface NemusPackProgress {
  job_id: string;
  /** Which pack is installing (`vsco` | `dirt-samples` | `drum-machines` | …). */
  pack_id: string;
  /** `downloading` | `extracting`. */
  phase: string;
  done: number;
  total: number;
  pct: number;
}

/** `nemus:audio_error` payload: the audio device could not be opened. */
export interface NemusAudioError {
  message: string;
}

// ── Command request / response types ──────────────────────────────────────────

/** One downloadable sample pack with its install status (`nemus_packs`). */
export interface NemusPack {
  /** Stable id used in commands (`vsco` | `dirt-samples` | `drum-machines` | …). */
  id: string;
  /** Human label for the UI. */
  name: string;
  /** One-line description of the pack's contents. */
  description: string;
  /** Rough download size in bytes, for a pre-install estimate (`~N MB`). */
  approx_bytes: number;
  installed: boolean;
  path: string;
  size_bytes: number;
  sha256: string | null;
  instrument_count: number;
}

/** Offline-render defaults (`[nemus].render`). */
export interface NemusRenderConfig {
  sample_rate: number;
  /** `int24` | `float32`. */
  bit_depth: string;
  tail_max_secs: number;
  /** `wav` | `ogg` — the remembered default export format. */
  format: string;
}

/** Persisted nemus settings (`[nemus]` in the global config). */
export interface NemusConfig {
  default_octave: number;
  default_cps: number;
  /** `trace` | `debug` | `info` | `warn` | `error`. */
  log_threshold: string;
  render: NemusRenderConfig;
  vsco_dir: string | null;
  packs_dir: string | null;
  /** Chosen audio output device (cpal name), or null for the host default. */
  output_device: string | null;
}

/** One selectable audio output device (`nemus_audio_devices`). */
export interface NemusAudioDevice {
  name: string;
  is_default: boolean;
}

/** Options for `nemus_render`. `cycles` is required (a Pattern has no length). */
export interface NemusRenderOpts {
  cycles: number;
  /** `int24` | `float32` — overrides the config default when set. */
  bit_depth?: string;
  tail_max_secs?: number;
  sample_rate?: number;
  /** `wav` | `ogg` — output container/codec. Defaults to WAV. */
  format?: string;
}

/** A `nemus_transport` verb. */
export type NemusTransportAction = 'play' | 'stop' | 'seek' | 'set_cps';

// ── Commands ──────────────────────────────────────────────────────────────────

/**
 * Evaluate `.nemus` source and stage it as the live arrangement. Returns
 * diagnostics inline (also emitted as `nemus:diagnostics`); a language error is
 * a diagnostic, not a rejection, so this resolves `Ok` with non-empty `errors`.
 * Does **not** open the audio device — that happens on the first `play`.
 */
export function nemusEval(source: string, projectDir?: string): Promise<NemusDiagnostics> {
  return invoke('nemus_eval', { source, projectDir: projectDir ?? null });
}

/** Low-level transport command. Prefer the named helpers below. */
export function nemusTransport(action: NemusTransportAction, value?: number): Promise<void> {
  return invoke('nemus_transport', { action, value: value ?? null });
}

/** Start playback (opens the audio device on first call). */
export function nemusPlay(): Promise<void> {
  return nemusTransport('play');
}

/** Stop and release all voices (the clock keeps its position). */
export function nemusStop(): Promise<void> {
  return nemusTransport('stop');
}

/** Jump the cycle clock so `cycle` aligns with the current frame. */
export function nemusSeek(cycle: number): Promise<void> {
  return nemusTransport('seek', cycle);
}

/** Change tempo (applied quantized at the next cycle boundary). */
export function nemusSetCps(cps: number): Promise<void> {
  return nemusTransport('set_cps', cps);
}

/**
 * Render `source` to a WAV at `path` over `opts.cycles` cycles, on a background
 * job. Returns the job id immediately; completion flows through the Jobs overlay.
 */
export function nemusRender(
  source: string,
  path: string,
  opts: NemusRenderOpts,
  projectDir?: string,
): Promise<string> {
  return invoke('nemus_render', { source, projectDir: projectDir ?? null, path, opts });
}

/** List every downloadable sample pack with its install status. */
export function nemusPacks(): Promise<NemusPack[]> {
  return invoke('nemus_packs');
}

/** Start downloading + installing a sample pack by id (job-tracked). Returns job id. */
export function nemusPackDownload(packId: string): Promise<string> {
  return invoke('nemus_pack_download', { packId });
}

/**
 * Re-index an installed pack: rebuild its registry from the extracted files on
 * disk (no re-download), refreshing the instruments it exposes. Returns the
 * updated pack status. Re-read packs + sounds afterwards.
 */
export function nemusPackReindex(packId: string): Promise<NemusPack> {
  return invoke('nemus_pack_reindex', { packId });
}

/** Delete an installed sample pack from disk. Re-read packs + sounds afterwards. */
export function nemusPackDelete(packId: string): Promise<void> {
  return invoke('nemus_pack_delete', { packId });
}

/** Read the nemus config (nemus's own `config.toml`). */
export function getNemusConfig(): Promise<NemusConfig> {
  return invoke('get_nemus_config');
}

/** Persist a new nemus config. Takes effect for the next session / render. */
export function setNemusConfig(nemus: NemusConfig): Promise<void> {
  return invoke('set_nemus_config', { nemus });
}

/** List the host's audio output devices (name + whether it's the system default). */
export function nemusAudioDevices(): Promise<NemusAudioDevice[]> {
  return invoke('nemus_audio_devices');
}

/** Choose the audio output device (cpal name, or null for the host default).
 *  Persists the choice and switches a live session to it immediately. */
export function nemusSetOutputDevice(device: string | null): Promise<void> {
  return invoke('nemus_set_output_device', { device });
}

// ── Event subscriptions ───────────────────────────────────────────────────────
//
// Each returns the Tauri `UnlistenFn` to detach. Scope listeners to the nemus
// window's lifetime (subscribe on mount, call the returned fn on teardown).

/** Subscribe to evaluation diagnostics. */
export function onNemusDiagnostics(cb: (d: NemusDiagnostics) => void): Promise<UnlistenFn> {
  return listen<NemusDiagnostics>(NEMUS_EVENTS.diagnostics, (e) => cb(e.payload));
}

/** Subscribe to the active-hap highlight set (emitted on change). */
export function onNemusActiveHaps(cb: (h: NemusActiveHaps) => void): Promise<UnlistenFn> {
  return listen<NemusActiveHaps>(NEMUS_EVENTS.activeHaps, (e) => cb(e.payload));
}

/** Subscribe to audio meters / telemetry (~30 fps). */
export function onNemusMeters(cb: (m: NemusMeters) => void): Promise<UnlistenFn> {
  return listen<NemusMeters>(NEMUS_EVENTS.meters, (e) => cb(e.payload));
}

/** Subscribe to transport position / tempo (~30 fps). */
export function onNemusTransport(cb: (t: NemusTransport) => void): Promise<UnlistenFn> {
  return listen<NemusTransport>(NEMUS_EVENTS.transport, (e) => cb(e.payload));
}

/** Subscribe to script log lines (already threshold-gated at the source). */
export function onNemusLog(cb: (l: NemusLogLine) => void): Promise<UnlistenFn> {
  return listen<NemusLogLine>(NEMUS_EVENTS.log, (e) => cb(e.payload));
}

/** Subscribe to sample-pack install progress (carries `pack_id`). */
export function onNemusPackProgress(cb: (p: NemusPackProgress) => void): Promise<UnlistenFn> {
  return listen<NemusPackProgress>(NEMUS_EVENTS.packProgress, (e) => cb(e.payload));
}

/** Subscribe to a fatal audio-device error (the session thread exited). */
export function onNemusAudioError(cb: (e: NemusAudioError) => void): Promise<UnlistenFn> {
  return listen<NemusAudioError>(NEMUS_EVENTS.audioError, (ev) => cb(ev.payload));
}

// ════════════════════════════════════════════════════════════════════════════
// Additive surface (Fase 4 · Step 1) — extends the frozen contract WITHOUT
// breaking it: new commands only, same snake_case discipline. These feed the
// Step 2/3 fan-outs (arrangement viz, sound bank, mixer) + the project model.
// ════════════════════════════════════════════════════════════════════════════

// ── nemus_query: the whole arrangement timeline (off-thread Pattern query) ─────
//
// `active_haps` only reports what sounds *now*; the arrangement view needs the
// full timeline. `nemus_query` queries the last-evaluated `Tracks` over
// `[0, cycles)` off the audio thread and returns every hap. Empty when nothing
// has been evaluated yet.

/** One hap of the queried arrangement. `start`/`end` are in cycles (absolute
 *  timeline); `has_onset` is false for continuous signals (no `whole`). */
export interface NemusQueryHap {
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
export interface NemusQuerySection {
  /** Owning mixer-strip / arrangement-lane index (0-based). */
  track: number;
  /** Section label (`section("INTRO", …)`). */
  name: string;
  /** Start cycle (absolute, inclusive). */
  start: number;
  /** End cycle (absolute, exclusive). */
  end: number;
}

/** `nemus_query` result: every hap + every named section over the window. */
export interface NemusQueryHaps {
  haps: NemusQueryHap[];
  /** Named section bands (empty unless a track uses `arrange(section(...))`). */
  sections: NemusQuerySection[];
  /** Period (in cycles) after which the whole arrangement repeats — the natural
   *  render length. `0` only when there are no haps at all. */
  loop_cycles: number;
  /** Effective render tempo (cycles/s): the arrangement's starting `tempo(...)`
   *  point, else its `cps(...)`. `null` when the script set neither (fall back to
   *  the configured default). Mirrors how `nemus_render` picks the bounce tempo. */
  cps: number | null;
}

/** Query the last-evaluated arrangement over `[0, cycles)`. Empty until an eval
 *  has succeeded. Off the audio thread — safe to call while playing. */
export function nemusQuery(cycles: number): Promise<NemusQueryHaps> {
  return invoke('nemus_query', { cycles });
}

// ── nemus_sounds: the resolvable instrument list (registry introspection) ──────

export type NemusInstrumentKind = 'synth' | 'sample' | 'sfz';

/** One resolvable voice in the sound registry. */
export interface NemusInstrument {
  /** Dotted registry name (`strings.violin`) or a short bank name (`bd`). */
  name: string;
  kind: NemusInstrumentKind;
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

/** `nemus_sounds` result. Always includes the built-in default synth. */
export interface NemusSoundList {
  instruments: NemusInstrument[];
}

/** List the instruments the engine can currently resolve (built-in synths + any
 *  installed sample pack). Reflects the real registry, not a static list, so it
 *  tracks what's actually installed. */
export function nemusSounds(): Promise<NemusSoundList> {
  return invoke('nemus_sounds');
}

// ── nemus_set_track: live mixer overrides (ephemeral; eval re-baselines) ───────
//
// The source stays authoritative: on every eval the arrangement re-establishes
// the baseline. These overrides are live session tweaks on top, applied in
// real-time (smooth knob drag), released at the next eval. Surgical "commit
// knob → source literal" is the future `nemus_set_literal`.

/** A live mixer override target. `master_gain` ignores `track`. */
export type NemusTrackAction = 'gain' | 'pan' | 'mute' | 'solo' | 'master_gain';

/** Push a live mixer override to the running session (no-op when stopped).
 *  `value` is 0..1 for gain/pan/master_gain, and 0|1 (off|on) for mute/solo. */
export function nemusSetTrack(action: NemusTrackAction, track: number | null, value: number): Promise<void> {
  return invoke('nemus_set_track', { action, track: track ?? null, value });
}

// ── nemus_audition_expr: one-off instrument preview from a generated snippet ────

/** Play a one-off instrument preview from a `.nemus` snippet. The caller composes
 *  a tiny expression — a note (or chord / scale degree) plus the panel's knob /
 *  chain values, e.g. `n(c4).inst("synth.bass").gain(0.8).room(0.2)` — which the
 *  backend evaluates with the real language and plays one cycle of on a dedicated
 *  audition bus (bypasses the song mixer, so it's heard cleanly whether or not a
 *  song is playing). Opens the audio device on demand; a malformed snippet simply
 *  doesn't sound. The whole language drives the preview — no per-param plumbing. */
export function nemusAuditionExpr(expr: string, projectDir?: string): Promise<void> {
  return invoke('nemus_audition_expr', { expr, projectDir: projectDir ?? null });
}

// ── Project model: open / create a nemus project (folder + nemus.toml) ─────────

/** One `.nemus` file in a project (source read lazily on the FE via `fs_*`). */
export interface NemusProjectFile {
  /** Absolute path. */
  path: string;
  /** Project-relative path (forward slashes), e.g. `lib/drums.nemus`. */
  rel: string;
  /** File name with extension. */
  name: string;
  /** Listed under `libraries` in nemus.toml: imported-only, its `tracks(…)` ignored. */
  library: boolean;
}

/** A nemus project manifest (`nemus.toml`) + its `.nemus` files. */
export interface NemusProjectInfo {
  /** Absolute project folder. */
  path: string;
  /** `name` from nemus.toml (falls back to the folder name). */
  name: string;
  /** `audience` ("for whom") from nemus.toml. */
  audience: string;
  files: NemusProjectFile[];
}

/** Open a nemus project folder: parse `nemus.toml`, list its `.nemus` files. */
export function nemusOpenProject(dir: string): Promise<NemusProjectInfo> {
  return invoke('nemus_open_project', { dir });
}

/** Scaffold a new nemus project at `dir` (writes `nemus.toml` + a starter
 *  `.nemus`), returning the opened manifest. */
export function nemusCreateProject(dir: string, name: string, audience: string): Promise<NemusProjectInfo> {
  return invoke('nemus_create_project', { dir, name, audience });
}

/** Rename a project — set the root `name` in `nemus.toml` (preserves the rest of
 *  the manifest), returning the re-opened project. */
export function nemusSetProjectName(dir: string, name: string): Promise<NemusProjectInfo> {
  return invoke('nemus_set_project_name', { dir, name });
}

// ── Persisted nemus window state (recents + last project + layout) ─────────────
//
// A dedicated nemus state file (NOT localStorage, NOT the per-project nemus.toml,
// NOT the typed [nemus] settings): recents/last-project are global app state,
// the layout is the window's panel arrangement.

/** Persisted panel layout of the nemus window. */
export interface NemusLayoutState {
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

/** The dedicated nemus window state file. */
export interface NemusWorkspaceState {
  /** Recently-opened project folders, most-recent first. */
  recent_projects: string[];
  /** Project folder to reopen on launch, or null. */
  last_project: string | null;
  layout: NemusLayoutState;
}

/** Read the persisted nemus window state (recents + last project + layout). */
export function getNemusState(): Promise<NemusWorkspaceState> {
  return invoke('get_nemus_state');
}

/** Persist the nemus window state. */
export function setNemusState(state: NemusWorkspaceState): Promise<void> {
  return invoke('set_nemus_state', { state });
}

// ── nemus_lang_reference: the canonical DSL catalogue (autocomplete + hover) ───
//
// The `.nemus` language reference is authored once in Rust (`arbor-nemus-lang`'s
// `reference()`); the FE loads it once and drives autocomplete, hover docs, and
// the Docs panel off it — so the editor's language intelligence and the
// evaluator can never drift. Mirrors the serde structs in
// `crates/nemus/arbor-nemus-lang/src/reference.rs` field-for-field.

/** Category of a {@link NemusDslEntry} (matches the serde `snake_case` tag). */
export type NemusDslKind =
  | 'combinator' | 'generator' | 'signal' | 'signal_method' | 'transform'
  | 'seq_method' | 'island' | 'keyword' | 'log' | 'mini' | 'note';

/** One parameter of a DSL entry. */
export interface NemusDslParam {
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
export interface NemusDslEntry {
  /** The bare name as typed (`gain`, `par`, `sine`, `~`). */
  name: string;
  kind: NemusDslKind;
  /** One-line signature, e.g. `gain(x, pat) -> pat`. */
  signature: string;
  /** 1–2 sentence description. */
  summary: string;
  /** Its parameters in order (empty for nullary forms / operators). */
  params: NemusDslParam[];
  /** A short, realistic usage snippet. */
  example: string;
  /** What the call returns, when not obvious from the signature. */
  returns?: string;
}

/** Read the full `.nemus` DSL reference catalogue (static; load once). */
export function nemusLangReference(): Promise<NemusDslEntry[]> {
  return invoke('nemus_lang_reference');
}

/** Reformat `.nemus` source to canonical style (the AST pretty-printer). Rejects
 *  with the language error when the source has a syntax error — the caller then
 *  leaves the buffer untouched. The round-trip is semantic, not byte-exact:
 *  comments and incidental whitespace are not preserved. */
export function nemusFormat(source: string): Promise<string> {
  return invoke('nemus_format', { source });
}

/** One scale mode in the catalogue: canonical name + aliases + ascending semitone
 *  intervals (one octave from the root). */
export interface NemusScaleMode {
  name: string;
  aliases: string[];
  intervals: number[];
}

/** Read the scale-mode catalogue (`.scale("root:mode")` modes); load once. */
export function nemusScales(): Promise<NemusScaleMode[]> {
  return invoke('nemus_scales');
}

// ── External libraries (`[libraries]` in nemus.toml → `$lib/…` imports) ────────

/** One declared library's state: its source spec, the pinned commit SHA (when
 *  locked), and whether its cache is present (synced). */
export interface NemusLibraryStatus {
  name: string;
  source: string;
  sha: string | null;
  synced: boolean;
}

/** The project's declared libraries with their lock / sync state. */
export function nemusLibraries(projectDir: string): Promise<NemusLibraryStatus[]> {
  return invoke('nemus_libraries', { projectDir });
}

/** Start a background sync of the project's libraries (resolve refs → SHAs,
 *  download missing commits, rewrite `nemus.lock`). Returns the job id. */
export function nemusSyncLibraries(projectDir: string): Promise<string> {
  return invoke('nemus_sync_libraries', { projectDir });
}

// ── Audio / MIDI import (WAV → MIDI, MIDI → .nemus) ───────────────────────────

/** Options for the import commands (all optional; the backend fills defaults). */
export interface NemusImportOpts {
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

/**
 * D4 — transcribe a WAV and write a `.mid` to `output`. Returns the job id;
 * progress/completion arrive on `arbor://job-progress` / `job-done`.
 */
export function nemusConvertWavToMidi(
  input: string,
  output: string,
  opId?: string,
  opts?: NemusImportOpts,
): Promise<string> {
  return invoke('nemus_convert_wav_to_midi', { input, output, opId: opId ?? null, opts: opts ?? null });
}

/**
 * D3 — transcribe a WAV and return idiomatic `.nemus` text (the MIDI never
 * touches disk). `opId` correlates the backend progress/done events with a
 * client-side transfer so the UI can show a live bar; open the result in a tab.
 */
export function nemusImportAudioAsNemus(
  input: string,
  opId?: string,
  opts?: NemusImportOpts,
): Promise<string> {
  return invoke('nemus_import_audio_as_nemus', { input, opId: opId ?? null, opts: opts ?? null });
}

/** D5 — convert an existing `.mid` to idiomatic `.nemus` text (no transcription). */
export function nemusImportMidiAsNemus(input: string, opts?: NemusImportOpts): Promise<string> {
  return invoke('nemus_import_midi_as_nemus', { input, opts: opts ?? null });
}

// ── ONNX transcription models (downloaded on-demand) ──────────────────────────

/** State of one downloadable transcription model (mirrors `nemus::models`). */
export interface NemusModelStatus {
  id: string;
  name: string;
  description: string;
  approx_bytes: number;
  installed: boolean;
  path: string;
  size_bytes: number;
}

/** List every transcription model with its install state. */
export function nemusModels(): Promise<NemusModelStatus[]> {
  return invoke('nemus_models');
}

/** Start a background download of model `id` (returns the job id; progress on
 *  `arbor://job-progress` / `job-done`). */
export function nemusDownloadModel(id: string): Promise<string> {
  return invoke('nemus_download_model', { id });
}

/** Delete a downloaded model. */
export function nemusDeleteModel(id: string): Promise<void> {
  return invoke('nemus_delete_model', { id });
}

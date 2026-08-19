/**
 * Tyto recorder IPC — the **BE↔FE contract** for `tyto-be`.
 *
 * Types + thin `tyto(...)` wrappers only — no UI, no state. Every payload mirrors
 * a serde struct in `crates/products/tyto/be/src/` (or `tyto-core`) 1:1,
 * **field-for-field in snake_case** (the Rust wire shape is authoritative — do not
 * camelCase the payloads; they're forwarded verbatim inside the opaque `params`).
 *
 * Commands route through the generic Model-D `rpc` bridge to `tyto-be` via the
 * bound {@link tyto} helper. The capture handlers are **real** — recording (scap +
 * ffmpeg), system-audio loopback + mic, screenshots (GDI) and the on-disk library
 * all run through them. Availability is signalled by the shell over `listen`
 * (`arbor://tyto-be-up` / `-down`) — see {@link listenTytoBackend}; the store keeps a
 * mock fallback only for when the backend is detached.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { tyto } from '../rpc';

// ── Backend availability events (emitted by the shell) ────────────────────────

export const TYTO_BE_UP = 'arbor://tyto-be-up';
export const TYTO_BE_DOWN = 'arbor://tyto-be-down';

/** The OS answered the screen-recording permission (payload: granted). Emitted by
 *  the shell, which asks when the Tyto window opens — the dialog can outlive the
 *  window's first source fetch, so the answer arrives as an event rather than being
 *  something the frontend could have waited for. */
export const TYTO_CAPTURE_PERMISSION = 'tyto://capture-permission';

/**
 * Subscribe to `tyto-be` attach/detach. `onUp` fires when the backend attaches
 * (the window should (re)fetch its sources / config / library); `onDown` when it
 * detaches (crash / teardown). Returns a disposer that removes both listeners.
 */
export async function listenTytoBackend(onUp: () => void, onDown: () => void): Promise<UnlistenFn> {
  const ups = await listen(TYTO_BE_UP, () => onUp());
  const downs = await listen(TYTO_BE_DOWN, () => onDown());
  return () => { ups(); downs(); };
}

// ── Config (real today — `tyto-core::config`) ─────────────────────────────────

export interface TytoCaptureConfig {
  fps: number;
  system_audio: boolean;
  mic_id: string;
  /** Seconds of 3-2-1 countdown before a video recording starts (0 = off). */
  countdown_secs: number;
}
export interface TytoEncodingConfig {
  quality: string;
  /** Derived backend-side from `quality` — sent for completeness, never authored
   *  here. Read it to display the bitrate; don't compute it. */
  bitrate_kbps: number;
}
export interface TytoOutputConfig {
  dir: string;
  filename_template: string;
  /** Screenshot image format: `png` | `jpg` | `webp`. */
  screenshot_format: string;
  /** Copy a screenshot to the OS clipboard right after saving (screenshots only). */
  copy_screenshot_to_clipboard: boolean;
}
/** Frame-sequence recording defaults (`record_output === 'frames'`). */
export interface TytoFramesConfig {
  /** Image format of each frame: `png` | `jpg` | `webp`. */
  format: string;
  /** Sampling ceiling in fps — the real rate is lower whenever the screen is still. */
  sample_fps: number;
  /** Downscale each frame to at most this width (0 = captured resolution). */
  max_width: number;
}
/** The typed product config (`…/tyto/config.toml`). Distinct from the launcher's
 *  `TytoConfig` (the OS-global open shortcut), which lives in `types/config.ts`. */
export interface TytoRecorderConfig {
  default_mode: string;
  default_target: string;
  /** What a recording produces: `video` (H.264 mp4) | `frames` (image sequence). */
  record_output: string;
  capture: TytoCaptureConfig;
  encoding: TytoEncodingConfig;
  output: TytoOutputConfig;
  frames: TytoFramesConfig;
}

export const getTytoConfig = () => tyto<TytoRecorderConfig>('get_tyto_config');

/** Where captures actually land, with the platform default resolved. `output.dir`
 *  is empty by default and *means* "the OS videos folder", so this is the only thing
 *  that can answer "where do my captures go" without the frontend guessing an OS. */
export const getOutputDir = () => tyto<string>('output_dir');
/** Persist the config and get back what was actually written — **normalized**, so
 *  the derived fields (the bitrate the quality preset implies) come from the one
 *  place that owns them instead of being recomputed here. */
export const setTytoConfig = (config: TytoRecorderConfig) => tyto<TytoRecorderConfig>('set_tyto_config', { config });

// ── Sources ───────────────────────────────────────────────────────────────────

export interface MonitorSource {
  id: string;
  name: string;
  resolution: string;
  scale: number;
  primary: boolean;
}
export interface WindowSource {
  id: string;
  title: string;
  app: string;
}
export interface AudioInputSource {
  id: string;
  name: string;
  is_default: boolean;
}
export interface CaptureSources {
  monitors: MonitorSource[];
  windows: WindowSource[];
  /** `null` when capture is available. Otherwise why the lists are empty, phrased
   *  for the user (a refused screen-recording permission, chiefly) — show it rather
   *  than an empty picker, which reads as a broken app. */
  unavailable: string | null;
}

export const listCaptureSources = () => tyto<CaptureSources>('list_capture_sources');
export const listAudioInputs = () => tyto<AudioInputSource[]>('list_audio_inputs');

/** Args for a live preview grab (subset of {@link StartRecordingArgs}). */
export interface PreviewSourceArgs {
  target_kind: string;
  source_id?: string | null;
  region?: PixelRectWire | null;
}

/** Grab a downscaled preview thumbnail of a source → temp PNG path. */
export const previewSource = (args: PreviewSourceArgs) => tyto<string>('preview_source', { args });

// ── Session ─────────────────────────────────────────────────────────────────

export interface StartRecordingArgs {
  target_kind: string;
  source_id?: string | null;
  fps?: number | null;
  quality?: string | null;
  system_audio?: boolean | null;
  mic_id?: string | null;
  /** Physical-pixel rectangle (monitor-local) when target_kind === 'region'. */
  region?: PixelRectWire | null;
  /** Freehand mask polygon in physical, region-local px (0-based within the crop).
   *  Honoured only by take_screenshot for a region target — pixels outside are made
   *  transparent and the file is forced to PNG. Recordings ignore it. */
  mask_points?: number[][] | null;
  /** `video` | `frames` — what this recording should produce. Omitted = the
   *  persisted default. Only the CHOICE travels: how a sequence is written (frame
   *  format, sampling ceiling, downscale) comes from the persisted config. */
  output?: string | null;
}
export interface SessionStateWire {
  session_id: string | null;
  recording: boolean;
  paused: boolean;
  elapsed_ms: number;
  /** What the running session produces: `video` | `frames`. */
  output: string;
}

/** `tyto://recording-progress` event payload (emitted by the engine ~5×/s). */
export interface RecordingProgress {
  elapsed_ms: number;
  frame_count: number;
}
export const TYTO_RECORDING_PROGRESS = 'tyto://recording-progress';

/** Subscribe to the engine's recording-progress ticks. Returns a disposer. */
export async function listenRecordingProgress(cb: (p: RecordingProgress) => void): Promise<UnlistenFn> {
  return listen<RecordingProgress>(TYTO_RECORDING_PROGRESS, (e) => cb(e.payload));
}

/** `tyto://recording-error` — the engine fires this (once) when the capture source
 *  is lost mid-recording (monitor unplugged, window closed, resolution/GPU switch).
 *  The recording should be stopped so the partial file is saved. */
export interface RecordingError {
  message: string;
}
export const TYTO_RECORDING_ERROR = 'tyto://recording-error';

/** Subscribe to the engine's recording-error event. Returns a disposer. */
export async function listenRecordingError(cb: (e: RecordingError) => void): Promise<UnlistenFn> {
  return listen<RecordingError>(TYTO_RECORDING_ERROR, (ev) => cb(ev.payload));
}

export const startRecording = (args: StartRecordingArgs) => tyto<string>('start_recording', { args });
export const stopRecording = () => tyto<void>('stop_recording');
export const pauseRecording = (paused: boolean) => tyto<void>('pause_recording', { paused });
export const takeScreenshot = (args: StartRecordingArgs) => tyto<string>('take_screenshot', { args });
export const sessionState = () => tyto<SessionStateWire>('session_state');

// ── Region ────────────────────────────────────────────────────────────────────

export interface PixelRectWire {
  x: number;
  y: number;
  w: number;
  h: number;
}
export interface SelectRegionArgs {
  monitor_id: string;
  css: PixelRectWire;
}
export interface RegionSelectionWire {
  css: PixelRectWire;
  physical: PixelRectWire;
  scale_factor: number;
}

// NB: the RPC seam keys params by the handler's PARAMETER name, so a single-arg
// handler `select_region(state, args)` needs `{ args }` (like start_recording /
// take_screenshot / preview_source) — NOT the bare object.
export const selectRegion = (args: SelectRegionArgs) => tyto<RegionSelectionWire>('select_region', { args });
export const clearRegion = () => tyto<void>('clear_region');

/** A frozen desktop snapshot for the region overlay: PNG path + the monitor's
 *  logical bounds (to size the opaque window) + scale. */
export interface FrozenFrame {
  path: string;
  monitor_id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  scale: number;
}

export const freezeScreen = (monitorId?: string | null) =>
  tyto<FrozenFrame>('freeze_screen', { monitor_id: monitorId ?? null });

/** Foreground-window UI element rects (monitor-local CSS px) for the overlay's smart
 *  pick. Empty off Windows / when the app exposes no accessibility. */
export const enumerateUiElements = (monitorId: string) =>
  tyto<PixelRectWire[]>('enumerate_ui_elements', { monitor_id: monitorId });

/** One window hover-target for the in-window selector, in **monitor-local CSS px**
 *  (clipped to the frozen monitor — same space as {@link enumerateUiElements} and
 *  `freeze_screen`). `id` = `win-<hwnd>`. */
export interface WindowPickRectWire {
  id: string;
  x: number;
  y: number;
  w: number;
  h: number;
}

/** Windows on the given monitor as monitor-local CSS-px hover rects, for the in-window
 *  Snip-style Window pick. Empty off Windows. */
export const enumerateWindowRects = (monitorId: string) =>
  tyto<WindowPickRectWire[]>('enumerate_window_rects', { monitor_id: monitorId });

// ── Library ─────────────────────────────────────────────────────────────────

export interface CaptureWire {
  id: string;
  name: string;
  /** `record` (mp4) | `screenshot` (still) | `frames` (image sequence). */
  kind: string;
  target: string;
  duration_ms: number | null;
  size_bytes: number;
  created_at: number;
  /** The file, or the `.frames` directory for a sequence. */
  path: string;
  /** Thumbnail path — only a frame sequence has one. */
  poster: string | null;
}

/** A frame sequence resolved for playback (`read_frame_sequence`). */
export interface FrameSequenceWire {
  dir: string;
  width: number;
  height: number;
  sample_fps: number;
  duration_ms: number;
  target: string;
  size_bytes: number;
  /** Absolute path of every frame, in playback order. */
  frames: string[];
  /** Presentation time of each frame, ms from the start (`times[0] === 0`). */
  times: number[];
}

/** Read a saved frame sequence: geometry, per-frame timings and frame paths. */
export const readFrameSequence = (id: string) => tyto<FrameSequenceWire>('read_frame_sequence', { id });

export const listCaptures = () => tyto<CaptureWire[]>('list_captures');
export const renameCapture = (id: string, name: string) => tyto<void>('rename_capture', { id, name });
export const removeCapture = (id: string) => tyto<void>('remove_capture', { id });
export const clearCaptures = () => tyto<void>('clear_captures');
export const revealCapture = (id: string) => tyto<void>('reveal_capture', { id });
export const openCapture = (id: string) => tyto<void>('open_capture', { id });
/** Reveal the output directory (created if missing) in the OS file manager. */
export const revealOutput = () => tyto<void>('reveal_output');

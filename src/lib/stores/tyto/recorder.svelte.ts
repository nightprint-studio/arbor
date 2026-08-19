/**
 * Tyto recorder — state store, BE-first with a mock fallback.
 *
 * When `tyto-be` is up this drives the real capture engine: it hydrates config,
 * enumerates sources / mics, records (video + optional mic and/or system audio),
 * takes screenshots and lists the on-disk library — all through the `session` /
 * `sources` / `region` / `library` handlers. The device lists are seeded with mock
 * fixtures purely so the picker isn't empty for a beat before the backend answers;
 * the library starts empty and is authoritative from `list_captures`.
 *
 * With the backend DOWN the store degrades to a local mock (a timer for "recording"
 * and synthetic capture entries) so the UI never breaks — the components don't move
 * either way.
 */

import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow } from '@tauri-apps/api/window';
import {
  listenTytoBackend, listenRecordingProgress, listenRecordingError,
  getTytoConfig, setTytoConfig, type TytoRecorderConfig,
  listCaptureSources, listAudioInputs, listCaptures, readFrameSequence, getOutputDir,
  TYTO_CAPTURE_PERMISSION,
  startRecording as beStart, stopRecording as beStop, takeScreenshot as beScreenshot,
  removeCapture as beRemove, renameCapture as beRename, clearCaptures as beClear,
  revealCapture as beReveal, openCapture as beOpen, revealOutput as beRevealOutput,
  selectRegion as beSelectRegion, freezeScreen, previewSource,
  enumerateUiElements, enumerateWindowRects,
  type CaptureWire, type StartRecordingArgs, type PixelRectWire, type WindowPickRectWire,
  type FrozenFrame, type FrameSequenceWire,
} from '$lib/ipc/tyto/recorder';
import { uiStore } from '$lib/stores/ui.svelte';
import { setTytoSelection, resetTytoBounds, screenRecordingStatus } from '$lib/ipc/tyto/main-window';
import { convertFileSrc } from '@tauri-apps/api/core';
import { openRecordingHud, closeRecordingHud, TYTO_RECORDING_STOPPED } from '$lib/ipc/tyto/hud-window';

// ── Domain model ─────────────────────────────────────────────────────────────

/** A rectangle in pixels — `x`/`y` is the top-left, `w`/`h` the size. */
export interface PixelRect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The result of a region selection (mocked: derived from the in-window drag
 *  mapped onto the chosen monitor's resolution). */
export interface RegionSelection {
  /** Rectangle in CSS pixels (the raw drag, within the selector). */
  css: PixelRect;
  /** Rectangle in physical pixels — what a real capture backend would crop. */
  physical: PixelRect;
  /** The monitor's scale factor used for the conversion. */
  scaleFactor: number;
}

export type CaptureMode = 'record' | 'screenshot';
/** What a recording produces. Not a capture MODE: the flow, the countdown, the HUD
 *  and the source picking are identical — only the sink at the end differs. */
export type RecordOutput = 'video' | 'frames';
/** What a saved capture IS on disk — a superset of {@link CaptureMode}, because a
 *  recording can have landed as a video or as an image sequence. */
export type CaptureKind = 'record' | 'screenshot' | 'frames';
export type TargetKind = 'monitor' | 'window' | 'region';
/** The pick method active inside the in-window Snip-style selector:
 *  `rect`/`free`/`smart` resolve to a region; `window`/`display` pick a whole target. */
export type SelectMethod = 'rect' | 'free' | 'smart' | 'window' | 'display';
export type Quality = 'high' | 'balanced' | 'compact';
export type Fps = 30 | 60;
export type ScreenshotFormat = 'png' | 'jpg' | 'webp';
/** Image format of each frame in a sequence — the same encoders as screenshots. */
export type FrameFormat = 'png' | 'jpg' | 'webp';

export interface MonitorTarget {
  id: string;
  name: string;
  resolution: string;
  scale: number;
  primary: boolean;
}

export interface WindowTarget {
  id: string;
  title: string;
  app: string;
}

export interface AudioInput {
  id: string;
  name: string;
  default: boolean;
}

export interface Capture {
  id: string;
  name: string;
  kind: CaptureKind;
  target: string;
  durationMs: number | null; // null for screenshots
  sizeBytes: number;
  createdAt: number;
  /** Hue (deg) for the synthetic thumbnail gradient — mock stand-in for a frame. */
  hue: number;
  /** Absolute path on disk (empty in the mock): the file, or the `.frames`
   *  directory for a sequence. Used to show the real media via `convertFileSrc`. */
  path: string;
  /** Thumbnail path — only a frame sequence has one (a video is its own poster
   *  frame, a screenshot its own thumbnail). Empty otherwise. */
  poster: string;
}

/** A frame sequence loaded for playback. Times are ms from the start; `frames[i]`
 *  is an asset URL ready for an `<img>`. */
export interface FrameSequence {
  width: number;
  height: number;
  durationMs: number;
  sampleFps: number;
  /** `convertFileSrc` URLs, in playback order. */
  frames: string[];
  /** Presentation time of each frame, ms from the start (`times[0] === 0`). */
  times: number[];
}

// ── Shared control vocabularies ──────────────────────────────────────────────
//
// The same choices are offered in the capture panel and in the settings dialog. They
// live here, once, because two hand-written copies of a list of options drift — one
// gains "WebP" and the other doesn't, and the two surfaces quietly stop agreeing on
// what the product can do.

export const TYTO_FPS_OPTIONS = [
  { value: '30', label: '30' },
  { value: '60', label: '60' },
];

export const TYTO_QUALITY_OPTIONS = [
  { value: 'high',     label: 'High' },
  { value: 'balanced', label: 'Balanced' },
  { value: 'compact',  label: 'Compact' },
];

export const TYTO_COUNTDOWN_OPTIONS = [
  { value: '0',  label: 'Off' },
  { value: '3',  label: '3s' },
  { value: '5',  label: '5s' },
  { value: '10', label: '10s' },
];

/** Encoders shared by screenshots and frame sequences — the same three either way. */
export const TYTO_IMAGE_FORMAT_OPTIONS = [
  { value: 'png',  label: 'PNG' },
  { value: 'jpg',  label: 'JPG' },
  { value: 'webp', label: 'WebP' },
];

export const TYTO_MODE_OPTIONS = [
  { value: 'record',     label: 'Record' },
  { value: 'screenshot', label: 'Screenshot' },
];

export const TYTO_OUTPUT_OPTIONS = [
  { value: 'video',  label: 'Video' },
  { value: 'frames', label: 'Frames' },
];

/** `0` is "as captured" — spelled out, because a bare 0 in a width field reads as a
 *  mistake rather than as a choice. */
export const TYTO_FRAME_WIDTH_OPTIONS = [
  { value: '0',    label: 'As captured' },
  { value: '1920', label: '1920 px' },
  { value: '1280', label: '1280 px' },
  { value: '960',  label: '960 px' },
  { value: '640',  label: '640 px' },
];

// ── Mock fixtures ────────────────────────────────────────────────────────────

const MOCK_MONITORS: MonitorTarget[] = [
  { id: 'mon-1', name: 'Display 1 · Dell U2723QE', resolution: '3840 × 2160', scale: 1.5, primary: true },
  { id: 'mon-2', name: 'Display 2 · LG 27GP850',   resolution: '2560 × 1440', scale: 1.0, primary: false },
];

const MOCK_WINDOWS: WindowTarget[] = [
  { id: 'win-1', title: 'Roman Tactics — Bevy',      app: 'roman_tactics.exe' },
  { id: 'win-2', title: 'arbor — Visual Studio Code', app: 'Code.exe' },
  { id: 'win-3', title: 'Corvus — Arbor',             app: 'arbor.exe' },
  { id: 'win-4', title: 'Documentation — Firefox',    app: 'firefox.exe' },
];

const MOCK_MICS: AudioInput[] = [
  { id: 'mic-1', name: 'Shure MV7 (USB)',       default: true },
  { id: 'mic-2', name: 'Microphone (Realtek)',  default: false },
];

// ── Store ────────────────────────────────────────────────────────────────────

function createRecorderStore() {
  let mode = $state<CaptureMode>('record');
  let targetKind = $state<TargetKind>('monitor');
  let selectedMonitorId = $state<string>(MOCK_MONITORS[0].id);
  let selectedWindowId = $state<string>(MOCK_WINDOWS[0].id);
  let region = $state<RegionSelection | null>(null);
  // Freehand mask polygon in PHYSICAL, REGION-LOCAL px (0-based within the crop),
  // set alongside `region` on a freehand confirm. Screenshots pass it as a mask so the
  // capture is the traced shape (transparent outside). Null = plain rectangle.
  let regionMask = $state<number[][] | null>(null);

  // ── In-window Snip-style selector state ─────────────────────────────────────
  // When `selecting`, the Tyto window covers ONE monitor showing a frozen backdrop and
  // the user picks directly on it (no separate tyto-region window, no poll). The rects
  // are monitor-local CSS px — same space as `freeze_screen` / `enumerate_ui_elements`.
  let selecting = $state(false);
  let selectMethod = $state<SelectMethod>('rect');
  let selectFrozenUrl = $state<string | null>(null);
  let selectMonitorId = $state<string>('');
  let selectMonitorName = $state<string>('');
  // Smart (UI-element) rects + window rects for the active frozen monitor.
  let selectElements = $state<PixelRectWire[]>([]);
  let selectWindows = $state<WindowPickRectWire[]>([]);

  let systemAudio = $state(true);
  let micId = $state<string | null>(MOCK_MICS[0].id);

  let fps = $state<Fps>(60);
  let quality = $state<Quality>('balanced');
  // Derived from `quality` by tyto-core, never here: the frontend displays the
  // number the encoder will actually use rather than keeping a second copy of the
  // preset table that could drift from it. 0 until the backend has answered.
  let bitrateKbps = $state(0);
  // What a recording produces. `frames` swaps the mp4 encoder for a deduplicated,
  // timestamped image sequence — same capture, same flow, different sink.
  let recordOutput = $state<RecordOutput>('video');
  let frameFormat = $state<FrameFormat>('png');
  let frameSampleFps = $state(12);
  // 0 = keep the captured resolution.
  let frameMaxWidth = $state(0);
  // Seconds of 3-2-1 countdown before a video recording starts (0 = off).
  let countdownSecs = $state(3);

  // ── In-window countdown state ───────────────────────────────────────────────
  // The 3-2-1 is rendered INSIDE the live Tyto window (over the frozen backdrop it's
  // already covering the monitor with) — never a separate window, so there's no webview
  // recreation / white flash. `countingDown` shows TytoCountdown; `countdownValue` is the
  // current digit; `cancelCountdownFn` lets Esc abort mid-count.
  let countingDown = $state(false);
  let countdownValue = $state(0);
  let cancelCountdownFn: (() => void) | null = null;

  // Empty until the backend resolves it: the default lives in `tyto-core` and is
  // platform-dependent, so any literal here is a wrong answer on some machine.
  let outputDir = $state('');
  // Whether that path is the user's choice or the resolved default. Load-bearing on
  // save: writing the resolved default back would turn "wherever this OS keeps
  // videos" into a fixed path, and moving the folder would stop following.
  let outputDirExplicit = false;
  // Screenshot image format (still captures only; recordings use the container).
  let screenshotFormat = $state<ScreenshotFormat>('png');
  // Copy a screenshot to the OS clipboard right after it's saved (backend, via arboard).
  let copyToClipboard = $state(true);

  let recording = $state(false);
  let elapsedMs = $state(0);
  let startedAt = 0;
  let timer: ReturnType<typeof setInterval> | null = null;

  // The library is authoritative from tyto-be (`list_captures`). Empty until the
  // backend answers; the mock fallback (backend down) fills it via `pushCapture`.
  let captures = $state<Capture[]>([]);
  let counter = 0;

  // Freshly-produced capture: the newest capture's id (highlighted briefly in the
  // library) and a monotonic signal the shell watches to reveal the library after
  // any capture — so "the library page always opens" once a recording/shot lands.
  let captureFlashId = $state<string | null>(null);
  let captureSignal = $state(0);
  let flashTimer: ReturnType<typeof setTimeout> | null = null;
  // The newest capture's id, latched for the library to auto-open its preview once. The
  // library reads + clears it (a fresh capture "shows itself" without an extra click).
  let autoPreviewId = $state<string | null>(null);
  function flashNewest() {
    const newest = captures[0];
    captureFlashId = newest ? newest.id : null;
    autoPreviewId = newest ? newest.id : null;
    if (flashTimer) clearTimeout(flashTimer);
    if (captureFlashId) flashTimer = setTimeout(() => { captureFlashId = null; }, 4000);
    captureSignal += 1;
  }

  // Device / capture lists — seeded with the mock fixtures, replaced by tyto-be's
  // enumeration when it returns any (stubs return empty today → mock stays).
  let monitors = $state<MonitorTarget[]>(MOCK_MONITORS);
  let windows = $state<WindowTarget[]>(MOCK_WINDOWS);
  let mics = $state<AudioInput[]>(MOCK_MICS);

  // True while tyto-be is attached (proven by a successful config read or the
  // shell's up/down events). Gates config persistence.
  let backendUp = $state(false);
  // One-shot guard so a late `tyto-be-up` refresh never clobbers the user's
  // in-session picks with the on-disk defaults.
  let configHydrated = false;
  let unlistenBackend: (() => void) | null = null;
  // The active recording is a real tyto-be session (drives whether stop hits the
  // backend or the mock timer).
  let beSession = false;
  // Last backend error surfaced to the UI (empty = none).
  let lastError = $state<string | null>(null);
  // Why capture can't run at all (a refused OS screen-recording permission), as the
  // backend phrased it. Distinct from `lastError`: this is a standing condition the
  // picker must explain, not an event that just happened.
  let captureUnavailable = $state<string | null>(null);
  // The shell's reading of the same permission, fetched only once capture is
  // refused. It names WHICH of the three failures this is — the recorder process
  // alone can only report that it was told no.
  let captureDiagnosis = $state<string | null>(null);

  function targetLabel(): string {
    if (targetKind === 'monitor') return monitors.find(m => m.id === selectedMonitorId)?.name.split(' · ')[0] ?? 'Monitor';
    if (targetKind === 'window') return windows.find(w => w.id === selectedWindowId)?.title ?? 'Window';
    return region ? `Region ${region.physical.w}×${region.physical.h}` : 'Region';
  }

  function stamp(): string {
    // tyto_yyyyMMdd_HHmmss — matches the spec's default filename template.
    const d = new Date();
    const p = (n: number, l = 2) => String(n).padStart(l, '0');
    return `tyto_${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}_${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
  }

  function pushCapture(kind: CaptureKind, durationMs: number | null) {
    counter += 1;
    const sizeBytes = kind === 'screenshot'
      ? 1_500_000 + Math.round((counter * 811_237) % 3_000_000)
      : Math.round((durationMs ?? 0) / 1000) * 1_620_000 + 4_000_000;
    captures = [
      {
        id: `cap-${Date.now()}-${counter}`,
        name: stamp(),
        kind,
        target: targetLabel(),
        durationMs,
        sizeBytes,
        createdAt: Date.now(),
        hue: (counter * 47 + 20) % 360,
        path: '',
        poster: '',
      },
      ...captures,
    ];
    flashNewest();
  }

  function stopTimer() {
    if (timer) { clearInterval(timer); timer = null; }
  }

  /** A capture can start only when the target is fully specified: a region needs a
   *  drawn rectangle; a window needs a still-present selection (guards the "capture
   *  with nothing/stale selected" case); a monitor always has one. */
  function isTargetReady(): boolean {
    // Nothing is capturable when the OS said no — disable the action rather than let
    // it fail, and let `notReadyReason` say why.
    if (captureUnavailable) return false;
    if (targetKind === 'region') return region !== null;
    if (targetKind === 'window') return windows.some((w) => w.id === selectedWindowId);
    return true;
  }

  // ── Backend bridge ─────────────────────────────────────────────────────────

  /** Apply the persisted capture/encoding/output *settings* (audio, fps, quality,
   *  countdown, output). Safe to call repeatedly: the settings are persisted on every
   *  change, so re-reading them yields the current values — this is what re-syncs a
   *  stale toggle after the backend attaches late (the same race Sitta hit) or on
   *  window refocus. */
  function applySettings(cfg: TytoRecorderConfig) {
    if (cfg.capture.fps === 30 || cfg.capture.fps === 60) fps = cfg.capture.fps;
    systemAudio = cfg.capture.system_audio;
    micId = cfg.capture.mic_id ? cfg.capture.mic_id : null;
    if (Number.isFinite(cfg.capture.countdown_secs)) countdownSecs = Math.max(0, Math.trunc(cfg.capture.countdown_secs));
    if (cfg.encoding.quality === 'high' || cfg.encoding.quality === 'balanced' || cfg.encoding.quality === 'compact') {
      quality = cfg.encoding.quality;
    }
    if (Number.isFinite(cfg.encoding.bitrate_kbps)) bitrateKbps = cfg.encoding.bitrate_kbps;
    if (cfg.record_output === 'video' || cfg.record_output === 'frames') recordOutput = cfg.record_output;
    if (cfg.frames) {
      const ff = cfg.frames.format;
      if (ff === 'png' || ff === 'jpg' || ff === 'webp') frameFormat = ff;
      if (Number.isFinite(cfg.frames.sample_fps)) frameSampleFps = Math.min(60, Math.max(1, Math.trunc(cfg.frames.sample_fps)));
      if (Number.isFinite(cfg.frames.max_width)) frameMaxWidth = Math.max(0, Math.trunc(cfg.frames.max_width));
    }
    if (cfg.output.dir) { outputDir = cfg.output.dir; outputDirExplicit = true; }
    const fmt = cfg.output.screenshot_format;
    if (fmt === 'png' || fmt === 'jpg' || fmt === 'webp') screenshotFormat = fmt;
    if (typeof cfg.output.copy_screenshot_to_clipboard === 'boolean') copyToClipboard = cfg.output.copy_screenshot_to_clipboard;
  }

  /** Seed the store from the on-disk product config, once. The active mode/target are
   *  seeded here only (they're session UI, not re-synced on refresh); the settings go
   *  through [`applySettings`] so they can also be re-synced later. */
  function hydrateFromConfig(cfg: TytoRecorderConfig) {
    if (!configHydrated) {
      configHydrated = true;
      if (cfg.default_mode === 'record' || cfg.default_mode === 'screenshot') mode = cfg.default_mode;
      if (cfg.default_target === 'monitor' || cfg.default_target === 'window' || cfg.default_target === 'region') {
        targetKind = cfg.default_target;
      }
    }
    applySettings(cfg);
  }

  /** Persist the current capture defaults to tyto-be (best-effort). No-op until
   *  the backend is up, so early setter calls during boot don't error-spam. */
  function persistConfig() {
    if (!backendUp) return;
    const cfg: TytoRecorderConfig = {
      default_mode:   mode,
      default_target: targetKind,
      record_output:  recordOutput,
      capture:  { fps, system_audio: systemAudio, mic_id: micId ?? '', countdown_secs: countdownSecs },
      encoding: { quality, bitrate_kbps: bitrateKbps },
      output:   { dir: outputDirExplicit ? outputDir : '', filename_template: 'tyto_%Y%m%d_%H%M%S', screenshot_format: screenshotFormat, copy_screenshot_to_clipboard: copyToClipboard },
      frames:   { format: frameFormat, sample_fps: frameSampleFps, max_width: frameMaxWidth },
    };
    // Adopt ONLY the derived values from the round trip — re-applying the whole
    // config would clobber anything the user changed while it was in flight.
    void setTytoConfig(cfg)
      .then((normalized) => { bitrateKbps = normalized.encoding.bitrate_kbps; })
      .catch(() => {});
  }

  function mapCapture(c: CaptureWire): Capture {
    let h = 0;
    for (let i = 0; i < c.id.length; i++) h = (h * 31 + c.id.charCodeAt(i)) % 360;
    return {
      id: c.id, name: c.name, kind: c.kind as CaptureKind, target: c.target,
      durationMs: c.duration_ms, sizeBytes: c.size_bytes, createdAt: c.created_at, hue: h,
      path: c.path,
      poster: c.poster ?? '',
    };
  }

  /** Pull whatever tyto-be can serve. A successful config read proves the backend
   *  is reachable (covers an already-up backend whose up-event we missed). Each
   *  list only replaces the mock when the backend returns something. */
  async function refreshFromBackend() {
    try { const cfg = await getTytoConfig(); backendUp = true; hydrateFromConfig(cfg); } catch { return; }
    // An empty `output.dir` means "the OS default" — only the backend knows which.
    if (!outputDir) { try { outputDir = await getOutputDir(); } catch { /* leave it blank */ } }
    try {
      const s = await listCaptureSources();
      captureUnavailable = s.unavailable ?? null;
      captureDiagnosis = null;
      if (captureUnavailable) {
        // Only worth a round trip when something is actually wrong.
        void screenRecordingStatus().then((st) => { captureDiagnosis = st.hint; }).catch(() => {});
        // Deliberately NOT the mock fixtures: offering a "Dell U2723QE" that cannot be
        // captured turns a permission problem into a mystery. Empty lists plus the
        // reason is the honest state.
        monitors = [];
        windows = [];
      } else {
        if (s.monitors.length) {
          monitors = s.monitors.map(m => ({ id: m.id, name: m.name, resolution: m.resolution, scale: m.scale, primary: m.primary }));
          if (!monitors.some(m => m.id === selectedMonitorId)) selectedMonitorId = monitors[0].id;
        }
        if (s.windows.length) {
          windows = s.windows.map(w => ({ id: w.id, title: w.title, app: w.app }));
          if (!windows.some(w => w.id === selectedWindowId)) selectedWindowId = windows[0].id;
        }
      }
    } catch { /* keep mock sources */ }
    try {
      const a = await listAudioInputs();
      if (a.length) mics = a.map(x => ({ id: x.id, name: x.name, default: x.is_default }));
    } catch { /* keep mock mics */ }
    await refreshCaptures(false);
  }

  /** Re-read the library from disk. `authoritative` replaces the list even when
   *  empty (after a real capture / delete); otherwise the mock seed is kept when
   *  the backend has nothing yet. */
  async function refreshCaptures(authoritative: boolean) {
    try {
      const caps = await listCaptures();
      if (authoritative || caps.length) captures = caps.map(mapCapture);
    } catch { /* keep current list */ }
  }

  /** The current selection as backend recording/screenshot args. */
  function currentArgs(): StartRecordingArgs {
    return {
      target_kind: targetKind,
      source_id: targetKind === 'monitor' ? selectedMonitorId
        : targetKind === 'window' ? selectedWindowId
        : null,
      fps,
      quality,
      system_audio: systemAudio,
      mic_id: micId,
      region: region ? { ...region.physical } : null,
      output: recordOutput,
    };
  }

  /** Run the in-window 3-2-1 countdown. Assumes the Tyto window is ALREADY covering the
   *  target monitor with the frozen backdrop up (the selector left it that way, or
   *  [`coverMonitor`] just set it) — so this only drives the digit + the timer; TytoCountdown
   *  paints over the same backdrop. Resolves `true` when it reaches zero, `false` if the
   *  user aborted it (Esc → [`cancelCountdown`]). Total wall time = `countdownSecs`s. */
  function runCountdownTimer(): Promise<boolean> {
    countingDown = true;
    countdownValue = countdownSecs;
    return new Promise((resolve) => {
      let done = false;
      const finish = (ok: boolean) => { if (done) return; done = true; cancelCountdownFn = null; resolve(ok); };
      cancelCountdownFn = () => finish(false);
      const tick = () => {
        if (done) return;
        if (countdownValue <= 1) { finish(true); return; }
        countdownValue -= 1;
        setTimeout(tick, 1000);
      };
      setTimeout(tick, 1000);
    });
  }

  /** Kick off the real backend recording: hand over to the on-screen HUD (which hides
   *  Tyto) and start the engine. On failure, surface it + restore the normal panel
   *  (the window may still be at its monitor-covering countdown bounds). */
  async function beginBackendRecording() {
    try {
      await beStart(currentArgs());
      beSession = true;
      recording = true;
      elapsedMs = 0; // driven by the tyto://recording-progress event
      void openRecordingHud(targetLabel());
    } catch (e) {
      lastError = String(e);
      uiStore.showToast(`Couldn't start recording: ${e}`, 'error');
      // The window may be at covering bounds (a countdown ran) — reset before showing so
      // it doesn't reappear monitor-sized, then reveal it.
      try { await resetTytoBounds(); } catch { /* ignore */ }
      try { await getCurrentWindow().show(); } catch { /* ignore */ }
    }
  }

  // ── In-window selector internals ─────────────────────────────────────────────

  /** Hide Tyto → freeze `monitorId` (or the primary when null) → enumerate BOTH the
   *  smart UI-element rects AND the window rects for that monitor → grow the Tyto window
   *  to cover the frozen monitor → show it. Populates all `select*` state. Returns
   *  `false` (with the error surfaced + Tyto restored) if the freeze/enumerate fails, so
   *  the caller leaves selection off. Frozen backdrop = the CURRENT monitor only (never
   *  the virtual desktop). */
  /** Hide Tyto → freeze `monitorId` (the primary when null) → grow the window to cover
   *  that monitor's PHYSICAL bounds → show it over the frozen backdrop. Sets
   *  `selectMonitorId`/`selectMonitorName`/`selectFrozenUrl`. Returns the FrozenFrame, or
   *  null (error surfaced + Tyto restored) on failure. Shared by the Snip selector and the
   *  in-window countdown; the frozen backdrop is the CURRENT monitor only.
   *
   *  PHYSICAL bounds (logical × scale) are unambiguous across monitors, so switching to a
   *  different-DPI display doesn't mis-place/mis-size the covering window. */
  async function coverMonitor(monitorId: string | null): Promise<FrozenFrame | null> {
    const win = getCurrentWindow();
    try {
      // Step Tyto aside BEFORE freezing so it isn't in the frozen backdrop; give the
      // compositor a beat to actually hide it.
      await win.hide();
      await new Promise((r) => setTimeout(r, 140));
      const frame = await freezeScreen(monitorId);
      selectMonitorId = frame.monitor_id;
      selectMonitorName = monitors.find((m) => m.id === frame.monitor_id)?.name ?? 'Display';
      selectFrozenUrl = convertFileSrc(frame.path);
      const s = frame.scale || 1;
      await setTytoSelection(
        true,
        Math.round(frame.x * s), Math.round(frame.y * s),
        Math.round(frame.width * s), Math.round(frame.height * s),
      );
      await win.show();
      return frame;
    } catch (e) {
      lastError = String(e);
      try { await win.show(); } catch { /* ignore */ }
      return null;
    }
  }

  /** Cover the monitor (via [`coverMonitor`]) AND enumerate its smart + window hover rects
   *  — the full entry into the Snip selector. Returns `false` (Tyto restored) on failure. */
  async function freezeMonitorForSelection(monitorId: string | null): Promise<boolean> {
    const frame = await coverMonitor(monitorId);
    if (!frame) return false;
    // Enumerate both pick layers for this monitor (monitor-local CSS px). Smart rects are
    // best-effort (empty off Windows / no accessibility); window rects likewise.
    const [elements, wins] = await Promise.all([
      enumerateUiElements(frame.monitor_id).catch(() => []),
      enumerateWindowRects(frame.monitor_id).catch(() => []),
    ]);
    selectElements = elements;
    selectWindows = wins;
    return true;
  }

  return {
    // ── reads ──
    get mode() { return mode; },
    get targetKind() { return targetKind; },
    get monitors() { return monitors; },
    get windows() { return windows; },
    get mics() { return mics; },
    /** True while tyto-be is attached (drives the "backend in progress" hints). */
    get backendUp() { return backendUp; },
    get selectedMonitorId() { return selectedMonitorId; },
    get selectedWindowId() { return selectedWindowId; },
    get region() { return region; },
    get systemAudio() { return systemAudio; },
    get micId() { return micId; },
    get fps() { return fps; },
    get quality() { return quality; },
    /** What a recording produces: an mp4, or a `.frames` image sequence. */
    get recordOutput() { return recordOutput; },
    /** Image format of each frame in a sequence. */
    get frameFormat() { return frameFormat; },
    /** Sampling ceiling for a frame sequence — the real rate is lower on a still screen. */
    get frameSampleFps() { return frameSampleFps; },
    /** Frame downscale width (0 = captured resolution). */
    get frameMaxWidth() { return frameMaxWidth; },
    get countdownSecs() { return countdownSecs; },
    /** Bitrate of the active quality preset, as tyto-core derived it. 0 until the
     *  backend has answered — the UI shows the codec alone rather than a made-up number. */
    get bitrateKbps() { return bitrateKbps; },
    get outputDir() { return outputDir; },
    /** Screenshot image format (`png` | `jpg` | `webp`). */
    get screenshotFormat() { return screenshotFormat; },
    get recording() { return recording; },
    get elapsedMs() { return elapsedMs; },
    get captures() { return captures; },
    get currentTargetLabel() { return targetLabel(); },
    /** Id of the most recent capture, highlighted briefly in the library. */
    get captureFlashId() { return captureFlashId; },
    /** Monotonic counter bumped after every capture — the shell reveals the library
     *  when it changes, so the library page always surfaces a fresh capture. */
    get captureSignal() { return captureSignal; },

    /** True when the active target is fully specified (region needs a rectangle). */
    get targetReady() { return isTargetReady(); },
    /** Why capture is impossible right now, or null. A standing condition (a refused
     *  screen-recording permission), not a transient error. */
    get captureUnavailable() { return captureUnavailable; },
    /** The shell's diagnosis of that refusal — which of the several ways a granted
     *  permission still fails to reach the recorder this one is. Null when there is
     *  nothing to add beyond [`captureUnavailable`]. */
    get captureDiagnosis() { return captureDiagnosis; },
    /** Why the primary action is disabled — the permission problem when there is one,
     *  otherwise the missing region. One source for every tooltip that explains it. */
    get notReadyReason() { return captureUnavailable ?? 'Pick a capture region first'; },

    // ── In-window Snip-style selector (reads) ──
    /** True while the Tyto window is acting as the in-window fullscreen selector. */
    get selecting() { return selecting; },
    /** `convertFileSrc` URL of the frozen-monitor PNG backdrop, or null when not selecting. */
    get selectFrozenUrl() { return selectFrozenUrl; },
    /** The active pick method within the frozen monitor. */
    get selectMethod() { return selectMethod; },
    /** Smart (foreground UI-element) hover rects for the frozen monitor, monitor-local CSS px. */
    get selectElements() { return selectElements; },
    /** Window hover rects for the frozen monitor, monitor-local CSS px (`id` = `win-<hwnd>`). */
    get selectWindows() { return selectWindows; },
    /** Id of the currently-frozen monitor (`mon-<hmonitor>`). */
    get selectMonitorId() { return selectMonitorId; },
    /** Display name of the currently-frozen monitor (for the monitor-switch button). */
    get selectMonitorName() { return selectMonitorName; },

    // ── In-window countdown + post-capture (reads) ──
    /** True while the in-window 3-2-1 countdown is running (over the frozen backdrop). */
    get countingDown() { return countingDown; },
    /** The current countdown digit — drives TytoCountdown. */
    get countdownValue() { return countdownValue; },
    /** Id of the freshly-produced capture the library should auto-open a preview for
     *  (read once, then cleared via [`clearAutoPreview`]). */
    get autoPreviewId() { return autoPreviewId; },
    /** Copy a screenshot to the OS clipboard right after it's saved. */
    get copyToClipboard() { return copyToClipboard; },

    // ── mutations ──
    setMode(m: CaptureMode) { mode = m; persistConfig(); },
    setTargetKind(k: TargetKind) { targetKind = k; persistConfig(); },
    selectMonitor(id: string) { selectedMonitorId = id; },
    selectWindow(id: string) { selectedWindowId = id; },
    setRegion(sel: RegionSelection | null) {
      region = sel;
      regionMask = null; // an externally-set region has no freehand mask
      if (sel) targetKind = 'region';
    },

    /** Entry point from the shell's method buttons — opens the in-window Snip-style
     *  selector in `initialMode` (delegates to [`enterSelection`]). */
    async openScreenRegion(initialMode: SelectMethod = 'rect') {
      return this.enterSelection(initialMode);
    },

    // ── In-window Snip-style selector (actions) ──

    /** Change the pick method while staying on the current frozen monitor (no re-freeze
     *  — the backdrop + enumerated rects are still valid for the same display). */
    setSelectMethod(m: SelectMethod) { selectMethod = m; },

    /** Enter the in-window fullscreen selector: freeze the active monitor (the current
     *  `selectedMonitorId`, else the primary), enumerate its smart + window rects, grow
     *  the Tyto window to cover it, and paint the frozen backdrop + toolbar. Needs the
     *  backend — a no-op (with an error surfaced) when it's down. */
    async enterSelection(method: SelectMethod) {
      if (!backendUp) { lastError = 'Recording backend not available'; return; }
      const start = selectedMonitorId && monitors.some((m) => m.id === selectedMonitorId)
        ? selectedMonitorId
        : null;
      const ok = await freezeMonitorForSelection(start);
      if (!ok) return;
      selectMethod = method;
      selecting = true;
    },

    /** Switch the frozen backdrop to the next monitor in `monitors` (wrap-around),
     *  re-freezing + re-enumerating and re-covering that display. Keeps the active
     *  method. No-op with fewer than 2 monitors. */
    async switchSelectionMonitor() {
      if (!selecting || monitors.length < 2) return;
      const cur = monitors.findIndex((m) => m.id === selectMonitorId);
      const next = monitors[(cur + 1) % monitors.length];
      await freezeMonitorForSelection(next.id);
    },

    /** Leave the selector: restore the full Tyto control panel and drop the frozen
     *  backdrop + enumerated rects. Safe to call when not selecting. */
    async exitSelection() {
      selecting = false;
      selectFrozenUrl = null;
      selectElements = [];
      selectWindows = [];
      try { await setTytoSelection(false, 0, 0, 0, 0); } catch { /* ignore */ }
    },

    /** Commit a drawn region (rect/free/smart): resolve the monitor-local CSS rect into
     *  a physical crop against the frozen monitor, set it as the region target (with the
     *  freehand mask when `points` is given), exit the selector, then capture. */
    async commitRegion(css: { x: number; y: number; width: number; height: number }, points: number[][] | null) {
      const monitorId = selectMonitorId;
      try {
        const sel = await beSelectRegion({
          monitor_id: monitorId,
          css: { x: css.x, y: css.y, w: css.width, h: css.height },
        });
        region = { css: sel.css, physical: sel.physical, scaleFactor: sel.scale_factor };
        // Freehand: convert the traced polygon (monitor-local CSS px) to PHYSICAL,
        // REGION-LOCAL px — physical = css * scaleFactor, then subtract the region's
        // physical top-left so the polygon is 0-based within the crop the BE masks.
        if (points && points.length > 2) {
          const s = sel.scale_factor;
          const ox = sel.physical.x;
          const oy = sel.physical.y;
          regionMask = points.map(([px, py]) => [Math.round(px * s) - ox, Math.round(py * s) - oy]);
        } else {
          regionMask = null;
        }
        targetKind = 'region';
        persistConfig();
      } catch (e) {
        lastError = String(e);
        await this.exitSelection();
        return;
      }
      // Record → keep covering the monitor and run the in-window countdown over the same
      // frozen backdrop (no flash), then hide + start. Screenshot → restore the full panel
      // first, so the grab's hide/show cycle acts on the panel, not the covering selector.
      if (mode === 'record') {
        await this.startRecordingFromCover();
      } else {
        await this.exitSelection();
        await this.takeScreenshot();
      }
    },

    /** Commit a whole-window pick: select the window target, then capture (record keeps the
     *  cover for the in-window countdown; screenshot restores the panel first). */
    async commitWindow(id: string) {
      selectedWindowId = id;
      targetKind = 'window';
      persistConfig();
      if (mode === 'record') {
        await this.startRecordingFromCover();
      } else {
        await this.exitSelection();
        await this.takeScreenshot();
      }
    },

    /** Commit a whole-monitor pick: select the monitor target, then capture (record keeps
     *  the cover for the in-window countdown; screenshot restores the panel first). */
    async commitMonitor(id: string) {
      selectedMonitorId = id;
      targetKind = 'monitor';
      persistConfig();
      if (mode === 'record') {
        await this.startRecordingFromCover();
      } else {
        await this.exitSelection();
        await this.takeScreenshot();
      }
    },

    /** Start a recording directly from the covering selector: the window is already over
     *  the monitor with the frozen backdrop up, so the in-window countdown runs over that
     *  same backdrop (no re-cover, no flash). Then Tyto hides and the engine starts. On a
     *  cancelled countdown (Esc) it restores the full panel without recording. */
    async startRecordingFromCover() {
      if (recording) return;
      lastError = null;
      // Backend down (mock): the selector never really covered — restore + mock-record.
      if (!backendUp) { await this.exitSelection(); await this.startRecording(); return; }

      if (countdownSecs > 0) {
        // Hand the covering surface from the selector to the countdown IN THE SAME TICK
        // (selecting→false + countingDown→true together) so the full panel never flashes.
        selecting = false;
        const completed = await runCountdownTimer();
        if (!completed) { countingDown = false; await this.exitSelection(); return; } // Esc → panel
        // countingDown stays true (TytoCountdown keeps covering) until the window is hidden.
      }
      // Hide the covering window BEFORE dropping the last covering flag, so TytoShell never
      // paints the full panel over the monitor for a frame; then start via the HUD.
      try { await getCurrentWindow().hide(); } catch { /* ignore */ }
      selecting = false;
      countingDown = false;
      selectFrozenUrl = null;
      selectElements = [];
      selectWindows = [];
      await beginBackendRecording();
    },

    toggleSystemAudio() { systemAudio = !systemAudio; persistConfig(); },
    setMic(id: string | null) { micId = id; persistConfig(); },
    setFps(v: Fps) { fps = v; persistConfig(); },
    setQuality(q: Quality) { quality = q; persistConfig(); },
    setRecordOutput(v: RecordOutput) { recordOutput = v; persistConfig(); },
    setFrameFormat(v: FrameFormat) { frameFormat = v; persistConfig(); },
    setFrameSampleFps(v: number) { frameSampleFps = Math.min(60, Math.max(1, Math.trunc(v))); persistConfig(); },
    setFrameMaxWidth(v: number) { frameMaxWidth = Math.max(0, Math.trunc(v)); persistConfig(); },
    setCountdownSecs(v: number) { countdownSecs = Math.max(0, Math.trunc(v)); persistConfig(); },
    setOutputDir(dir: string) { outputDir = dir; outputDirExplicit = true; persistConfig(); },
    setScreenshotFormat(f: ScreenshotFormat) { screenshotFormat = f; persistConfig(); },
    setCopyToClipboard(v: boolean) { copyToClipboard = v; persistConfig(); },
    /** Abort a running in-window countdown (Esc) — the waiting starter restores the panel. */
    cancelCountdown() { cancelCountdownFn?.(); },
    /** The library calls this after opening the auto-preview, so it fires once per capture. */
    clearAutoPreview() { autoPreviewId = null; },

    async startRecording() {
      if (recording || !isTargetReady()) return;
      lastError = null;
      if (backendUp) {
        // Optional on-screen 3-2-1 before capture begins (video only). Rendered IN this
        // window over a frozen backdrop (no separate window / white flash): cover the
        // target monitor, count down, then hide Tyto so it isn't in the shot.
        if (countdownSecs > 0) {
          const monId = targetKind === 'monitor' ? selectedMonitorId
            : (selectedMonitorId && monitors.some((m) => m.id === selectedMonitorId) ? selectedMonitorId : null);
          // Mark the countdown active BEFORE covering, so when coverMonitor reveals the
          // window it paints TytoCountdown over the backdrop — never the full panel
          // stretched to monitor size.
          countingDown = true;
          countdownValue = countdownSecs;
          const frame = await coverMonitor(monId);
          if (!frame) { countingDown = false; return; } // freeze failed — surfaced + restored
          const completed = await runCountdownTimer();
          if (!completed) { countingDown = false; await this.exitSelection(); return; } // Esc → panel
          // Hide BEFORE clearing countingDown so the full panel never flashes over the monitor.
          try { await getCurrentWindow().hide(); } catch { /* ignore */ }
          countingDown = false;
          selectFrozenUrl = null;
        }
        await beginBackendRecording();
        return;
      }
      // Mock fallback (backend down): local timer + synthetic capture.
      beSession = false;
      recording = true;
      elapsedMs = 0;
      startedAt = Date.now();
      timer = setInterval(() => { elapsedMs = Date.now() - startedAt; }, 200);
    },
    async stopRecording() {
      if (!recording) return;
      if (beSession) {
        recording = false;
        elapsedMs = 0;
        beSession = false;
        try { await beStop(); } catch (e) { lastError = String(e); }
        // Tear down the HUD + restore Tyto (also emits recording-stopped).
        try { await closeRecordingHud(); } catch { /* HUD may already be gone */ }
        await refreshCaptures(true);
        flashNewest();
        return;
      }
      stopTimer();
      const dur = Date.now() - startedAt;
      recording = false;
      elapsedMs = 0;
      pushCapture('record', dur);
    },
    async takeScreenshot() {
      if (!isTargetReady()) return;
      lastError = null;
      if (backendUp) {
        // Hide Tyto for a monitor/region grab so its own window isn't in the shot
        // (a window grab targets another window, so Tyto is never captured).
        const hideSelf = targetKind !== 'window';
        const win = getCurrentWindow();
        try {
          if (hideSelf) {
            await win.hide();
            await new Promise((r) => setTimeout(r, 160));
          }
          // Screenshots (only) carry the freehand mask: a masked region grab is punched
          // to the traced shape and forced to PNG. Recording stays mask-free (bounding box).
          await beScreenshot({
            ...currentArgs(),
            mask_points: targetKind === 'region' ? regionMask : null,
          });
          await refreshCaptures(true);
          flashNewest();
        } catch (e) {
          lastError = String(e);
          uiStore.showToast(`Screenshot failed: ${e}`, 'error');
        } finally {
          if (hideSelf) { try { await win.show(); } catch { /* ignore */ } }
        }
        return;
      }
      pushCapture('screenshot', null);
    },

    async removeCapture(id: string) {
      if (backendUp) {
        try { await beRemove(id); await refreshCaptures(true); } catch (e) { lastError = String(e); }
        return;
      }
      captures = captures.filter(c => c.id !== id);
    },
    async renameCapture(id: string, name: string) {
      const n = name.trim();
      if (!n) return;
      if (backendUp) {
        try { await beRename(id, n); await refreshCaptures(true); } catch (e) { lastError = String(e); }
        return;
      }
      captures = captures.map(c => (c.id === id ? { ...c, name: n } : c));
    },
    async clearCaptures() {
      if (backendUp) {
        try { await beClear(); await refreshCaptures(true); } catch (e) { lastError = String(e); }
        return;
      }
      captures = [];
    },
    /** Grab a live preview thumbnail of the current target (backend). Returns the
     *  temp PNG path, or null when the backend is down / the grab fails. */
    async capturePreview(): Promise<string | null> {
      if (!backendUp) return null;
      try {
        return await previewSource({
          target_kind: targetKind,
          source_id: targetKind === 'monitor' ? selectedMonitorId
            : targetKind === 'window' ? selectedWindowId
            : null,
          region: region ? { ...region.physical } : null,
        });
      } catch {
        return null;
      }
    },

    /** Load a saved frame sequence for playback: geometry, per-frame presentation
     *  times and the frames themselves as asset URLs. Null when the backend is down
     *  or the capture isn't a sequence. */
    async loadFrameSequence(id: string): Promise<FrameSequence | null> {
      if (!backendUp) return null;
      try {
        const seq: FrameSequenceWire = await readFrameSequence(id);
        return {
          width: seq.width,
          height: seq.height,
          durationMs: seq.duration_ms,
          sampleFps: seq.sample_fps,
          frames: seq.frames.map((p) => convertFileSrc(p)),
          times: seq.times,
        };
      } catch (e) {
        lastError = String(e);
        return null;
      }
    },

    /** Reveal a capture in the OS file manager (backend reverse-channel). */
    async revealCapture(id: string) {
      try { await beReveal(id); } catch (e) { lastError = String(e); }
    },
    /** Open a capture with the OS default handler. */
    async openCapture(id: string) {
      try { await beOpen(id); } catch (e) { lastError = String(e); }
    },
    /** Re-read the persisted settings and re-apply them (audio / fps / quality /
     *  countdown / output). Called on window refocus to re-sync a possibly-stale
     *  control (e.g. the system-audio toggle) — the Sitta "force refresh" pattern. */
    async reloadConfig() {
      if (!backendUp) return;
      try { const cfg = await getTytoConfig(); applySettings(cfg); } catch { /* keep current */ }
    },

    /** Reveal the output folder (where captures are saved) in the OS file manager. */
    async revealOutputFolder() {
      if (backendUp) {
        try { await beRevealOutput(); } catch (e) { lastError = String(e); }
      } else {
        uiStore.showToast(`Captures are saved to ${outputDir}`, 'info');
      }
    },

    /** Subscribe to tyto-be attach/detach and do an initial fetch. Called once by
     *  TytoWindow on mount. Idempotent. All backend calls are best-effort — a
     *  missing / stubbed backend leaves the mock data in place. */
    initBackend() {
      if (unlistenBackend) return;
      void listenTytoBackend(
        () => { backendUp = true; void refreshFromBackend(); },
        () => { backendUp = false; },
      ).then((off) => { unlistenBackend = off; });
      // Live elapsed from the engine's authoritative clock while recording.
      void listenRecordingProgress((p) => { if (recording && beSession) elapsedMs = p.elapsed_ms; });
      // Capture source lost mid-recording (monitor unplugged, resolution switch): the
      // HUD auto-stops + saves the partial file; here we just surface why.
      void listenRecordingError((e) => {
        if (!recording && !beSession) return;
        lastError = e.message;
        uiStore.showToast(e.message, 'warning');
      });
      // The OS answered the screen-recording permission. The shell asks when Tyto
      // opens and the dialog outlives our first source fetch, so a grant has to be
      // able to replace the "not available" panel without reopening the window.
      // Re-fetch whatever the answer was: the backend owns the wording, including the
      // way out, and a second copy of it here is how two messages start to disagree.
      void listen(TYTO_CAPTURE_PERMISSION, () => { void refreshFromBackend(); });
      // The HUD (or shell) stopped the recording — sync our UI + reload the library.
      void listen(TYTO_RECORDING_STOPPED, () => {
        if (!recording && !beSession) return;
        recording = false;
        beSession = false;
        elapsedMs = 0;
        void refreshCaptures(true).then(() => flashNewest());
      });
      // The backend may already be up (window re-summoned) — fetch eagerly.
      void refreshFromBackend();
    },
    /** Last backend error (recording/screenshot/library), or null. */
    get lastError() { return lastError; },

    /** Total bytes across every capture in the library. */
    get totalBytes() { return captures.reduce((sum, c) => sum + c.sizeBytes, 0); },
  };
}

export const recorderStore = createRecorderStore();

// ── Formatting helpers (shared by the components) ────────────────────────────

export function formatDuration(ms: number): string {
  const total = Math.floor(ms / 1000);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const p = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${p(m)}:${p(s)}` : `${m}:${p(s)}`;
}

export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const units = ['KB', 'MB', 'GB'];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i += 1; }
  return `${v.toFixed(v >= 100 ? 0 : 1)} ${units[i]}`;
}

export function formatAgo(ts: number): string {
  const sec = Math.floor((Date.now() - ts) / 1000);
  if (sec < 60) return 'just now';
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} min ago`;
  const hr = Math.floor(min / 60);
  if (hr < 24) return `${hr} h ago`;
  const day = Math.floor(hr / 24);
  return `${day} d ago`;
}

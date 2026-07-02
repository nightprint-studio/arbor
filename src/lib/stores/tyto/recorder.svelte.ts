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
  listCaptureSources, listAudioInputs, listCaptures,
  startRecording as beStart, stopRecording as beStop, takeScreenshot as beScreenshot,
  removeCapture as beRemove, renameCapture as beRename, clearCaptures as beClear,
  revealCapture as beReveal, openCapture as beOpen, revealOutput as beRevealOutput,
  selectRegion as beSelectRegion, freezeScreen, freezeVirtual, previewSource,
  enumerateUiElements, enumeratePickTargets,
  type CaptureWire, type StartRecordingArgs,
} from '$lib/ipc/tyto/recorder';
import { uiStore } from '$lib/stores/ui.svelte';
import { openRegionSelectorWindow, takeRegionResult } from '$lib/ipc/tyto/region-window';
import { openRecordingHud, closeRecordingHud, TYTO_RECORDING_STOPPED } from '$lib/ipc/tyto/hud-window';
import { openCountdownOverlay, takeCountdownDone, closeCountdownOverlay } from '$lib/ipc/tyto/countdown-window';

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
export type TargetKind = 'monitor' | 'window' | 'region';
export type Quality = 'high' | 'balanced' | 'compact';
export type Fps = 30 | 60;
export type ScreenshotFormat = 'png' | 'jpg' | 'webp';

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
  kind: CaptureMode;
  target: string;
  durationMs: number | null; // null for screenshots
  sizeBytes: number;
  createdAt: number;
  /** Hue (deg) for the synthetic thumbnail gradient — mock stand-in for a frame. */
  hue: number;
  /** Absolute file path on disk (empty in the mock). Used to show the real image /
   *  play the real video via `convertFileSrc`. */
  path: string;
}

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

const QUALITY_BITRATE: Record<Quality, number> = { high: 24000, balanced: 12000, compact: 6000 };

/** Poll the shell for the region-selection outcome until it's ready (safety cap far
 *  above any real selection). Reliable where a pushed event isn't: an outgoing invoke
 *  works even while the Tyto window is hidden, and the poll speeds up once the shell
 *  re-shows Tyto on confirm/cancel. */
async function pollRegionResult() {
  for (let i = 0; i < 3000; i++) {
    const r = await takeRegionResult();
    if (r) return r;
    await new Promise((res) => setTimeout(res, 200));
  }
  return null;
}

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

  let systemAudio = $state(true);
  let micId = $state<string | null>(MOCK_MICS[0].id);

  let fps = $state<Fps>(60);
  let quality = $state<Quality>('balanced');
  // Seconds of 3-2-1 countdown before a video recording starts (0 = off).
  let countdownSecs = $state(3);

  let outputDir = $state('C:\\Users\\user\\Videos\\Tyto');
  // Screenshot image format (still captures only; recordings use the container).
  let screenshotFormat = $state<ScreenshotFormat>('png');

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
  function flashNewest() {
    const newest = captures[0];
    captureFlashId = newest ? newest.id : null;
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

  function pushCapture(kind: CaptureMode, durationMs: number | null) {
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
    if (cfg.output.dir) outputDir = cfg.output.dir;
    const fmt = cfg.output.screenshot_format;
    if (fmt === 'png' || fmt === 'jpg' || fmt === 'webp') screenshotFormat = fmt;
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
      capture:  { fps, system_audio: systemAudio, mic_id: micId ?? '', countdown_secs: countdownSecs },
      encoding: { quality, bitrate_kbps: QUALITY_BITRATE[quality], codec: 'mp4' },
      output:   { dir: outputDir, filename_template: 'tyto_%Y%m%d_%H%M%S', screenshot_format: screenshotFormat },
    };
    void setTytoConfig(cfg).catch(() => {});
  }

  function mapCapture(c: CaptureWire): Capture {
    let h = 0;
    for (let i = 0; i < c.id.length; i++) h = (h * 31 + c.id.charCodeAt(i)) % 360;
    return {
      id: c.id, name: c.name, kind: c.kind as CaptureMode, target: c.target,
      durationMs: c.duration_ms, sizeBytes: c.size_bytes, createdAt: c.created_at, hue: h,
      path: c.path,
    };
  }

  /** Pull whatever tyto-be can serve. A successful config read proves the backend
   *  is reachable (covers an already-up backend whose up-event we missed). Each
   *  list only replaces the mock when the backend returns something. */
  async function refreshFromBackend() {
    try { const cfg = await getTytoConfig(); backendUp = true; hydrateFromConfig(cfg); } catch { return; }
    try {
      const s = await listCaptureSources();
      if (s.monitors.length) {
        monitors = s.monitors.map(m => ({ id: m.id, name: m.name, resolution: m.resolution, scale: m.scale, primary: m.primary }));
        if (!monitors.some(m => m.id === selectedMonitorId)) selectedMonitorId = monitors[0].id;
      }
      if (s.windows.length) {
        windows = s.windows.map(w => ({ id: w.id, title: w.title, app: w.app }));
        if (!windows.some(w => w.id === selectedWindowId)) selectedWindowId = windows[0].id;
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
    };
  }

  /** Run the optional 3-2-1 countdown before a video recording. Hides Tyto (so the
   *  user sees their real screen), opens the self-driven overlay window, and polls
   *  for its completion (a pull model — reliable while Tyto is hidden). Leaves Tyto
   *  hidden on success so the recording begins cleanly. Returns `false` (with the
   *  error surfaced + Tyto restored) if the overlay couldn't open. */
  async function runCountdown(): Promise<boolean> {
    const win = getCurrentWindow();
    try {
      await win.hide();
      await new Promise((r) => setTimeout(r, 150));
      await openCountdownOverlay(countdownSecs);
    } catch (e) {
      lastError = String(e);
      try { await win.show(); } catch { /* ignore */ }
      return false;
    }
    // Poll for the self-driven overlay to finish. Cap generously above the real
    // duration so a stuck overlay can't hang the start forever.
    const maxPolls = (countdownSecs + 4) * 20; // 50ms cadence
    for (let i = 0; i < maxPolls; i++) {
      let done = false;
      try { done = await takeCountdownDone(); } catch { done = true; }
      if (done) return true;
      await new Promise((r) => setTimeout(r, 50));
    }
    // Timed out — tear the overlay down and proceed rather than hang.
    try { await closeCountdownOverlay(); } catch { /* ignore */ }
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
    get countdownSecs() { return countdownSecs; },
    get bitrateKbps() { return QUALITY_BITRATE[quality]; },
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

    /** Thin alias kept for the region-family modes ('rect' | 'free' | 'smart') — the
     *  full on-screen selector lives in [`openCaptureSelector`]. */
    async openScreenRegion(initialMode: 'rect' | 'free' | 'smart' = 'rect') {
      return this.openCaptureSelector(initialMode);
    },

    /** Open the opaque on-screen capture selector in `mode`, then apply its outcome:
     *   • 'rect' | 'free' | 'smart' → freeze the current monitor + gather UI-element
     *     rects; the drawn rectangle resolves into a region target.
     *   • 'window' | 'display' → freeze the WHOLE virtual desktop + enumerate every
     *     window / monitor hover target; a click picks that window / monitor target.
     *  Needs the backend — a no-op (with an error surfaced) when it's down. */
    async openCaptureSelector(mode: 'rect' | 'free' | 'smart' | 'window' | 'display') {
      const pickTarget = mode === 'window' || mode === 'display';
      // rect/free/smart resolve into a region; set that eagerly so the source trigger
      // reflects the pending pick. window/display keep the current target until picked.
      if (!pickTarget) targetKind = 'region';
      if (!backendUp) { lastError = 'Recording backend not available'; return; }
      const win = getCurrentWindow();
      let frame;
      try {
        // Step Tyto aside BEFORE freezing so it isn't in the captured frame; give
        // the compositor a moment to actually hide it.
        await win.hide();
        await new Promise((r) => setTimeout(r, 150));
        if (pickTarget) {
          // Whole-desktop backdrop + every window/monitor hover target (virtual-desktop
          // CSS px). No smart UI-element rects in these modes.
          frame = await freezeVirtual();
          const { windows: winRects, monitors: monRects } = await enumeratePickTargets();
          await openRegionSelectorWindow({
            screenshotPath: frame.path,
            x: frame.x, y: frame.y, width: frame.width, height: frame.height,
            elements: [],
            windows: winRects,
            monitors: monRects,
            initialMode: mode,
          });
        } else {
          frame = await freezeScreen(null);
          // Snapshot the foreground app's UI element rects for the overlay's smart pick
          // (captured now, before the overlay covers the screen). Empty = smart disabled.
          const elements = await enumerateUiElements(frame.monitor_id).catch(() => []);
          await openRegionSelectorWindow({
            screenshotPath: frame.path,
            x: frame.x, y: frame.y, width: frame.width, height: frame.height,
            elements,
            windows: [],
            monitors: [],
            initialMode: mode,
          });
        }
      } catch (e) {
        // The backend couldn't freeze / open the overlay — surface it and make sure
        // Tyto is visible again.
        lastError = String(e);
        try { await win.show(); } catch { /* ignore */ }
        return;
      }
      // The overlay is up. Pull the outcome (reliable even while Tyto is hidden — no
      // dependency on a pushed event landing as the window is re-shown). A failure
      // resolving the rect here surfaces as an error, NOT the mock picker.
      try {
        const outcome = await pollRegionResult();
        if (!outcome || !outcome.confirmed) return;
        // Whole-window / whole-monitor pick: no rectangle, just switch the target.
        if (outcome.window_id) {
          this.selectWindow(outcome.window_id);
          this.setTargetKind('window');
          return;
        }
        if (outcome.monitor_id) {
          this.selectMonitor(outcome.monitor_id);
          this.setTargetKind('monitor');
          return;
        }
        // Region rectangle (rect/free/smart): resolve CSS → physical against the monitor.
        const sel = await beSelectRegion({
          monitor_id: frame.monitor_id,
          css: { x: outcome.x, y: outcome.y, w: outcome.width, h: outcome.height },
        });
        region = { css: sel.css, physical: sel.physical, scaleFactor: sel.scale_factor };
        // Freehand: convert the traced polygon (window-local CSS px) to PHYSICAL,
        // REGION-LOCAL px — physical = css * scaleFactor, then subtract the region's
        // physical top-left so the polygon is 0-based within the crop the BE masks.
        if (outcome.points && outcome.points.length > 2) {
          const s = sel.scale_factor;
          const ox = sel.physical.x;
          const oy = sel.physical.y;
          regionMask = outcome.points.map(([px, py]) => [
            Math.round(px * s) - ox,
            Math.round(py * s) - oy,
          ]);
        } else {
          regionMask = null;
        }
        targetKind = 'region';
      } catch (e) {
        lastError = String(e);
      }
    },
    toggleSystemAudio() { systemAudio = !systemAudio; persistConfig(); },
    setMic(id: string | null) { micId = id; persistConfig(); },
    setFps(v: Fps) { fps = v; persistConfig(); },
    setQuality(q: Quality) { quality = q; persistConfig(); },
    setCountdownSecs(v: number) { countdownSecs = Math.max(0, Math.trunc(v)); persistConfig(); },
    setOutputDir(dir: string) { outputDir = dir; persistConfig(); },
    setScreenshotFormat(f: ScreenshotFormat) { screenshotFormat = f; persistConfig(); },

    async startRecording() {
      if (recording || !isTargetReady()) return;
      lastError = null;
      if (backendUp) {
        // Optional on-screen 3-2-1 before capture begins (video only). Leaves Tyto
        // hidden on success so it isn't in the shot.
        if (countdownSecs > 0) {
          const ok = await runCountdown();
          if (!ok) return; // couldn't open the overlay — already surfaced + Tyto shown
        }
        try {
          await beStart(currentArgs());
          beSession = true;
          recording = true;
          elapsedMs = 0; // driven by the tyto://recording-progress event
          // Hand off to the on-screen HUD and hide Tyto so it's not in the capture.
          void openRecordingHud(targetLabel());
        } catch (e) {
          lastError = String(e);
          uiStore.showToast(`Couldn't start recording: ${e}`, 'error');
          // A countdown or the HUD may have hidden Tyto — make sure it's back.
          try { await getCurrentWindow().show(); } catch { /* ignore */ }
        }
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

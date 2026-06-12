/**
 * grove export/render status — drives the title-bar feedback for the offline WAV
 * bounce. The render runs as a background job, so `groveRender` resolves with a
 * job id long before the WAV is written; without this the user sees a silent
 * button and can't tell whether the export started, finished, or failed.
 *
 * Tracks the job via the global `arbor://job-done` event (broadcast to every
 * window). The listener is armed *before* the job id is known and buffers events,
 * so a fast render that finishes before the id arrives is never missed.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { transfersStore } from '$lib/feedback/stores/transfers.svelte';

/**
 * Default number of times the offline bounce repeats the arrangement's natural
 * loop period (`arrangementStore.loopCycles`). A `Pattern` has no intrinsic
 * length, so the render window is `loopCycles × loops`; one loop is the sane
 * default and the Export dialog lets the user override it. Single source of
 * truth shared by the footer estimate and the dialog's default.
 */
export const DEFAULT_RENDER_LOOPS = 1;

/** Render-size estimate inputs — mirrors the offline bounce parameters. */
export interface RenderEstimateInput {
  /** Total cycles rendered (`loopCycles × loops`). */
  cycles:     number;
  /** Live cycles-per-second from the transport. */
  cps:        number;
  /** Render tail (reverb/release flush) in seconds. */
  tailSecs:   number;
  /** Sample rate in Hz. */
  sampleRate: number;
  /** PCM sample format — drives bytes/sample (int24 → 3, float32 → 4). */
  bitDepth?:  string;
}

export interface RenderEstimate {
  /** Wall-clock duration of the WAV in seconds (0 when not estimable). */
  durationSecs: number;
  /** Stereo file size in bytes. */
  sizeBytes:    number;
}

/**
 * The single source of truth for "how long / how big is this export" — used by
 * both the footer status strip and the Export options dialog so their figures
 * never drift from each other (or from the actual WAV the backend writes).
 */
export function estimateRender(input: RenderEstimateInput): RenderEstimate {
  const { cycles, cps, tailSecs, sampleRate, bitDepth } = input;
  const bytesPerSample = bitDepth === 'float32' ? 4 : 3;
  const durationSecs =
    cycles > 0 && cps > 0 ? cycles / cps + tailSecs : 0;
  const sizeBytes = durationSecs * (sampleRate || 48_000) * bytesPerSample * 2;
  return { durationSecs, sizeBytes };
}

/** `m:ss` (seconds zero-padded). */
export function fmtRenderDuration(secs: number): string {
  const total = Math.round(secs);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}:${s.toString().padStart(2, '0')}`;
}

/** Bytes → human KB / MB (1 decimal for MB, integer for KB). */
export function fmtRenderSize(bytes: number): string {
  const mb = bytes / (1024 * 1024);
  if (mb >= 1) return `${mb.toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

export type RenderStatus = 'idle' | 'rendering' | 'done' | 'failed';

interface JobDone {
  job_id: string;
  success: boolean;
  error: string | null;
}

interface JobProgress {
  job_id: string;
  /** Whole-percent completion, 0..100. */
  pct: number;
}

/** Last path segment (forward- or back-slash). */
function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function createRenderStore() {
  let status = $state<RenderStatus>('idle');
  let file   = $state<string | null>(null);
  let error  = $state<string | null>(null);

  let unlisten: UnlistenFn | null = null;
  let unlistenProgress: UnlistenFn | null = null;
  let clearTimer: ReturnType<typeof setTimeout> | null = null;
  // The shared transfers-overlay entry id for the in-flight export (the output
  // path, unique per render); null when idle.
  let transferId: string | null = null;

  function disarm() {
    if (unlisten) { unlisten(); unlisten = null; }
    if (unlistenProgress) { unlistenProgress(); unlistenProgress = null; }
  }
  function settle(s: RenderStatus, err?: string) {
    disarm();
    status = s;
    error = err ?? null;
    // Mirror the terminal state into the shared transfers overlay.
    if (transferId) {
      if (s === 'done') transfersStore.finish(transferId);
      else if (s === 'failed') transfersStore.fail(transferId, err);
      transferId = null;
    }
    // Auto-clear the terminal badge so the button returns to its idle icon.
    if (clearTimer) clearTimeout(clearTimer);
    clearTimer = setTimeout(() => {
      if (status === 'done' || status === 'failed') status = 'idle';
    }, 5000);
  }

  return {
    get status() { return status; },
    get file()   { return file; },
    get error()  { return error; },
    get active() { return status === 'rendering'; },

    /**
     * Track a render: `promise` is the `groveRender` call (resolves to a job id),
     * `outPath` only labels the badge. Surfaces success/failure when the job ends.
     */
    async track(promise: Promise<string>, outPath: string) {
      disarm();
      if (clearTimer) { clearTimeout(clearTimer); clearTimer = null; }
      status = 'rendering';
      file = basename(outPath);
      error = null;
      // Surface the export in the shared transfers overlay, with a live percent
      // streamed from the backend render job (`arbor://job-progress`). Carries the
      // output path so the overlay can reveal the finished WAV in the explorer.
      transferId = outPath;
      transfersStore.start({
        id: outPath, kind: 'export', label: file, sublabel: 'Rendering…', progress: 0, path: outPath,
      });

      let jobId: string | null = null;
      const buffered: JobDone[] = [];
      let bufferedPct: number | null = null;
      unlisten = await listen<JobDone>('arbor://job-done', (e) => {
        if (jobId == null) { buffered.push(e.payload); return; }
        if (e.payload.job_id === jobId) {
          settle(e.payload.success ? 'done' : 'failed', e.payload.error ?? undefined);
        }
      });
      unlistenProgress = await listen<JobProgress>('arbor://job-progress', (e) => {
        if (jobId == null) { bufferedPct = e.payload.pct; return; }
        if (e.payload.job_id === jobId && transferId) {
          transfersStore.update(transferId, { progress: e.payload.pct });
        }
      });

      try {
        jobId = await promise;
      } catch (err) {
        settle('failed', err instanceof Error ? err.message : String(err));
        return;
      }
      // Drain anything that landed before the id was known.
      if (bufferedPct != null && transferId) transfersStore.update(transferId, { progress: bufferedPct });
      const hit = buffered.find((p) => p.job_id === jobId);
      if (hit) settle(hit.success ? 'done' : 'failed', hit.error ?? undefined);
    },
  };
}

export const renderStore = createRenderStore();

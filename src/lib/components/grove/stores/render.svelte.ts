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

export type RenderStatus = 'idle' | 'rendering' | 'done' | 'failed';

interface JobDone {
  job_id: string;
  success: boolean;
  error: string | null;
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
  let clearTimer: ReturnType<typeof setTimeout> | null = null;

  function disarm() {
    if (unlisten) { unlisten(); unlisten = null; }
  }
  function settle(s: RenderStatus, err?: string) {
    disarm();
    status = s;
    error = err ?? null;
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

      let jobId: string | null = null;
      const buffered: JobDone[] = [];
      unlisten = await listen<JobDone>('arbor://job-done', (e) => {
        if (jobId == null) { buffered.push(e.payload); return; }
        if (e.payload.job_id === jobId) {
          settle(e.payload.success ? 'done' : 'failed', e.payload.error ?? undefined);
        }
      });

      try {
        jobId = await promise;
      } catch (err) {
        settle('failed', err instanceof Error ? err.message : String(err));
        return;
      }
      // Drain anything that landed before the id was known.
      const hit = buffered.find((p) => p.job_id === jobId);
      if (hit) settle(hit.success ? 'done' : 'failed', hit.error ?? undefined);
    },
  };
}

export const renderStore = createRenderStore();

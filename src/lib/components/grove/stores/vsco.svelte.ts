/**
 * grove VSCO store — install status of the VSCO 2 sample bank + live
 * download/extract progress. The download is a background job (the Jobs overlay
 * tracks it); progress also streams here for an inline indicator in the sound
 * bank. Subscribe on mount, unlisten on teardown.
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  groveVscoStatus, groveVscoDownload, onGroveVscoProgress,
  type GroveVscoStatus, type GroveVscoProgress,
} from '$lib/ipc/grove';
import { cancelJob } from '$lib/ipc/job';

function createVscoStore() {
  let status   = $state<GroveVscoStatus | null>(null);
  let progress = $state<GroveVscoProgress | null>(null);
  let jobId    = $state<string | null>(null);

  return {
    get status()   { return status; },
    get progress() { return progress; },
    get installed()       { return status?.installed ?? false; },
    get instrumentCount() { return status?.instrument_count ?? 0; },
    get sizeBytes()       { return status?.size_bytes ?? 0; },
    /** True while a download/extract job is in flight. */
    get downloading()     { return progress !== null && jobId !== null; },

    /** Re-read the install status from disk. */
    async refresh() {
      try { status = await groveVscoStatus(); } catch { /* keep last */ }
    },

    /** Start the download+install job. Progress flows via the subscription. */
    async download() {
      progress = null;
      try { jobId = await groveVscoDownload(); } catch { jobId = null; }
    },

    /** Cancel the in-flight download/extract job (standard job cancellation). */
    async cancel() {
      const id = jobId;
      if (!id) return;
      try { await cancelJob(id); } catch { /* already gone */ }
      jobId = null;
      progress = null;
    },

    subscribe(): Promise<UnlistenFn> {
      return onGroveVscoProgress((p) => {
        progress = p;
        // A 100% extract is the last event of an install — refresh + clear.
        if (p.phase === 'extracting' && p.total > 0 && p.done >= p.total) {
          jobId = null;
          progress = null;
          void this.refresh();
        }
      });
    },
  };
}

export const vscoStore = createVscoStore();

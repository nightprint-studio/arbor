/**
 * grove sample-packs store — install status of every downloadable pack (VSCO 2,
 * Dirt-Samples, drum machines, …) plus live per-pack download/extract progress.
 *
 * Each download is a background job (the Jobs overlay tracks it); progress also
 * streams here, keyed by `pack_id`, for the inline indicators in the sound bank.
 * Subscribe once on mount, unlisten on teardown.
 */

import type { UnlistenFn } from '@tauri-apps/api/event';
import {
  grovePacks, grovePackDownload, onGrovePackProgress,
  type GrovePack, type GrovePackProgress,
} from '$lib/ipc/grove';
import { cancelJob } from '$lib/feedback/ipc/job';

function createPacksStore() {
  let packs    = $state<GrovePack[]>([]);
  // Keyed by pack id; a pack is "downloading" while it has a live job id.
  let progress = $state<Record<string, GrovePackProgress | null>>({});
  let jobIds   = $state<Record<string, string>>({});

  /** Drop a pack's job id (no longer in flight). */
  function clearJob(id: string) {
    const next = { ...jobIds };
    delete next[id];
    jobIds = next;
  }

  return {
    /** All known packs with their install status (display order). */
    get packs() { return packs; },
    /** Live progress for a pack (`null` when idle). */
    progressOf(id: string) { return progress[id] ?? null; },
    /** Whether a pack has a download/extract job in flight. */
    downloadingOf(id: string) { return jobIds[id] != null; },

    /** Re-read every pack's install status from disk. */
    async refresh() {
      try { packs = await grovePacks(); } catch { /* keep last */ }
    },

    /** Start the download+install job for `id`. Progress flows via the subscription. */
    async download(id: string) {
      progress = { ...progress, [id]: null };
      try {
        const job = await grovePackDownload(id);
        jobIds = { ...jobIds, [id]: job };
      } catch { /* leave idle */ }
    },

    /** Cancel a pack's in-flight job (standard job cancellation). */
    async cancel(id: string) {
      const job = jobIds[id];
      if (!job) return;
      try { await cancelJob(job); } catch { /* already gone */ }
      clearJob(id);
      progress = { ...progress, [id]: null };
    },

    subscribe(): Promise<UnlistenFn> {
      return onGrovePackProgress((p) => {
        progress = { ...progress, [p.pack_id]: p };
        // A 100% extract is the last event of an install — refresh + clear.
        if (p.phase === 'extracting' && p.total > 0 && p.done >= p.total) {
          clearJob(p.pack_id);
          progress = { ...progress, [p.pack_id]: null };
          void this.refresh();
        }
      });
    },
  };
}

export const packsStore = createPacksStore();

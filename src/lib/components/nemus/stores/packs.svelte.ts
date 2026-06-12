/**
 * nemus sample-packs store — install status of every downloadable pack (VSCO 2,
 * Dirt-Samples, drum machines, …) plus live per-pack download/extract progress.
 *
 * Each download is a background job (the Jobs overlay tracks it); progress also
 * streams here, keyed by `pack_id`, for the inline indicators in the sound bank.
 * Subscribe once on mount, unlisten on teardown.
 *
 * Two backend streams drive the store: `nemus:pack_progress` (live %/phase) and
 * the global `arbor://job-done` (terminal success/failure). The latter is the
 * authority on failure — a download that errors emits no final extract-progress
 * event, so without it a failed transfer would stay "active" forever.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import {
  nemusPacks, nemusPackDownload, nemusPackDelete, onNemusPackProgress,
  type NemusPack, type NemusPackProgress,
} from '$lib/ipc/nemus';
import { cancelJob } from '$lib/feedback/ipc/job';
import { transfersStore } from '$lib/feedback/stores/transfers.svelte';

/** Terminal job result (mirrors the `arbor://job-done` payload). */
interface JobDone {
  job_id: string;
  success: boolean;
  error: string | null;
}

function createPacksStore() {
  let packs    = $state<NemusPack[]>([]);
  // Keyed by pack id; a pack is "downloading" while it has a live job id.
  let progress = $state<Record<string, NemusPackProgress | null>>({});
  let jobIds   = $state<Record<string, string>>({});

  /** Drop a pack's job id (no longer in flight). */
  function clearJob(id: string) {
    const next = { ...jobIds };
    delete next[id];
    jobIds = next;
  }

  /** The pack id that owns `jobId`, if any is still in flight. */
  function packForJob(jobId: string): string | undefined {
    return Object.keys(jobIds).find((pid) => jobIds[pid] === jobId);
  }

  async function refreshPacks() {
    try { packs = await nemusPacks(); } catch { /* keep last */ }
  }

  /** Apply a terminal job result to its pack: clear the in-flight state and
   *  resolve the transfer (success → finish + refresh; failure → fail, leaving
   *  the card back on its Download button so the user can retry). Idempotent —
   *  a success that also gets the final extract-progress event is a no-op the
   *  second time (the job id is already cleared). */
  function settleJob(packId: string, done: JobDone) {
    clearJob(packId);
    progress = { ...progress, [packId]: null };
    if (done.success) {
      transfersStore.finish(packId);
      void refreshPacks();
    } else {
      transfersStore.fail(packId, done.error ?? 'Download failed');
    }
  }

  return {
    /** All known packs with their install status (display order). */
    get packs() { return packs; },
    /** Live progress for a pack (`null` when idle). */
    progressOf(id: string) { return progress[id] ?? null; },
    /** Whether a pack has a download/extract job in flight. */
    downloadingOf(id: string) { return jobIds[id] != null; },

    /** Re-read every pack's install status from disk. */
    refresh() { return refreshPacks(); },

    /** Start the download+install job for `id`. Progress flows via the
     *  subscription, mirrored into the shared transfers overlay. */
    async download(id: string) {
      progress = { ...progress, [id]: null };
      const pack = packs.find((p) => p.id === id);
      transfersStore.start({
        id, kind: 'download', label: pack?.name ?? id, sublabel: 'Starting…', progress: null,
        // The install folder, so the finished transfer can be revealed.
        path: pack?.path,
        cancel: () => { void this.cancel(id); },
      });
      try {
        const job = await nemusPackDownload(id);
        jobIds = { ...jobIds, [id]: job };
      } catch (e) {
        // A Tauri command rejection is the serialized `AppError` *string* (see
        // `serialize_str` in error.rs), never an `Error` instance — so surface
        // it verbatim instead of a useless generic, otherwise the real cause
        // (bad path, disk, server error…) is silently discarded.
        transfersStore.fail(id, e instanceof Error ? e.message : String(e));
      }
    },

    /** Cancel a pack's in-flight job (standard job cancellation). */
    async cancel(id: string) {
      const job = jobIds[id];
      if (!job) return;
      try { await cancelJob(job); } catch { /* already gone */ }
      clearJob(id);
      progress = { ...progress, [id]: null };
      transfersStore.cancelled(id);
    },

    /** Delete an installed pack from disk, then re-read the pack list (which
     *  drives the sound bank to drop the pack's voices). Throws on failure so
     *  the caller can surface it. */
    async remove(id: string) {
      await nemusPackDelete(id);
      await refreshPacks();
    },

    async subscribe(): Promise<UnlistenFn> {
      const unProgress = await onNemusPackProgress((p) => {
        // A terminal job-done may already have cleared this pack (failure /
        // backstop); ignore late progress for a pack no longer in flight.
        if (jobIds[p.pack_id] == null) return;
        progress = { ...progress, [p.pack_id]: p };
        transfersStore.update(p.pack_id, {
          progress: p.pct >= 0 ? p.pct : null,
          sublabel: p.phase === 'extracting' ? 'Extracting…' : 'Downloading…',
        });
        // NB: do NOT settle on a 100% extract event — the pack's `registry.toml`
        // and `install.json` are written *after* the last extract-progress event,
        // so a refresh here would read a still-empty/not-yet-installed pack (the
        // sound bank would stay stale until a manual refresh). The terminal
        // `arbor://job-done` (below) is the single authority: it fires only once
        // those files are on disk, so refreshing there is the reliable signal.
      });

      // Terminal job results — the authority on download failure (and a backstop
      // for success when no final extract-progress event lands). Jobs that don't
      // map to an in-flight pack download (renders, other features) are ignored.
      const unDone = await listen<JobDone>('arbor://job-done', (e) => {
        const packId = packForJob(e.payload.job_id);
        if (packId) settleJob(packId, e.payload);
      });

      return () => { unProgress(); unDone(); };
    },
  };
}

export const packsStore = createPacksStore();

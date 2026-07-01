/**
 * merula external-libraries store — the project's `[libraries]` (GitHub modules)
 * with their lock/sync state, plus the job-tracked **sync** that downloads them.
 *
 * `refresh(dir)` loads the declared libraries + whether each is synced (cache
 * present for the locked SHA). `sync()` starts the background fetch; completion is
 * detected via the shared `arbor://job-done` event (the backend tags the job id),
 * then the list refreshes and a toast fires. Window-local UI state.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { merulaLibraries, merulaSyncLibraries, type MerulaLibraryStatus } from '$lib/ipc/merula/merula';
import { transfersStore } from '$lib/feedback/stores/transfers.svelte';
import { cancelJob } from '$lib/feedback/ipc/job';

/** Stable transfers-overlay id for the sync (only one runs at a time). */
const SYNC_TRANSFER_ID = 'merula:library-sync';

function createLibrariesStore() {
  let items   = $state<MerulaLibraryStatus[]>([]);
  let syncing = $state(false);
  let projectDir: string | null = null;
  let syncJobId: string | null = null;

  /** Reload the declared libraries + sync state for `dir`. */
  async function refresh(dir: string) {
    projectDir = dir;
    try { items = await merulaLibraries(dir); } catch { items = []; }
  }

  /** Cancel the in-flight sync (the Downloads & Exports overlay Stop button).
   *  Clears the job mapping first so the late `job-done` is ignored. */
  async function cancel() {
    const job = syncJobId;
    syncing = false;
    syncJobId = null;
    if (job) { try { await cancelJob(job); } catch { /* already gone */ } }
    transfersStore.cancelled(SYNC_TRANSFER_ID);
  }

  /** Start a background sync (no-op without a project / while already syncing).
   *  Surfaced in the shared Downloads & Exports overlay with a Stop button —
   *  the sync has no single percent, so the bar is indeterminate. */
  async function sync() {
    if (!projectDir || syncing) return;
    syncing = true;
    transfersStore.start({
      id: SYNC_TRANSFER_ID, kind: 'download', label: 'Libraries', sublabel: 'Syncing…',
      progress: null, cancel: () => { void cancel(); },
    });
    try {
      syncJobId = await merulaSyncLibraries(projectDir);
    } catch {
      syncing = false;
      transfersStore.fail(SYNC_TRANSFER_ID, 'Could not start library sync');
    }
  }

  /** Listen for the sync job's completion (one subscription per window). */
  async function subscribe(): Promise<UnlistenFn> {
    return listen<{ job_id: string; success: boolean; error?: string | null }>(
      'arbor://job-done',
      (ev) => {
        if (!syncJobId || ev.payload.job_id !== syncJobId) return;
        syncing = false;
        syncJobId = null;
        if (ev.payload.success) {
          transfersStore.finish(SYNC_TRANSFER_ID);
          if (projectDir) void refresh(projectDir);
        } else {
          transfersStore.fail(SYNC_TRANSFER_ID, ev.payload.error ?? 'sync failed');
        }
      },
    );
  }

  return {
    get items()   { return items; },
    get syncing() { return syncing; },
    /** Declared libraries not yet synced (cache missing / stale lock). */
    get missing() { return items.filter((l) => !l.synced).length; },
    get count()   { return items.length; },
    refresh,
    sync,
    cancel,
    subscribe,
  };
}

export const librariesStore = createLibrariesStore();

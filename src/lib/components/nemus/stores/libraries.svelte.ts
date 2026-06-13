/**
 * nemus external-libraries store — the project's `[libraries]` (GitHub modules)
 * with their lock/sync state, plus the job-tracked **sync** that downloads them.
 *
 * `refresh(dir)` loads the declared libraries + whether each is synced (cache
 * present for the locked SHA). `sync()` starts the background fetch; completion is
 * detected via the shared `arbor://job-done` event (the backend tags the job id),
 * then the list refreshes and a toast fires. Window-local UI state.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { nemusLibraries, nemusSyncLibraries, type NemusLibraryStatus } from '$lib/ipc/nemus';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

function createLibrariesStore() {
  let items   = $state<NemusLibraryStatus[]>([]);
  let syncing = $state(false);
  let projectDir: string | null = null;
  let syncJobId: string | null = null;

  /** Reload the declared libraries + sync state for `dir`. */
  async function refresh(dir: string) {
    projectDir = dir;
    try { items = await nemusLibraries(dir); } catch { items = []; }
  }

  /** Start a background sync (no-op without a project / while already syncing). */
  async function sync() {
    if (!projectDir || syncing) return;
    syncing = true;
    try {
      syncJobId = await nemusSyncLibraries(projectDir);
      toastStore.show('Syncing libraries…', 'info');
    } catch {
      syncing = false;
      toastStore.show('Could not start library sync', 'error');
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
          toastStore.show('Libraries synced', 'success');
          if (projectDir) void refresh(projectDir);
        } else {
          toastStore.show(`Library sync failed: ${ev.payload.error ?? 'unknown error'}`, 'error');
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
    subscribe,
  };
}

export const librariesStore = createLibrariesStore();

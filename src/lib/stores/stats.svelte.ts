import type { RepoStats } from '$lib/types/git';
import { computeRepoStats } from '$lib/ipc/stats';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';

function createStatsStore() {
  let stats    = $state<RepoStats | null>(null);
  let loading  = $state(false);
  let error    = $state<string | null>(null);
  /** Tab ID for which the current stats (or in-flight request) belong. */
  let trackedTabId = $state<string | null>(null);

  // ── Tauri event listeners ─────────────────────────────────────────────────
  // Called once from AppShell (same pattern as jobsStore.setupListeners).

  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'arbor://repo-stats-ready',
        handler: (e: { payload: { tab_id: string; stats: RepoStats } }) => {
          const { tab_id, stats: s } = e.payload;
          if (tab_id !== trackedTabId) return;
          stats   = s;
          loading = false;
          error   = null;
        },
      },
      {
        event: 'arbor://repo-stats-error',
        handler: (e: { payload: { tab_id: string; error: string } }) => {
          if (e.payload.tab_id !== trackedTabId) return;
          error   = e.payload.error;
          loading = false;
        },
      },
    ]);
  }

  // ── Public API ────────────────────────────────────────────────────────────

  async function load(tabId: string, force = false) {
    // Skip if already loaded for this tab (unless force-refreshing).
    if (!force && trackedTabId === tabId && (stats !== null || loading)) return;

    trackedTabId = tabId;
    loading      = true;
    stats        = null;
    error        = null;

    try {
      // Fire-and-forget — result arrives via 'arbor://repo-stats-ready'.
      await computeRepoStats(tabId);
    } catch (e) {
      error   = String(e);
      loading = false;
    }
  }

  function clear() {
    stats        = null;
    loading      = false;
    error        = null;
    trackedTabId = null;
  }

  return {
    get stats()   { return stats;   },
    get loading() { return loading; },
    get error()   { return error;   },
    get trackedTabId() { return trackedTabId; },
    setupListeners,
    load,
    clear,
  };
}

export const statsStore = createStatsStore();

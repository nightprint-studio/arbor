import type { SyncConfig, SyncStatus } from '$lib/types/corvus/sync';
import {
  getSyncConfig,
  setSyncConfig,
  syncStatus,
  syncEnable,
  syncDisable,
  syncPushNow,
} from '$lib/ipc/corvus/sync';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';

// ---------------------------------------------------------------------------
// syncStore — GitHub-backed corvus settings sync.
//
// Holds the editable config + the live status (dirty flag, last push/pull).
// The backend driver pushes on its own cadence; this store just reflects state
// and drives the settings section + palette actions. `arbor://corvus-sync-*`
// events keep the status fresh after a background push or a pull.
// ---------------------------------------------------------------------------

function createSyncStore() {
  let config = $state<SyncConfig | null>(null);
  let status = $state<SyncStatus | null>(null);
  let loaded = $state(false);

  async function loadConfig() {
    try {
      config = await getSyncConfig();
      status = await syncStatus();
      loaded = true;
    } catch {
      // First-run / backend not ready yet — keep defaults, retry on next call.
    }
  }

  async function refreshStatus() {
    try {
      status = await syncStatus();
    } catch { /* best-effort */ }
  }

  /** Persist the editable knobs (interval, include toggles). */
  async function saveConfig(next: SyncConfig) {
    config = next;
    await setSyncConfig(next);
    await refreshStatus();
  }

  async function enable(provider: string, repoName: string | null) {
    status = await syncEnable(provider, repoName);
    config = await getSyncConfig();
    return status;
  }

  async function disable() {
    status = await syncDisable();
    config = await getSyncConfig();
  }

  async function pushNow() {
    status = await syncPushNow();
  }

  function setupListeners(): () => void {
    return setupTauriListeners([
      { event: 'arbor://corvus-sync-pushed', handler: () => { void refreshStatus(); } },
      { event: 'arbor://corvus-sync-pulled', handler: () => { void loadConfig(); } },
    ]);
  }

  return {
    get config() { return config; },
    get status() { return status; },
    get loaded() { return loaded; },
    loadConfig,
    refreshStatus,
    saveConfig,
    enable,
    disable,
    pushNow,
    setupListeners,
  };
}

export const syncStore = createSyncStore();

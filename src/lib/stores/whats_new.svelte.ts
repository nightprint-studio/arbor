import { getWhatsNewConfig, setWhatsNewConfig } from '$lib/ipc/config';
import { getAppInfo } from '$lib/ipc/app';

/**
 * "What's New" store.
 *
 *   - Loads the persisted `last_seen_version` plus the current app version
 *     on boot. Their relationship drives the auto-open trigger.
 *   - `shouldAutoOpen()` is true only when both values are known AND they
 *     differ AND the persisted value is non-null (fresh install ⇒ silent).
 *   - `acknowledge()` persists the current version so the next launch
 *     stays quiet. Called from the modal's dismiss/close.
 */
function createWhatsNewStore() {
  // Both start undefined → "not loaded yet" — the auto-open guard waits
  // for `loaded` before deciding. Defaults err on the silent side: a
  // briefly-failed IPC won't flash the modal at everyone.
  let lastSeen       = $state<string | null>(null);
  let currentVersion = $state<string>('');
  let loaded         = $state(false);
  let open           = $state(false);
  /** When `true`, the modal is being shown manually (Command Palette /
   *  About link) and should NOT bump `last_seen_version` on close — the
   *  user might want it to re-appear next launch if they haven't yet
   *  upgraded past it (rare, but cleaner semantics). */
  let manualOnly     = $state(false);

  async function loadConfig() {
    try {
      const [cfg, info] = await Promise.all([getWhatsNewConfig(), getAppInfo()]);
      lastSeen       = cfg.last_seen_version ?? null;
      currentVersion = info.version;
    } catch {
      lastSeen       = null;
      currentVersion = '';
    }
    loaded = true;
  }

  /** True when the modal should auto-open on app boot.
   *  Fresh install (`lastSeen === null`) → silently record the current
   *  version, no popup. Otherwise pop only when versions differ. */
  function shouldAutoOpen(): boolean {
    if (!loaded || !currentVersion) return false;
    if (lastSeen === null) return false;
    return lastSeen !== currentVersion;
  }

  function persistCurrent() {
    void setWhatsNewConfig({ last_seen_version: currentVersion || null }).catch(() => {});
  }

  return {
    get lastSeen()       { return lastSeen; },
    get currentVersion() { return currentVersion; },
    get loaded()         { return loaded; },
    get open()           { return open; },

    loadConfig,
    shouldAutoOpen,

    /** Auto-trigger entry point: show the modal AND mark this version as
     *  seen immediately, so a force-quit before dismissal doesn't keep
     *  re-popping it. */
    autoShow() {
      open       = true;
      manualOnly = false;
      persistCurrent();
    },

    /** Silent acknowledge — used by AppShell on fresh-install boot so the
     *  next launch (after a real upgrade) actually pops. */
    silentlyAcknowledge() {
      if (!currentVersion) return;
      lastSeen = currentVersion;
      persistCurrent();
    },

    /** Manual re-entry from Command Palette or About panel — does NOT
     *  bump `last_seen_version` on close. */
    showManual() {
      open       = true;
      manualOnly = true;
    },

    hide() {
      open = false;
      // Manual opens leave persistence alone — autoShow already wrote it.
      manualOnly = false;
    },
  };
}

export const whatsNewStore = createWhatsNewStore();

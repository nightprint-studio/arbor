/**
 * samply for the profiler: which copy is used, the path the user configured, and the install of the
 * official release.
 *
 * The path lives in the Bennu config on disk (`profiler_tools.samply_path`), never in the browser.
 * An install runs in the backend; this store only starts it and follows its events.
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  EVT_SAMPLY_INSTALL,
  samplyInstall,
  samplyLatest,
  samplyRemove,
  samplyStatus,
  samplyValidate,
  setProfilerTools,
  type SamplyInstallEvent,
  type SamplyLatest,
  type SamplyStatus,
} from '$lib/ipc/bennu/profiler';
import { bennuProfilerStore } from './profiler.svelte';

export type InstallPhase = 'lookup' | 'download' | 'verify' | 'extract';

const msg = (e: unknown) => (e instanceof Error ? e.message : String(e));

function createSamplyStore() {
  let status = $state<SamplyStatus | null>(null);
  /** Result of validating the path being edited. */
  let pathVersion = $state<string | null>(null);
  let pathError = $state<string | null>(null);
  let validating = $state(false);

  let latest = $state<SamplyLatest | null>(null);
  let latestError = $state<string | null>(null);
  let checking = $state(false);

  let installing = $state(false);
  let phase = $state<InstallPhase>('lookup');
  let done = $state(0);
  let total = $state<number | null>(null);
  let installError = $state<string | null>(null);

  let unlisten: UnlistenFn | null = null;

  async function attachEvents(): Promise<void> {
    if (unlisten) return;
    unlisten = await listen<SamplyInstallEvent>(EVT_SAMPLY_INSTALL, (e) => {
      const ev = e.payload;
      if (ev.kind === 'phase') phase = ev.phase;
      else if (ev.kind === 'progress') { done = ev.done; total = ev.total; }
      else if (ev.kind === 'done') {
        installing = false;
        latest = null;
        toastStore.show(`samply ${ev.version} installed`, 'success');
        void loadStatus();
        void bennuProfilerStore.loadDetect();
      } else if (ev.kind === 'error') {
        installing = false;
        installError = ev.error;
      }
    });
  }

  async function loadStatus(): Promise<void> {
    try {
      status = await samplyStatus();
      installing = status.installing;
    } catch (e) {
      status = null;
      pathError = msg(e);
    }
  }

  /** Validate and persist the configured path; an empty path clears it. */
  async function setPath(path: string): Promise<boolean> {
    const p = path.trim();
    pathError = null;
    pathVersion = null;
    if (p) {
      validating = true;
      try {
        pathVersion = await samplyValidate(p);
      } catch (e) {
        pathError = msg(e);
        return false;
      } finally {
        validating = false;
      }
    }
    try {
      await setProfilerTools({ samply_path: p });
    } catch (e) {
      pathError = msg(e);
      return false;
    }
    await loadStatus();
    void bennuProfilerStore.loadDetect();
    return true;
  }

  async function checkLatest(): Promise<void> {
    checking = true;
    latestError = null;
    installError = null;
    try {
      latest = await samplyLatest();
    } catch (e) {
      latest = null;
      latestError = msg(e);
    } finally {
      checking = false;
    }
  }

  async function install(allowUnverified: boolean): Promise<void> {
    if (!latest) return;
    await attachEvents();
    installError = null;
    installing = true;
    phase = 'lookup';
    done = 0;
    total = null;
    try {
      await samplyInstall(latest.asset_name, allowUnverified);
    } catch (e) {
      installing = false;
      installError = msg(e);
    }
  }

  async function remove(): Promise<void> {
    try {
      await samplyRemove();
      await loadStatus();
      void bennuProfilerStore.loadDetect();
    } catch (e) {
      toastStore.show(`Could not remove samply: ${msg(e)}`, 'error');
    }
  }

  function dismissInstall(): void {
    latest = null;
    latestError = null;
    installError = null;
  }

  return {
    get status() { return status; },
    get pathVersion() { return pathVersion; },
    get pathError() { return pathError; },
    get validating() { return validating; },
    get latest() { return latest; },
    get latestError() { return latestError; },
    get checking() { return checking; },
    get installing() { return installing; },
    get phase() { return phase; },
    get done() { return done; },
    get total() { return total; },
    get installError() { return installError; },
    loadStatus,
    setPath,
    checkLatest,
    install,
    remove,
    dismissInstall,
    attachEvents,
  };
}

export const bennuSamplyStore = createSamplyStore();

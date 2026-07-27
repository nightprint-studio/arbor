/**
 * Where workspace products open: their own window, or a tab in the shared
 * container. Backed by `launcher.window_mode` in `config.toml`, whose default
 * is per-OS (tabbed on macOS, separate windows elsewhere).
 *
 * Read through {@link windowModeStore.ensure} rather than at import time — most
 * windows never need it, and the first product launch can afford one config
 * read.
 */
import { getLauncherConfig, setLauncherWindowMode, type WindowMode } from '$lib/ipc/config';

function createWindowModeStore() {
  let mode = $state<WindowMode>('windows');
  let loaded = false;
  let inFlight: Promise<void> | null = null;

  async function load(): Promise<void> {
    try {
      mode = (await getLauncherConfig()).window_mode ?? 'windows';
    } catch {
      // Non-Tauri or an unreadable config: separate windows is the safe
      // fallback — it's the behaviour that works without a container.
    }
    loaded = true;
  }

  return {
    get mode() { return mode; },
    get tabbed() { return mode === 'tabbed'; },

    /** Seed from a `LauncherConfig` the caller already read, so the launcher's
     *  own config load doubles as this store's. */
    hydrate(next: WindowMode | undefined) {
      if (!next) return;
      mode = next;
      loaded = true;
    },

    /** Read the setting once. Concurrent callers share the same read. */
    async ensure(): Promise<WindowMode> {
      if (loaded) return mode;
      inFlight ??= load().finally(() => { inFlight = null; });
      await inFlight;
      return mode;
    },

    /** Persist a new mode. Applies to the next launch — open windows stay. */
    async set(next: WindowMode): Promise<void> {
      const prev = mode;
      mode = next;
      loaded = true;
      try {
        await setLauncherWindowMode(next);
      } catch (e) {
        mode = prev;
        throw e;
      }
    },
  };
}

export const windowModeStore = createWindowModeStore();

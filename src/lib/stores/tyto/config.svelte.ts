import { getTytoConfig, setTytoConfig } from '$lib/ipc/config';
import type { TytoConfig } from '$lib/types/config';

/**
 * Reactive mirror of the launcher-side Tyto config (the opt-in OS-global
 * shortcut). Loaded by TytoWindow on mount; setters persist through the
 * keep-shell `set_tyto_config` command, which also (un)registers the OS hotkey.
 *
 * Mirrors the File Explorer's config store shape. A failed save (invalid or
 * already-claimed accelerator) re-reads from disk so the UI reverts to the last
 * good value, and rethrows so the caller can toast.
 */
const DEFAULTS: TytoConfig = {
  global_shortcut: false,
  global_shortcut_accel: 'Ctrl+Shift+R',
};

function createTytoConfigStore() {
  let config = $state<TytoConfig>({ ...DEFAULTS });
  let loaded = $state(false);

  async function loadConfig() {
    try {
      config = await getTytoConfig();
    } catch {
      config = { ...DEFAULTS };
    }
    loaded = true;
  }

  /** Persist a patch; on failure revert to the on-disk value and rethrow. */
  async function save(patch: Partial<TytoConfig>) {
    const prev = { ...config };
    const next = { ...config, ...patch };
    config = next;
    try {
      await setTytoConfig(next);
    } catch (e) {
      config = prev;
      throw e;
    }
  }

  return {
    get config() { return config; },
    get loaded() { return loaded; },
    get globalShortcut() { return config.global_shortcut; },
    get accelerator() { return config.global_shortcut_accel; },
    loadConfig,
    setGlobalShortcut: (on: boolean) => save({ global_shortcut: on }),
    setAccelerator: (accel: string) => save({ global_shortcut_accel: accel }),
  };
}

export const tytoConfigStore = createTytoConfigStore();

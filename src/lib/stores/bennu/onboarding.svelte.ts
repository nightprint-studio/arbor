/**
 * Bennu's welcome-tour state.
 *
 * A store of its own rather than a product key in the shell's, because the tours are separate
 * things: Corvus keeps its completion flag in `corvus/config.toml` and this one lives in
 * `bennu/config.toml`, which is the existing shape for per-product settings and the reason
 * finishing the git tour does not silently suppress the editor's.
 *
 * The default before the config resolves is **completed**, deliberately. Assuming otherwise
 * would flash the tour for everybody on every launch for the frame or two before the read
 * lands; `BennuWindow` awaits `loadConfig()` and only then asks `shouldAutoOpen()`.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import {
  getBennuOnboarding, setBennuOnboarding, type BennuOnboarding,
} from '$lib/ipc/bennu/config';

/**
 * Bennu's onboarding schema version.
 *
 * Bump it when a release adds a step worth re-showing to somebody who has already been through
 * the tour — everyone whose stored version is lower gets it again on the next launch. A change
 * to the wording of an existing step is not that.
 */
export const CURRENT_BENNU_ONBOARDING_VERSION = 1;

function createBennuOnboardingStore() {
  let completed = $state(true);
  let version = $state(CURRENT_BENNU_ONBOARDING_VERSION);
  let loaded = $state(false);
  let open = $state(false);

  async function loadConfig(): Promise<void> {
    try {
      const cfg = await getBennuOnboarding();
      completed = !!cfg.completed;
      version = Number.isFinite(cfg.version) ? cfg.version : 0;
    } catch {
      // First run, or the backend is not attached yet. Treated as "never seen" rather than
      // swallowed: the alternative is a first-run tour that silently never happens because one
      // IPC call lost a race at boot.
      completed = false;
      version = 0;
    }
    loaded = true;
  }

  function persist() {
    const next: BennuOnboarding = { completed, version };
    void setBennuOnboarding(next).catch(() => {});
  }

  return {
    get loaded() { return loaded; },
    get open() { return open; },

    loadConfig,

    /** Whether to open it unasked on launch. */
    shouldAutoOpen(): boolean {
      if (!loaded) return false;
      if (!completed) return true;
      return version < CURRENT_BENNU_ONBOARDING_VERSION;
    },

    /** Open it on purpose — the Command Palette, the docs. */
    show() { open = true; },

    /** Close without recording anything — for a re-run the user dismisses. */
    hide() { open = false; },

    /** Finished or skipped. Idempotent; both footer buttons land here. */
    finish() {
      completed = true;
      version = CURRENT_BENNU_ONBOARDING_VERSION;
      open = false;
      persist();
    },
  };
}

export const bennuOnboardingStore = createBennuOnboardingStore();

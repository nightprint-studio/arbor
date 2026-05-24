import { getOnboardingConfig, setOnboardingConfig } from '$lib/ipc/config';
import type { OnboardingConfig } from '$lib/types/config';

/**
 * Current onboarding schema version. Bump this when a future release adds
 * meaningful new steps (or re-orders existing ones in a way that warrants
 * re-prompting). Users whose stored `version` is lower will see the modal
 * re-open on next launch; bumping by 1 each time keeps the diff readable
 * in the changelog.
 */
export const CURRENT_ONBOARDING_VERSION = 1;

function createOnboardingStore() {
  // Defaults are intentionally non-disruptive: until `loadConfig()` resolves
  // we assume the user HAS completed it, so first paint doesn't flash the
  // modal for everyone every time. AppShell calls `loadConfig()` early and
  // only then checks `shouldAutoOpen`.
  let completed = $state<boolean>(true);
  let version   = $state<number>(CURRENT_ONBOARDING_VERSION);
  let loaded    = $state(false);
  let open      = $state(false);

  async function loadConfig() {
    try {
      const cfg = await getOnboardingConfig();
      completed = !!cfg.completed;
      version   = Number.isFinite(cfg.version) ? cfg.version : 0;
    } catch {
      // First-run / backend not ready — assume not completed so the tour
      // shows up. Avoids the alternative of silently swallowing first-run
      // because the IPC briefly failed during boot.
      completed = false;
      version   = 0;
    }
    loaded = true;
  }

  /** True when the modal should auto-open on app boot. */
  function shouldAutoOpen(): boolean {
    if (!loaded) return false;
    if (!completed) return true;
    return version < CURRENT_ONBOARDING_VERSION;
  }

  function persist() {
    const next: OnboardingConfig = { completed, version };
    void setOnboardingConfig(next).catch(() => {});
  }

  return {
    get completed() { return completed; },
    get version()   { return version;   },
    get loaded()    { return loaded;    },
    get open()      { return open;      },

    loadConfig,
    shouldAutoOpen,

    /** Show the modal (manual re-entry via Command Palette / Docs). */
    show() { open = true; },

    /** Hide without changing completion state — for re-runs where the
     *  user dismisses without explicitly re-finishing. */
    hide() { open = false; },

    /** Mark the tour as completed at the current schema version and
     *  close the modal. Idempotent — safe to call from both "Finish"
     *  and "Skip all". */
    finish() {
      completed = true;
      version   = CURRENT_ONBOARDING_VERSION;
      open      = false;
      persist();
    },

    /** Reset state — used by the "Re-run onboarding" entry in
     *  Settings if/when we expose it. Not called by the auto-open
     *  flow. */
    reset() {
      completed = false;
      version   = 0;
      persist();
    },
  };
}

export const onboardingStore = createOnboardingStore();

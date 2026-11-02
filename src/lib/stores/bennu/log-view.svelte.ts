/**
 * The console's clean-view settings — what to show of the program's output, remembered.
 *
 * Settings live in Bennu's config on disk (`[log_view]`), never in the browser. They are global
 * rather than per project on purpose: "hide Hibernate's parameter binders" is a statement about
 * the reader, and the same reader reads every project's console.
 *
 * What *is* per project is which packages are "mine" — read from the sources, so a stack trace
 * can keep the user's frames and fold the rest without anyone typing a package name. See
 * `log_view.rs` in the backend.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { getLogView, setLogView, ownPackages } from '$lib/ipc/bennu/log-view';
import { DEFAULT_LOG_VIEW, type LogViewRule, type LogViewSettings } from '$lib/components/bennu/console/log-view';

/** How long a burst of edits waits before it is written — typing in a rule's value is one save, not twenty. */
const SAVE_DELAY_MS = 400;

function createLogViewStore() {
  let settings = $state<LogViewSettings>(structuredClone(DEFAULT_LOG_VIEW));
  let detected = $state<string[]>([]);
  let detectedRoot = '';
  let loading: Promise<void> | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;

  function schedule() {
    clearTimeout(timer);
    timer = setTimeout(() => {
      // Fire-and-forget: the change is already on screen, and a failed write is not worth a dialog.
      void setLogView($state.snapshot(settings) as LogViewSettings).catch(() => {});
    }, SAVE_DELAY_MS);
  }

  function patch(p: Partial<LogViewSettings>) {
    settings = { ...settings, ...p };
    schedule();
  }

  return {
    get settings() { return settings; },
    /** Detected from the project's sources. */
    get detected() { return detected; },
    /** Every package that counts as the user's code: detected, then added by hand. */
    get own(): string[] { return [...new Set([...detected, ...settings.extra_own_packages])]; },

    /** Read the saved settings, once. */
    load(): Promise<void> {
      loading ??= getLogView()
        .then((c) => { settings = { ...DEFAULT_LOG_VIEW, ...c, rules: c.rules ?? [], extra_own_packages: c.extra_own_packages ?? [] }; })
        .catch(() => {});
      return loading;
    },

    /** Read the project's own packages, once per root. */
    async detect(root: string | undefined): Promise<void> {
      if (!root || root === detectedRoot) return;
      detectedRoot = root;
      try { detected = await ownPackages(root); } catch { detected = []; }
    },

    patch,
    setClean(on: boolean) { patch({ clean: on }); },
    addRule(rule: LogViewRule) { patch({ rules: [...settings.rules, rule] }); },
    updateRule(i: number, p: Partial<LogViewRule>) {
      patch({ rules: settings.rules.map((r, k) => (k === i ? { ...r, ...p } : r)) });
    },
    removeRule(i: number) { patch({ rules: settings.rules.filter((_, k) => k !== i) }); },
  };
}

export const bennuLogViewStore = createLogViewStore();

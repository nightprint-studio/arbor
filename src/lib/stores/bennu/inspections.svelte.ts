/**
 * Bennu inspections store — the per-project `[inspections]` section and the catalog of checks.
 *
 * Writes as you go rather than through an Apply. A severity is one choice about one check, not a
 * document: the loop worth having is "turn it down, look at the file, turn it down further", and an
 * Apply button in the middle of that is a step that does nothing but wait.
 *
 * Every write bumps `revision`, which the editor's validation effect watches, so a check turned off
 * stops underlining on the next debounce instead of on the next time the file is opened.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import {
  getInspectionConfig as ipcGet,
  inspectionCatalog as ipcCatalog,
  setInspectionConfig as ipcSet,
  type CheckInfo,
  type CheckSeverity,
  type InspectionConfig,
} from '$lib/ipc/bennu/inspections';

function createBennuInspectionsStore() {
  let checks = $state<CheckInfo[]>([]);
  let config = $state<InspectionConfig>({ severity: {} });
  let loadedRoot = $state<string | null>(null);
  let loading = $state(false);
  let revision = $state(0);

  return {
    get checks() {
      return checks;
    },
    get loading() {
      return loading;
    },
    get revision() {
      return revision;
    },
    /** How many checks this project reports at something other than their default. */
    get changedCount() {
      return Object.keys(config.severity).length;
    },

    /** What a check reports at here — the project's answer, else the check's own default. */
    severityOf(check: CheckInfo): CheckSeverity {
      return config.severity[check.code] ?? check.defaultSeverity;
    },

    /** Whether this project said something about a check, as opposed to taking its default. */
    isConfigured(check: CheckInfo): boolean {
      return config.severity[check.code] !== undefined;
    },

    /** Read the catalog and the section for `root`. Re-read when the project changes. */
    async load(root: string): Promise<void> {
      if (loadedRoot === root || loading) return;
      loading = true;
      try {
        const [list, cfg] = await Promise.all([ipcCatalog(root), ipcGet(root)]);
        checks = list;
        config = cfg.severity ? cfg : { severity: {} };
        loadedRoot = root;
      } catch {
        checks = [];
        config = { severity: {} };
      } finally {
        loading = false;
      }
    },

    /**
     * Set one check's severity, or — when it is the check's own default — stop saying anything
     * about it.
     *
     * Removing rather than storing the default is what keeps the file readable: it lists the
     * decisions somebody made, and a default written out is indistinguishable from one until the
     * day the default changes and the project silently keeps the old one.
     */
    async setSeverity(check: CheckInfo, severity: CheckSeverity): Promise<void> {
      const root = loadedRoot;
      if (!root) return;
      const next = { ...config.severity };
      if (severity === check.defaultSeverity) delete next[check.code];
      else next[check.code] = severity;
      config = { severity: next };
      revision += 1;
      await ipcSet(root, config).catch(() => {});
    },

    /** Put every check back to its own default. */
    async resetAll(): Promise<void> {
      const root = loadedRoot;
      if (!root) return;
      config = { severity: {} };
      revision += 1;
      await ipcSet(root, config).catch(() => {});
    },
  };
}

export const bennuInspectionsStore = createBennuInspectionsStore();

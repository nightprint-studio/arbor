/**
 * Global Branches-sidebar behaviour knobs.
 *
 * Per-repo on/off lives in [[branch-grouping]] (one entry per tab); this
 * store holds host-wide preferences that apply to every repo that has
 * grouping enabled — currently the single "recursive `/` split" switch.
 *
 * Persistence is routed through the backend `branches` slot in
 * `~/.config/arbor/config.toml`; never localStorage.
 */

import { getBranchesConfig, setBranchesConfig } from '$lib/ipc/config';

function createBranchesConfigStore() {
  let groupingRecursive = $state<boolean>(true);
  let loaded            = $state(false);

  async function loadConfig() {
    try {
      const cfg = await getBranchesConfig();
      groupingRecursive = !!cfg.grouping_recursive;
      loaded = true;
    } catch {
      // First-run / backend not ready — keep defaults.
    }
  }

  function persist() {
    void setBranchesConfig({ grouping_recursive: groupingRecursive }).catch(() => {});
  }

  function setGroupingRecursive(on: boolean) {
    if (groupingRecursive === on) return;
    groupingRecursive = on;
    persist();
  }

  return {
    get groupingRecursive() { return groupingRecursive; },
    get loaded()            { return loaded; },
    loadConfig,
    setGroupingRecursive,
  };
}

export const branchesConfigStore = createBranchesConfigStore();

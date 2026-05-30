/**
 * Per-repo branch-grouping state (folder-tree view of `feature/x` paths).
 *
 * Each tab has its own `enabled` flag plus the set of collapsed group
 * paths — both persisted in `.arbor/config.toml` under
 * `branch_grouping = { enabled, collapsed_groups }`. The host-wide
 * "split recursively vs only on the first /" preference lives in the
 * separate [[branches-config]] store; this store covers what's repo-local.
 *
 * Cache shape mirrors [[local-tags]]: a record keyed by tab id so the
 * Sidebar can render the toggle / collapsed state synchronously, plus
 * async helpers that mutate the backing TOML file and refresh the cache.
 */

import { getBranchGrouping, setBranchGrouping } from '$lib/ipc/config';
import type { BranchGroupingConfig } from '$lib/types/config';

interface TabState {
  enabled:         boolean;
  collapsedGroups: Set<string>;
}

// First-paint default mirrors the backend `BranchGroupingConfig::default()` —
// grouping is on out of the box, so a tab whose config hasn't been loaded
// yet (or any consumer that asks about a tab with no entry yet) renders
// grouped instead of flashing flat for one tick. The async `load(tabId)`
// then either confirms it or flips it to whatever the user persisted.
const EMPTY: TabState = { enabled: true, collapsedGroups: new Set() };

const _cache = $state<Record<string, TabState>>({});

function _toState(cfg: BranchGroupingConfig): TabState {
  return {
    enabled:         !!cfg.enabled,
    collapsedGroups: new Set(cfg.collapsed_groups ?? []),
  };
}

async function _refresh(tabId: string): Promise<TabState> {
  try {
    const cfg = await getBranchGrouping(tabId);
    const state = _toState(cfg);
    _cache[tabId] = state;
    return state;
  } catch {
    if (!(tabId in _cache)) _cache[tabId] = { enabled: true, collapsedGroups: new Set() };
    return _cache[tabId];
  }
}

function _persist(tabId: string, state: TabState) {
  void setBranchGrouping(tabId, {
    enabled:          state.enabled,
    collapsed_groups: Array.from(state.collapsedGroups),
  }).catch(() => {});
}

export const branchGroupingStore = {
  /** Reactive read — true if grouping is enabled for this tab. */
  isEnabled(tabId: string | null | undefined): boolean {
    if (!tabId) return false;
    return (_cache[tabId] ?? EMPTY).enabled;
  },

  /** Reactive read — true if the group at `path` (joined by `/`) is collapsed. */
  isCollapsed(tabId: string | null | undefined, path: string): boolean {
    if (!tabId) return false;
    return (_cache[tabId] ?? EMPTY).collapsedGroups.has(path);
  },

  /** Reactive read — the current set of collapsed group paths for the tab. */
  collapsedGroups(tabId: string | null | undefined): Set<string> {
    if (!tabId) return new Set();
    return (_cache[tabId] ?? EMPTY).collapsedGroups;
  },

  /** Load (or refresh) the cache for a tab from `.arbor/config.toml`. */
  async load(tabId: string): Promise<void> {
    await _refresh(tabId);
  },

  /** Toggle the on/off switch for the tab and persist. */
  async setEnabled(tabId: string, enabled: boolean): Promise<void> {
    if (!tabId) return;
    const current = _cache[tabId] ?? { enabled: false, collapsedGroups: new Set<string>() };
    if (current.enabled === enabled) return;
    const next: TabState = { enabled, collapsedGroups: current.collapsedGroups };
    _cache[tabId] = next;
    _persist(tabId, next);
  },

  /** Flip the on/off switch for the tab. */
  async toggleEnabled(tabId: string): Promise<void> {
    if (!tabId) return;
    const current = _cache[tabId] ?? { enabled: false, collapsedGroups: new Set<string>() };
    await this.setEnabled(tabId, !current.enabled);
  },

  /** Bulk set the collapse flag for many group paths in one persist call.
   *  Used by the group context-menu's "Expand / Collapse all" actions so
   *  we don't burn one IPC + one disk write per descendant. */
  setCollapsedMany(tabId: string, paths: Iterable<string>, collapsed: boolean): void {
    if (!tabId) return;
    const current = _cache[tabId] ?? { enabled: true, collapsedGroups: new Set<string>() };
    const next = new Set(current.collapsedGroups);
    let changed = false;
    for (const p of paths) {
      if (collapsed) {
        if (!next.has(p)) { next.add(p); changed = true; }
      } else {
        if (next.has(p)) { next.delete(p); changed = true; }
      }
    }
    if (!changed) return;
    const updated: TabState = { enabled: current.enabled, collapsedGroups: next };
    _cache[tabId] = updated;
    _persist(tabId, updated);
  },

  /** Mark a group path collapsed (or expanded) and persist. */
  setCollapsed(tabId: string, path: string, collapsed: boolean): void {
    if (!tabId) return;
    const current = _cache[tabId] ?? { enabled: false, collapsedGroups: new Set<string>() };
    const has = current.collapsedGroups.has(path);
    if (collapsed && has) return;
    if (!collapsed && !has) return;
    const nextSet = new Set(current.collapsedGroups);
    if (collapsed) nextSet.add(path);
    else           nextSet.delete(path);
    const next: TabState = { enabled: current.enabled, collapsedGroups: nextSet };
    _cache[tabId] = next;
    _persist(tabId, next);
  },
};

import type { RepoInfo, RepoStatus } from '../types/git';

/** Reason a tab is in tombstone state — set when the underlying path is
 *  no longer a valid open-able git repo.  See `validate_repo_path` and
 *  `MissingRepoState` for the lifecycle. */
export type TombstoneReason = 'missing' | 'unreachable' | 'not_a_repo';

export interface RepoTabTombstone {
  reason:  TombstoneReason;
  /** Human-readable explanation from the backend validator. */
  message: string;
  /** ms since epoch when we last classified the path. */
  checkedAt: number;
}

export interface RepoTab {
  id: string;
  path: string;
  name: string;
  isLoading: boolean;
  error: string | null;
  info: RepoInfo | null;
  status: RepoStatus | null;
  currentBranch: string | null;
  /** True when the tab was swapped onto a non-main worktree of the underlying
   *  repo via the worktree switcher.  Drives the worktree icon shown next to
   *  the tab name; the user-facing tab name itself does not change on swap. */
  isLinkedWorktree?: boolean;
  /** Set when the tab's underlying path is unavailable.  When non-null the
   *  main panel renders `MissingRepoState` (Locate / Remove / Retry) and the
   *  TitleBar tab shows a warning glyph.  All other panels skip work for
   *  this tab — there is no opened repo handle. */
  tombstone?: RepoTabTombstone | null;
}

// ---------------------------------------------------------------------------
// Persistence — tab changes are pushed to a consumer (wired in AppShell to
// `workspacesStore.persistSnapshotNow`) which serialises them into the
// active workspace's snapshot file.  Kept as a callback slot to avoid a
// circular import between tabsStore and workspacesStore.
// ---------------------------------------------------------------------------

let persistHook: () => void = () => {};
export function setTabsPersistHook(fn: () => void): void { persistHook = fn; }
function savePersisted(_tabs: RepoTab[], _activeId: string | null): void {
  try { persistHook(); } catch { /* non-critical */ }
}

// ---------------------------------------------------------------------------
// Store
// ---------------------------------------------------------------------------

/** Progress emitted during the initial tab-restore phase so the boot
 *  splash can show "Loading repository 'foo' (2/5)" instead of an
 *  indeterminate spinner.  Cleared back to null once init is finished. */
export interface TabsInitProgress {
  current: number;
  total:   number;
  message: string;
}

function createTabsStore() {
  let tabs = $state<RepoTab[]>([]);
  let activeTabId = $state<string | null>(null);
  let isInitializing = $state(true);
  let initProgress = $state<TabsInitProgress | null>(null);

  const activeTab = $derived(tabs.find(t => t.id === activeTabId) ?? null);

  function addTab(info: RepoInfo): RepoTab {
    const tab: RepoTab = {
      id: info.tab_id,
      path: info.path,
      name: info.name,
      isLoading: false,
      error: null,
      info,
      status: null,
      currentBranch: info.current_branch ?? null,
    };
    tabs.push(tab);
    activeTabId = tab.id;
    savePersisted(tabs, activeTabId);
    return tab;
  }

  /** Like addTab but does NOT change activeTabId. Used during batch startup
   *  so that CommitGraph's $effect doesn't fire for every repo as it's added. */
  function addTabSilent(info: RepoInfo): RepoTab {
    const tab: RepoTab = {
      id: info.tab_id,
      path: info.path,
      name: info.name,
      isLoading: false,
      error: null,
      info,
      status: null,
      currentBranch: info.current_branch ?? null,
    };
    tabs.push(tab);
    return tab;
  }

  function beginInit() { isInitializing = true; }
  function endInit()   { isInitializing = false; initProgress = null; }
  function setInitProgress(p: TabsInitProgress | null) { initProgress = p; }

  function removeTab(id: string) {
    const idx = tabs.findIndex(t => t.id === id);
    if (idx === -1) return;
    tabs.splice(idx, 1);
    if (activeTabId === id) {
      activeTabId = tabs[Math.max(0, idx - 1)]?.id ?? null;
    }
    savePersisted(tabs, activeTabId);
  }

  /** Close a tab without auto-shifting activeTabId to a sibling.  Used by
   *  the workspace-switch flow where we deactivate the whole set up-front
   *  so intermediate closes don't briefly "activate" tabs that are about
   *  to be closed — that race was firing cascading graph loads against
   *  already-closed repo handles. */
  function removeTabSilent(id: string) {
    const idx = tabs.findIndex(t => t.id === id);
    if (idx === -1) return;
    tabs.splice(idx, 1);
    savePersisted(tabs, activeTabId);
  }

  function clearActive() {
    activeTabId = null;
    savePersisted(tabs, activeTabId);
  }

  function setActive(id: string) {
    activeTabId = id;
    savePersisted(tabs, activeTabId);
  }

  function updateTab(id: string, patch: Partial<RepoTab>) {
    const tab = tabs.find(t => t.id === id);
    if (tab) Object.assign(tab, patch);
  }

  /** Add a tombstone tab — the underlying path failed to open at restore
   *  time, so we surface the entry in the bar with a warning glyph and let
   *  the main panel offer Locate / Remove / Retry.  Skips the loading +
   *  status fields on purpose: nothing should attempt a git operation
   *  against a tombstoned tab. */
  function addTombstoneTab(args: {
    id:       string;
    path:     string;
    name:     string;
    reason:   TombstoneReason;
    message:  string;
    silent?:  boolean;
  }): RepoTab {
    const tab: RepoTab = {
      id:        args.id,
      path:      args.path,
      name:      args.name,
      isLoading: false,
      error:     null,
      info:      null,
      status:    null,
      currentBranch: null,
      tombstone: { reason: args.reason, message: args.message, checkedAt: Date.now() },
    };
    tabs.push(tab);
    if (!args.silent) {
      activeTabId = tab.id;
      savePersisted(tabs, activeTabId);
    }
    return tab;
  }

  function setTombstone(id: string, info: RepoTabTombstone | null): void {
    const tab = tabs.find(t => t.id === id);
    if (!tab) return;
    tab.tombstone = info;
  }

  function nextTab() {
    if (tabs.length <= 1) return;
    const idx = tabs.findIndex(t => t.id === activeTabId);
    activeTabId = tabs[(idx + 1) % tabs.length].id;
    savePersisted(tabs, activeTabId);
  }

  function prevTab() {
    if (tabs.length <= 1) return;
    const idx = tabs.findIndex(t => t.id === activeTabId);
    activeTabId = tabs[(idx - 1 + tabs.length) % tabs.length].id;
    savePersisted(tabs, activeTabId);
  }

  function reorderTabs(fromIndex: number, toIndex: number): void {
    if (fromIndex === toIndex) return;
    const clamped = Math.max(0, Math.min(toIndex, tabs.length - 1));
    const [moved] = tabs.splice(fromIndex, 1);
    tabs.splice(clamped, 0, moved);
    savePersisted(tabs, activeTabId);
  }

  /** Replace a tab in-place with the metadata of a different opened repo.
   *  Used by the worktree switcher: swapping which path the tab points at
   *  preserves the user's mental model of "1 project, 1 tab" instead of
   *  spawning a new tab per worktree path.
   *
   *  `opts.preserveName` keeps the original tab name (so swapping worktrees
   *  doesn't rename the tab in the bar).  `opts.isLinkedWorktree` toggles
   *  the worktree-indicator icon next to the name. */
  function replaceTab(
    oldId: string,
    info: RepoInfo,
    opts?: { preserveName?: boolean; isLinkedWorktree?: boolean },
  ): RepoTab {
    const idx = tabs.findIndex(t => t.id === oldId);
    const old = idx !== -1 ? tabs[idx] : null;
    const fresh: RepoTab = {
      id: info.tab_id,
      path: info.path,
      name: opts?.preserveName && old ? old.name : info.name,
      isLoading: false,
      error: null,
      info,
      status: null,
      currentBranch: info.current_branch ?? null,
      isLinkedWorktree: opts?.isLinkedWorktree ?? false,
    };
    if (idx === -1) {
      tabs.push(fresh);
    } else {
      tabs[idx] = fresh;
    }
    if (activeTabId === oldId || activeTabId === null) {
      activeTabId = fresh.id;
    }
    savePersisted(tabs, activeTabId);
    return fresh;
  }

  return {
    get tabs() { return tabs; },
    get activeTabId() { return activeTabId; },
    get activeTab() { return activeTab; },
    get isInitializing() { return isInitializing; },
    get initProgress() { return initProgress; },
    setInitProgress,
    addTab,
    addTabSilent,
    addTombstoneTab,
    setTombstone,
    removeTab,
    removeTabSilent,
    clearActive,
    setActive,
    updateTab,
    nextTab,
    prevTab,
    reorderTabs,
    replaceTab,
    beginInit,
    endInit,
  };
}

export const tabsStore = createTabsStore();

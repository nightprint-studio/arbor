import type { RepoStatus, BranchInfo, TagInfo, StashEntry, SubmoduleInfo } from '../types/git';

/** Per-tab "has conflicts" memory.  Only the 0→>0 transition fires a
 *  notification — clean in-progress operations (merge/rebase/cherry-pick
 *  with no conflicted files), routine status refreshes, and cross-tab
 *  switches into an already-conflicting state all stay silent. */
function describeOp(s: RepoStatus): string {
  if (s.is_rebasing)       return 'rebase';
  if (s.is_merging)        return 'merge';
  if (s.is_cherry_picking) return 'cherry-pick';
  if (s.is_reverting)      return 'revert';
  return 'operation';
}

function createRepoStore() {
  let status = $state<RepoStatus | null>(null);
  let localBranches = $state<BranchInfo[]>([]);
  let remoteBranches = $state<BranchInfo[]>([]);
  let stashes = $state<StashEntry[]>([]);
  let submodules = $state<SubmoduleInfo[]>([]);
  let tags = $state<TagInfo[]>([]);
  let nearestTag = $state<string | null>(null);
  let isRefreshing = $state(false);

  // Per-tab "did this tab have conflicts last time we saw it?" memory.
  // Used to fire the notification ONLY on the 0→>0 conflict-count
  // transition — never on cross-tab switches into an already-conflicting
  // state, never on plain in-progress merges without conflicts, never on
  // routine status refreshes.
  const hadConflictsByTab = new Map<string, boolean>();

  function notifyConflictsAppeared(s: RepoStatus) {
    const count = s.conflicted.length;
    const op    = describeOp(s);
    // Single persistent notification — the notifications store ALSO
    // renders it as a transient card at bottom-right for ~6 seconds, so
    // we don't double up with a separate toast.  The bell badge keeps it
    // visible after the transient fades.
    void import('./notifications.svelte').then(({ notificationsStore }) => {
      notificationsStore.add(
        'Merge conflicts to resolve',
        `${count} file${count === 1 ? '' : 's'} need${count === 1 ? 's' : ''} resolution${op !== 'operation' ? ` (${op} in progress)` : ''}. Open the Stage area to fix them.`,
        'warning',
      );
    });
  }

  function setStatus(s: RepoStatus | null) {
    status = s;
    if (!s) return;
    // Attribute the alert to whichever tab is currently active.  In
    // Arbor `repoStore.status` always reflects the active tab's state,
    // so this proxy is safe enough — background refreshes for the
    // active tab still pass through this guard correctly.
    void import('./tabs.svelte').then(({ tabsStore }) => {
      const tabId = tabsStore.activeTabId;
      if (!tabId) return;
      const nextHas = s.conflicted.length > 0;
      const prevHas = hadConflictsByTab.get(tabId) ?? false;
      hadConflictsByTab.set(tabId, nextHas);
      // Fire only on the moment conflicts actually appear on this tab.
      // Same state, cross-tab switch, or plain in-progress ops without
      // conflicts all stay silent.
      if (nextHas && !prevHas) {
        notifyConflictsAppeared(s);
      }
    });
  }

  /** Drop a tab's remembered conflict state — call when the tab is
   *  closed so the map doesn't grow unboundedly and so reopening the
   *  same path under a new tab id starts fresh. */
  function forgetTab(tabId: string) {
    hadConflictsByTab.delete(tabId);
  }

  function setLocalBranches(b: BranchInfo[]) { localBranches = b; }
  function setRemoteBranches(b: BranchInfo[]) { remoteBranches = b; }
  function setStashes(s: StashEntry[]) { stashes = s; }
  function setSubmodules(s: SubmoduleInfo[]) { submodules = s; }
  function setTags(t: TagInfo[]) { tags = t; }
  function setNearestTag(t: string | null) { nearestTag = t; }
  function setRefreshing(v: boolean) { isRefreshing = v; }

  function clear() {
    status = null;
    localBranches = [];
    remoteBranches = [];
    stashes = [];
    submodules = [];
    tags = [];
    nearestTag = null;
    isRefreshing = false;
    hadConflictsByTab.clear();
  }

  return {
    get status() { return status; },
    get localBranches() { return localBranches; },
    get remoteBranches() { return remoteBranches; },
    get stashes() { return stashes; },
    get submodules() { return submodules; },
    get tags() { return tags; },
    get nearestTag() { return nearestTag; },
    get isRefreshing() { return isRefreshing; },
    setStatus,
    setLocalBranches,
    setRemoteBranches,
    setStashes,
    setSubmodules,
    setTags,
    setNearestTag,
    setRefreshing,
    clear,
    forgetTab,
  };
}

export const repoStore = createRepoStore();

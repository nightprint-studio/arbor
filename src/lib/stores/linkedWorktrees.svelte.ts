import type { WorktreeLink, SyncSummary } from '$lib/types/linkedWorktree';
import { listWorktreeLinks } from '$lib/ipc/linkedWorktree';
import { setupTauriListeners } from '$lib/utils/tauri-listeners';
import { coalesceLatest } from '$lib/utils/coalesce';

// ---------------------------------------------------------------------------
// linkedWorktreesStore — runtime state for cross-project worktree sync.
//
// `latestSummary` is set whenever an `arbor://worktree-link-sync-done` event
// arrives.  The badge in CommitGraph reads it; the manager modal reads it too
// to surface a banner immediately after a sync.
// ---------------------------------------------------------------------------

function createLinkedWorktreesStore() {
  let links         = $state<WorktreeLink[]>([]);
  let loading       = $state(false);
  let lastError     = $state<string | null>(null);
  let latestSummary = $state<SyncSummary | null>(null);
  let isSyncing     = $state<Set<string>>(new Set());

  async function load() {
    loading = true;
    lastError = null;
    try {
      links = await listWorktreeLinks();
    } catch (e) {
      lastError = `${e}`;
    } finally {
      loading = false;
    }
  }

  function linkForRepo(repoId: string | null | undefined): WorktreeLink | null {
    if (!repoId) return null;
    return links.find(l => l.members.some(m => m.repo_id === repoId)) ?? null;
  }

  function isInAnyLink(repoId: string): boolean {
    return links.some(l => l.members.some(m => m.repo_id === repoId));
  }

  /** Compute the expected branch for a member given the link's last sync target. */
  function expectedBranchFor(link: WorktreeLink, repoId: string): string | null {
    const t = link.last_sync_target;
    if (!t) return null;
    if (t.initiator_repo_id === repoId) return t.branch;
    const group = link.alias_groups.find(g =>
      g.members.some(e => e.repo_id === t.initiator_repo_id && e.branch === t.branch)
    );
    if (group) {
      const entry = group.members.find(e => e.repo_id === repoId);
      if (entry) return entry.branch;
    }
    return t.branch;
  }

  // A workspace-wide reload (`load()`) is idempotent — running it once with
  // the freshest state is enough.  Coalesce so back-to-back link-changed
  // events fire one reload per frame instead of N.
  const reloadCoalesced = coalesceLatest<void>(() => { void load(); });

  function setupListeners(): () => void {
    return setupTauriListeners([
      {
        event: 'arbor://worktree-links-changed',
        handler: () => { reloadCoalesced(); },
      },
      {
        event: 'arbor://worktree-link-sync-started',
        handler: (e: { payload: { link_id: string } }) => {
          isSyncing = new Set([...isSyncing, e.payload.link_id]);
        },
      },
      {
        event: 'arbor://worktree-link-sync-done',
        handler: (e: { payload: SyncSummary }) => {
          latestSummary = e.payload;
          const next = new Set(isSyncing);
          next.delete(e.payload.link_id);
          isSyncing = next;
          void load();
        },
      },
    ]);
  }

  function dismissSummary() { latestSummary = null; }

  return {
    get links()         { return links; },
    get loading()       { return loading; },
    get lastError()     { return lastError; },
    get latestSummary() { return latestSummary; },
    get isSyncing()     { return isSyncing; },
    load,
    linkForRepo,
    isInAnyLink,
    expectedBranchFor,
    setupListeners,
    dismissSummary,
  };
}

export const linkedWorktreesStore = createLinkedWorktreesStore();

<script lang="ts">
  import { GitBranch, Globe, Archive, Tag, RefreshCw, GitCommitHorizontal, FileDiff, Layers, Trash2, Copy, GitMerge, AlertTriangle, Search as SearchIcon, Upload } from 'lucide-svelte';
  import { pushBranch } from '$lib/ipc/remote';
  import { localTagTracker } from '$lib/stores/local-tags.svelte';
  import RepoActions from './RepoActions.svelte';
  import BranchTree from './BranchTree.svelte';
  import StashList from './StashList.svelte';
  import SubmoduleList from './SubmoduleList.svelte';
  import WorktreeList from './WorktreeList.svelte';
  import BisectSessionList from './BisectSessionList.svelte';
  import BranchCleanupModal from './BranchCleanupModal.svelte';
  import BranchRenameModal from './BranchRenameModal.svelte';
  import DeleteTagModal from './DeleteTagModal.svelte';
  import type { BranchInfo } from '$lib/types/git';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { repoStore } from '$lib/stores/repo.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { bisectStore } from '$lib/stores/bisect.svelte';
  import { deleteTag } from '$lib/ipc/branch';
  import { getStatus } from '$lib/ipc/stage';
  import { listSubmodules } from '$lib/ipc/submodule';
  import { cacheStore } from '$lib/stores/cache.svelte';
  import { graphStore } from '$lib/stores/graph.svelte';
  import { worktreeStore } from '$lib/stores/worktree.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import SidebarSection from '$lib/components/shared/ui/SidebarSection.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';
  import { tooltip } from '$lib/actions/tooltip';

  const tab    = $derived(tabsStore.activeTab);
  const status = $derived(repoStore.status);

  const stagedCount    = $derived(status?.staged.length ?? 0);
  const unstagedCount  = $derived((status?.unstaged.length ?? 0) + (status?.untracked.length ?? 0));
  const changeCount    = $derived(stagedCount + unstagedCount);
  const isMerging      = $derived(status?.is_merging ?? false);
  const conflictCount  = $derived(status?.conflicted.length ?? 0);

  let expanded = $state({ locals: false, remotes: false, stashes: false, submodules: false, tags: false, workspaces: false, bisect: false });
  let refreshing = $state(false);
  let cleanupOpen = $state(false);
  let cleanupTab  = $state<'local' | 'remote'>('local');
  let renameBranch = $state<BranchInfo | null>(null);

  // Tag context menu — items list rebuilt per-open so we can hide
  // "Push to origin" / "Delete on origin" for tags that are still local-only.
  type TagCtx = { x: number; y: number; name: string };
  let tagCtxMenu = $state<TagCtx | null>(null);

  // Pending tag-delete confirmation (modal driven). null = no confirmation
  // currently displayed.
  type PendingTagDelete = { name: string; scope: 'local' | 'remote' };
  let pendingTagDelete = $state<PendingTagDelete | null>(null);

  function buildTagMenu(name: string): MenuItem[] {
    const isLocal = !!tab && localTagTracker.isLocal(tab.id, name);
    const items: MenuItem[] = [
      { id: 'copy', label: 'Copy value', icon: Copy, iconColor: 'var(--text-muted)' },
    ];
    if (isLocal) {
      items.push({ id: 'push', label: 'Push to origin', icon: Upload, iconColor: 'var(--accent)' });
    }
    items.push({ id: 'sep1', label: '', separator: true });
    items.push({ id: 'delete-local',  label: 'Elimina localmente',     icon: Trash2, danger: true });
    if (!isLocal) {
      items.push({ id: 'delete-remote', label: 'Elimina locale + origin', icon: Trash2, danger: true });
    }
    return items;
  }

  $effect(() => {
    graphStore.refreshTick; // re-run on auto-fetch so remote branches update
    if (tab) loadSidebarData(tab.id);
    else { repoStore.clear(); worktreeStore.clear(); }
  });

  $effect(() => {
    if (tab) bisectStore.loadSessions(tab.id);
  });

  async function loadSidebarData(tabId: string) {
    try {
      // Branches, stashes, tags, submodules go through the cache.
      // Status is always fetched live (not cached — changes constantly).
      const [sidebar, status] = await Promise.all([
        cacheStore.loadSidebarData(tabId),
        getStatus(tabId),
      ]);
      repoStore.setLocalBranches(sidebar.localBranches);
      repoStore.setRemoteBranches(sidebar.remoteBranches);
      repoStore.setStashes(sidebar.stashes);
      repoStore.setStatus(status);
      repoStore.setSubmodules(sidebar.submodules);
      repoStore.setTags(sidebar.tags);
      repoStore.setNearestTag(sidebar.nearestTag);
      tabsStore.updateTab(tabId, { status });
      // Worktrees loaded independently (not cached — quick git CLI call)
      worktreeStore.load(tabId);
      // Local-only tag set: refresh from .arbor/config.toml (best-effort —
      // failures here, e.g. on an old backend build that doesn't have the
      // command yet, must NOT break the sidebar update above).
      localTagTracker.load(tabId).catch(() => {});
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
  }

  /** Sort tags: semver-aware descending (v1.2.3 style), fallback lexicographic. */
  function sortedTags() {
    return [...repoStore.tags].sort((a, b) => {
      const parse = (s: string) => {
        const m = s.replace(/^[^\d]*/, '').match(/^(\d+)(?:\.(\d+))?(?:\.(\d+))?/);
        if (!m) return null;
        return [+m[1], +(m[2] ?? 0), +(m[3] ?? 0)] as [number, number, number];
      };
      const av = parse(a.name), bv = parse(b.name);
      if (av && bv) {
        for (let i = 0; i < 3; i++) {
          if (bv[i] !== av[i]) return bv[i] - av[i]; // descending
        }
        return 0;
      }
      return b.name.localeCompare(a.name);
    });
  }

  async function handleTagCtxSelect(id: string) {
    if (!tagCtxMenu || !tab) return;
    const { name } = tagCtxMenu;
    tagCtxMenu = null;
    if (id === 'copy') {
      try {
        await navigator.clipboard.writeText(name);
        uiStore.showToast(`Copied "${name}"`, 'info');
      } catch { uiStore.showToast('Copy failed', 'error'); }
      return;
    }
    if (id === 'push') {
      try {
        await pushBranch(tab.id, 'origin', `refs/tags/${name}`);
        await localTagTracker.markPushed(tab.id, name).catch(() => {});
        uiStore.showToast(`Pushato il tag "${name}" su origin`, 'success');
      } catch (err) {
        uiStore.showToast(`Push tag fallito: ${err}`, 'error');
      }
      return;
    }
    if (id === 'delete-local') {
      pendingTagDelete = { name, scope: 'local' };
      return;
    }
    if (id === 'delete-remote') {
      pendingTagDelete = { name, scope: 'remote' };
    }
  }

  async function executeTagDelete() {
    if (!tab || !pendingTagDelete) return;
    const { name, scope } = pendingTagDelete;
    pendingTagDelete = null;

    if (scope === 'local') {
      try {
        await deleteTag(tab.id, name);
        await localTagTracker.markPushed(tab.id, name).catch(() => {});
        uiStore.showToast(`Tag "${name}" eliminato in locale`, 'success');
        await loadSidebarData(tab.id);
        graphStore.refresh();
      } catch (err) { uiStore.showToast(`${err}`, 'error'); }
      return;
    }

    // scope === 'remote' → push delete refspec, then drop the local ref.
    try {
      await pushBranch(tab.id, 'origin', `:refs/tags/${name}`);
    } catch (err) {
      uiStore.showToast(`Delete su origin fallito: ${err}`, 'error');
      return;
    }
    try {
      await deleteTag(tab.id, name);
      await localTagTracker.markPushed(tab.id, name).catch(() => {});
      uiStore.showToast(`Tag "${name}" eliminato in locale e su origin`, 'success');
      await loadSidebarData(tab.id);
      graphStore.refresh();
    } catch (err) {
      uiStore.showToast(`Origin pulito ma delete locale fallito: ${err}`, 'warning');
    }
  }

  async function handleRefresh() {
    if (!tab || refreshing) return;
    refreshing = true;
    try {
      await loadSidebarData(tab.id);
    } finally {
      refreshing = false;
    }
  }
</script>

<PanelShell title="Branches & Stashes">
  {#snippet icon()}<GitBranch size={14} />{/snippet}
  {#snippet actions()}
    <button
      class="ps-btn"
      onclick={handleRefresh}
      disabled={refreshing}
      use:tooltip={'Refresh branches and stashes'}
    >
      <RefreshCw size={11} class={refreshing ? 'spin' : ''} />
    </button>
  {/snippet}

  <div class="sections">

    <!-- ── Conflict banner: any unmerged entries in the index, with or
         without an active merge. Index-only conflicts (e.g. leftover from
         an aborted merge / reverted operation) still need attention. ── -->
    {#if tab && conflictCount > 0}
      <div class="changes-banner merge-banner">
        <div class="changes-info">
          <AlertTriangle size={13} class="merge-conflict-icon" />
          <span class="changes-summary merge-conflict-text">
            {conflictCount} file in conflitto{isMerging ? '' : ' (unmerged)'}
          </span>
        </div>
        <button
          class="commit-cta merge-resolve-cta"
          onclick={() => uiStore.openMergeModal()}
          use:tooltip={'Apri la risoluzione guidata dei conflitti'}
        >
          <GitMerge size={12} />
          Risolvi conflitti…
        </button>
      </div>

    <!-- ── Working tree changes (normal, no merge conflict) ── -->
    {:else if tab && (changeCount > 0 || (isMerging && conflictCount === 0))}
      <div class="changes-banner" class:merge-in-progress={isMerging}>
        <div class="changes-info">
          {#if isMerging}
            <GitMerge size={13} class="merge-icon" />
            <span class="changes-summary merge-text">Merge in corso</span>
          {:else}
            <FileDiff size={13} class="changes-icon" />
            <span class="changes-summary">
              {#if unstagedCount > 0 && stagedCount > 0}
                {unstagedCount} unstaged, {stagedCount} staged
              {:else if unstagedCount > 0}
                {unstagedCount} change{unstagedCount !== 1 ? 's' : ''} to stage
              {:else}
                {stagedCount} file{stagedCount !== 1 ? 's' : ''} staged
              {/if}
            </span>
          {/if}
        </div>
        {#if isMerging}
          <button
            class="commit-cta merge-resolve-cta"
            onclick={() => uiStore.openMergeModal()}
            use:tooltip={'Completa o annulla il merge'}
          >
            <GitMerge size={12} />
            Completa merge…
          </button>
        {:else}
          <button
            class="commit-cta"
            onclick={() => uiStore.toggleBottomSection('stage')}
            use:tooltip={tooltipForAction('Open Stage & Commit area', 'stage_view')}
          >
            <GitCommitHorizontal size={12} />
            {uiStore.activeBottomSection === 'stage' ? 'Close Stage Area' : 'Stage & Commit…'}
          </button>
        {/if}
      </div>
    {/if}

    <!-- Local branches -->
    <SidebarSection
      label="Local Branches"
      iconColor="var(--graph-lane-0)"
      badge={repoStore.localBranches.length || null}
      badgeColor="var(--graph-lane-0)"
      bind:expanded={expanded.locals}
    >
      {#snippet icon()}<GitBranch size={13} />{/snippet}
      {#snippet actions()}
        <button
          class="cleanup-btn"
          use:tooltip={{ content: 'Branch cleanup', description: 'Delete merged local branches' }}
          onclick={() => { cleanupTab = 'local'; cleanupOpen = true; }}
        >
          <Trash2 size={11} />
        </button>
      {/snippet}
      <BranchTree
        branches={repoStore.localBranches}
        type="local"
        onRename={(b) => renameBranch = b}
        onCreateBranch={(b) => window.dispatchEvent(new CustomEvent('arbor:new-branch-from', { detail: { oid: b.head_oid } }))}
      />
    </SidebarSection>

    <!-- Remote branches -->
    <SidebarSection
      label="Remotes"
      iconColor="var(--graph-lane-1)"
      badge={repoStore.remoteBranches.length || null}
      badgeColor="var(--graph-lane-1)"
      bind:expanded={expanded.remotes}
    >
      {#snippet icon()}<Globe size={13} />{/snippet}
      {#snippet actions()}
        <button
          class="cleanup-btn"
          use:tooltip={{ content: 'Branch cleanup', description: 'Delete merged remote branches' }}
          onclick={() => { cleanupTab = 'remote'; cleanupOpen = true; }}
        >
          <Trash2 size={11} />
        </button>
      {/snippet}
      <BranchTree branches={repoStore.remoteBranches} type="remote" />
    </SidebarSection>

    <!-- Stashes -->
    <SidebarSection
      label="Stashes"
      iconColor="var(--color-stash)"
      badge={repoStore.stashes.length || null}
      badgeVariant="stash"
      bind:expanded={expanded.stashes}
    >
      {#snippet icon()}<Archive size={13} />{/snippet}
      <StashList stashes={repoStore.stashes} onRefresh={() => tab && loadSidebarData(tab.id)} />
    </SidebarSection>

    <!-- Tags -->
    <SidebarSection
      label="Tags"
      iconColor="var(--color-tag)"
      badge={repoStore.tags.length || null}
      badgeVariant="tag"
      bind:expanded={expanded.tags}
    >
      {#snippet icon()}<Tag size={13} />{/snippet}
      {#if repoStore.tags.length === 0}
        <EmptyState message="No tags" />
      {:else}
        {@const tags = sortedTags()}
        {#each tags as tag (tag.name)}
          <div
            class="tag-item"
            class:nearest={tag.name === repoStore.nearestTag}
            role="button"
            tabindex="0"
            use:tooltip={{
              content: tag.name,
              description: tag.message
                ? `${tag.message.trim()}\nClick to locate · Right-click for options`
                : 'Click to locate · Right-click for options',
            }}
            onclick={() => graphStore.scrollToBranch(tag.name)}
            onkeydown={(e) => e.key === 'Enter' && graphStore.scrollToBranch(tag.name)}
            oncontextmenu={(e) => { e.preventDefault(); e.stopPropagation(); tagCtxMenu = { x: e.clientX, y: e.clientY, name: tag.name }; }}
          >
            <span class="tag-icon-small"><Tag size={11} /></span>
            <span class="tag-name truncate">{tag.name}</span>
            {#if tag.name === repoStore.nearestTag}
              <span class="nearest-pill" use:tooltip={'Nearest ancestor tag from HEAD'}>HEAD</span>
            {/if}
            {#if tab && localTagTracker.isLocal(tab.id, tag.name)}
              <span class="local-only-badge" use:tooltip={{ content: 'Tag locale', description: 'Non ancora pushato su origin' }}>local</span>
            {/if}
            {#if tag.message}
              <span class="annotated-dot" use:tooltip={'Annotated tag'}>A</span>
            {/if}
          </div>
        {/each}
      {/if}
    </SidebarSection>

    <!-- Bisect Sessions (only shown when sessions exist) -->
    {#if tab && bisectStore.sessions.length > 0}
      <SidebarSection
        label="Bisect Sessions"
        iconColor="var(--color-bisect)"
        badge={bisectStore.sessions.length}
        badgeColor="var(--color-bisect)"
        bind:expanded={expanded.bisect}
      >
        {#snippet icon()}<SearchIcon size={13} />{/snippet}
        <BisectSessionList
          tabId={tab.id}
          onResume={() => { /* graph refresh handled by bisectStore.resume */ }}
        />
      </SidebarSection>
    {/if}

    <!-- Submodules (only shown when the repo has submodules) -->
    {#if repoStore.submodules.length > 0}
      {@const needsAttention = repoStore.submodules.filter(s => !s.is_initialized || s.is_dirty || s.behind > 0).length}
      <SidebarSection
        label="Submodules"
        iconColor="var(--color-submodule)"
        badge={needsAttention > 0 ? needsAttention : repoStore.submodules.length}
        badgeVariant={needsAttention > 0 ? 'stash' : 'default'}
        badgeTitle={needsAttention > 0 ? `${needsAttention} submodule${needsAttention !== 1 ? 's' : ''} need attention` : undefined}
        bind:expanded={expanded.submodules}
      >
        {#snippet icon()}<Layers size={13} />{/snippet}
        <SubmoduleList
          submodules={repoStore.submodules}
          onRefresh={() => tab && loadSidebarData(tab.id)}
        />
      </SidebarSection>
    {/if}

    <!-- Worktrees -->
    {#if tab}
      <WorktreeList bind:expanded={expanded.workspaces} />
    {/if}

  </div>

  {#if cleanupOpen}
    <BranchCleanupModal
      initialTab={cleanupTab}
      onClose={() => cleanupOpen = false}
      onRefresh={() => tab && loadSidebarData(tab.id)}
    />
  {/if}

  {#if renameBranch}
    <BranchRenameModal
      branch={renameBranch}
      onClose={() => renameBranch = null}
      onRenamed={() => tab && loadSidebarData(tab.id)}
    />
  {/if}

  {#if tagCtxMenu}
    <ContextMenu
      x={tagCtxMenu.x}
      y={tagCtxMenu.y}
      items={buildTagMenu(tagCtxMenu.name)}
      onSelect={handleTagCtxSelect}
      onClose={() => tagCtxMenu = null}
    />
  {/if}

  {#if pendingTagDelete}
    <DeleteTagModal
      tagName={pendingTagDelete.name}
      scope={pendingTagDelete.scope}
      onConfirm={executeTagDelete}
      onCancel={() => pendingTagDelete = null}
    />
  {/if}

</PanelShell>

<style>
  /* ── Section list — scrolling handled by parent PanelShell ── */
  .sections {
    padding: 4px 0 4px;
  }

  /* ── Tag list items ── */
  .tag-item {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 3px 8px 3px 4px;
    cursor: pointer;
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-secondary);
    transition: background var(--transition-fast), color var(--transition-fast);
    overflow: hidden;
    outline: none;
    min-height: 22px;
  }
  .tag-item:hover { background: rgba(255,255,255,0.05); color: var(--text-primary); }
  .tag-item:focus-visible { outline: 1px solid var(--border-focus); outline-offset: -1px; }
  .tag-item.nearest {
    color: var(--text-primary);
    background: color-mix(in srgb, var(--color-tag) 10%, transparent);
    font-weight: 500;
  }
  .tag-item.nearest:hover { background: color-mix(in srgb, var(--color-tag) 17%, transparent); }

  .tag-icon-small {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    color: var(--color-tag);
  }
  .tag-name { flex: 1; min-width: 0; }

  .nearest-pill {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.4px;
    color: var(--color-tag);
    background: color-mix(in srgb, var(--color-tag) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-tag) 35%, transparent);
    padding: 0 4px;
    border-radius: 999px;
    flex-shrink: 0;
  }

  .annotated-dot {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.3px;
    color: var(--text-muted);
    background: rgba(255,255,255,0.07);
    border: 1px solid rgba(255,255,255,0.12);
    padding: 0 4px;
    border-radius: 999px;
    flex-shrink: 0;
    line-height: 14px;
  }

  /* Same look as the branch local-only badge for visual consistency. */
  .local-only-badge {
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.3px;
    color: var(--color-tag);
    background: color-mix(in srgb, var(--color-tag) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-tag) 30%, transparent);
    padding: 0 4px;
    border-radius: 999px;
    flex-shrink: 0;
    line-height: 14px;
    text-transform: uppercase;
  }

  /* ── Branch cleanup button (hover-reveal handled by SidebarSection's
        .section-actions wrapper) ── */
  .cleanup-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--text-disabled);
    border-radius: var(--radius-sm);
    cursor: pointer;
    padding: 3px 6px;
    transition: color var(--transition-fast), background var(--transition-fast);
    flex-shrink: 0;
    height: 100%;
  }
  .cleanup-btn:hover { color: var(--error, #c75450); background: rgba(199,84,80,0.12); }

  :global(.spin) { animation: spin 0.9s linear infinite; }
  @keyframes spin { from { transform: rotate(0deg); } to { transform: rotate(360deg); } }

  /* ── Working tree changes banner — card style ── */
  .changes-banner {
    margin: 6px 8px;
    padding: 9px 10px;
    background: var(--bg-elevated);
    border: 1px solid rgba(77, 120, 204, 0.28);
    border-left: 3px solid var(--accent);
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: 8px;
    animation: fadeIn 150ms ease;
  }

  @keyframes fadeIn {
    from { opacity: 0; transform: translateY(-3px); }
    to   { opacity: 1; transform: translateY(0); }
  }

  .changes-info {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-secondary);
  }

  :global(.changes-icon) {
    color: var(--accent);
    flex-shrink: 0;
  }

  .changes-summary {
    font-size: 11px;
    color: var(--text-secondary);
    flex: 1;
  }

  .commit-cta {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    padding: 5px 10px;
    background: var(--accent);
    color: var(--text-on-accent);
    border: none;
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: background var(--transition-fast), opacity var(--transition-fast);
    letter-spacing: 0.2px;
  }
  .commit-cta:hover { background: var(--accent-hover, #3b5fc0); }
  .commit-cta:active { opacity: 0.85; }

  /* ── Merge conflict banner ── */
  .merge-banner {
    border-color: rgba(226, 163, 53, 0.4);
    border-left-color: var(--warning);
    background: rgba(226, 163, 53, 0.06);
  }
  .merge-in-progress {
    border-color: rgba(77, 120, 204, 0.28);
    border-left-color: var(--accent);
  }
  :global(.merge-conflict-icon) { color: var(--warning); flex-shrink: 0; }
  :global(.merge-icon)          { color: var(--accent);  flex-shrink: 0; }
  .merge-conflict-text { color: var(--warning); font-weight: 600; }
  .merge-text          { color: var(--accent); font-weight: 500; }

  .merge-resolve-cta {
    background: var(--warning);
    color: var(--text-on-accent);
  }
  .merge-resolve-cta:hover { background: color-mix(in srgb, var(--warning) 80%, white); }
</style>

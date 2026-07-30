<script lang="ts">
  import { FolderOpen, Clock, ChevronRight, Download, FolderGit2, Search, AlertTriangle, Trash2, FolderSearch } from 'lucide-svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { workspacesStore } from '$lib/stores/corvus/workspaces.svelte';
  import { workspaceColorVar } from '$lib/types/corvus/workspace';
  import Monogram from './ui/Monogram.svelte';
  import { openRepo as ipcOpenRepo } from '$lib/ipc/corvus/graph';
  import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
  import {
    validateRepoPaths, validateRepoPath, removeRecentRepo, relocateRepo,
    type RepoPathStatus,
  } from '$lib/ipc/corvus/missing';
  import FileExplorerModal from '../sitta/FileExplorerModal.svelte';
  import Contribution    from '../corvus/Contribution.svelte';
  import PluginIcon      from '$lib/components/plugins/PluginIcon.svelte';
  import ArborLogo       from './internal/ArborLogo.svelte';
  import Kbd             from './internal/Kbd.svelte';
  import { tooltipForAction } from '$lib/utils/shortcut';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    onOpen,
    onOpenPath,
    onClone,
  }: {
    onOpen: () => void;
    onOpenPath: (path: string) => void;
    onClone: () => void;
  } = $props();

  const recentRepos  = $derived(uiStore.recentRepos.slice(0, 6));
  const activeWs     = $derived(workspacesStore.active);

  // ── Recent / workspace repo missing-path tracking ─────────────────────────
  // We classify both lists in parallel on mount and again after any locate /
  // remove action so the "missing" badges reflect on-disk state without the
  // user having to refresh.
  let recentStatus  = $state<Map<string, RepoPathStatus>>(new Map());
  let wsStatus      = $state<Map<string, RepoPathStatus>>(new Map());

  /** Picker / busy state for the "Locate folder…" entry-action. */
  let locating = $state<{ repoId?: string; recentPath?: string } | null>(null);

  async function refreshStatuses(): Promise<void> {
    const recents = uiStore.recentRepos.slice(0, 6);
    const wsPaths = wsReposFull.map(r => r.path);

    if (recents.length > 0) {
      try {
        const v = await validateRepoPaths(recents);
        const m = new Map<string, RepoPathStatus>();
        recents.forEach((p, i) => m.set(p, v[i]?.status ?? 'ok'));
        recentStatus = m;
      } catch { /* leave previous state */ }
    } else { recentStatus = new Map(); }

    if (wsPaths.length > 0) {
      try {
        const v = await validateRepoPaths(wsPaths);
        const m = new Map<string, RepoPathStatus>();
        wsReposFull.forEach((r, i) => m.set(r.id, v[i]?.status ?? 'ok'));
        wsStatus = m;
      } catch { /* leave previous state */ }
    } else { wsStatus = new Map(); }
  }

  // Re-run whenever the lists change.  Tracking by length+first path is
  // enough — the list is short and reactive enough that path-by-path diffing
  // would be churn for no benefit.
  $effect(() => {
    // touch reactive deps
    void uiStore.recentRepos.length;
    void wsReposFull.length;
    void refreshStatuses();
  });

  function reasonHint(s: RepoPathStatus | undefined): string {
    switch (s) {
      case 'missing':     return 'Folder no longer exists on disk';
      case 'unreachable': return 'Drive or network share unavailable';
      case 'not_a_repo':  return 'Folder is no longer a git repository';
      default:            return '';
    }
  }

  async function removeRecent(path: string, ev: MouseEvent): Promise<void> {
    ev.stopPropagation();
    try { await removeRecentRepo(path); } catch { /* non-critical */ }
    await uiStore.loadRecentRepos();
    await refreshStatuses();
  }

  function openLocateForWs(repoId: string, ev: MouseEvent): void {
    ev.stopPropagation();
    locating = { repoId };
  }
  function openLocateForRecent(path: string, ev: MouseEvent): void {
    ev.stopPropagation();
    locating = { recentPath: path };
  }

  async function handleLocateConfirm(newPath: string): Promise<void> {
    const target = locating;
    locating = null;
    if (!target) return;
    try {
      const v = await validateRepoPath(newPath);
      if (v.status !== 'ok') {
        uiStore.showToast(v.message || 'Selected folder is not a git repository', 'error');
        return;
      }
      if (target.repoId) {
        await relocateRepo(target.repoId, newPath);
        uiStore.showToast(`Relocated to ${newPath}`, 'success');
        await workspacesStore.reloadRegistry();
      } else if (target.recentPath) {
        // Recent paths aren't registered repos by themselves — treat as a
        // simple "open at the new location".  Drop the dead entry first so
        // the most-recent slot doesn't end up duplicated.
        try { await removeRecentRepo(target.recentPath); } catch {}
        onOpenPath(newPath);
        await uiStore.loadRecentRepos();
      }
      await refreshStatuses();
    } catch (err) {
      uiStore.showToast(`${err}`, 'error');
    }
  }

  /** Cap the workspace-repos section at 5 items — any more turns the
   *  welcome screen into a scroll-fest.  The "+N more" footer row
   *  bounces to the command palette (Open Project) which is the
   *  full-fledged search / fuzzy-filter surface. */
  const WS_REPO_LIMIT = 5;
  const wsReposFull = $derived.by(() => {
    const ws = workspacesStore.active;
    if (!ws) return [];
    return ws.repo_ids
      .map(id => workspacesStore.registryById.get(id))
      .filter((r): r is NonNullable<typeof r> => !!r);
  });
  const wsRepos        = $derived(wsReposFull.slice(0, WS_REPO_LIMIT));
  const wsRepoOverflow = $derived(Math.max(0, wsReposFull.length - WS_REPO_LIMIT));

  async function openWsRepo(path: string, repoId: string) {
    const existing = tabsStore.tabs.find(t => t.id === repoId);
    if (existing) { tabsStore.setActive(existing.id); return; }
    try {
      const info = await ipcOpenRepo(path, repoId);
      tabsStore.addTab(info);
      uiStore.addRecentRepo(path);
    } catch (err) {
      uiStore.showToast(`Failed to open repo: ${err}`, 'error');
    }
  }

  function repoName(path: string): string {
    return path.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? path;
  }

  function repoDir(path: string): string {
    const parts = path.replace(/\\/g, '/').split('/').filter(Boolean);
    return parts.slice(0, -1).join('/') || path;
  }
</script>

<div class="welcome">
  <div class="content">
    <div class="logo">
      <!-- Goes through the shared ArborLogo widget so plugin-applied
           branding overrides paint here too without touching this file. -->
      <ArborLogo size={88} />
    </div>
    <h1>Arbor</h1>
    <p class="subtitle">Git GUI Client</p>

    <div class="action-row">
      <button class="btn btn-primary open-btn" onclick={() => onOpen()}>
        <FolderOpen size={15} />
        Open Repository
      </button>
      <button class="btn btn-ghost clone-btn" onclick={() => onClone()}>
        <Download size={15} />
        Clone…
      </button>
    </div>

    <p class="hint">or drag a folder here · <Kbd action="open_repo" size="sm" /> · Find a project <Kbd action="open_project" size="sm" /></p>

    <!-- Plugin quick-action cards (arbor:welcome-action) -->
    <Contribution point="arbor:welcome-action">
      {#snippet item({ payload, fire })}
        {@const p = payload as { title: string; description?: string; icon?: string; action: string }}
        <button type="button" class="welcome-plugin-card" onclick={() => fire()}>
          {#if p.icon}
            <span class="welcome-plugin-icon"><PluginIcon name={p.icon} size={20} /></span>
          {/if}
          <span class="welcome-plugin-text">
            <span class="welcome-plugin-title">{p.title}</span>
            {#if p.description}
              <span class="welcome-plugin-desc">{p.description}</span>
            {/if}
          </span>
        </button>
      {/snippet}
    </Contribution>

    {#if activeWs && wsReposFull.length > 0}
      <div class="recent-section">
        <div class="recent-header">
          <Monogram name={activeWs.name} color={workspaceColorVar(activeWs.color_idx)} size={20} />
          <span>In {activeWs.name}</span>
          <button class="recent-switcher-btn" onclick={() => uiStore.openCommandPaletteWithVerb('open-project')} use:tooltip={tooltipForAction('Open Project', 'open_project')}>
            <Kbd action="open_project" size="sm" />
          </button>
        </div>
        <div class="recent-list">
          {#each wsRepos as repo (repo.id)}
            {@const status = wsStatus.get(repo.id) ?? 'ok'}
            {@const isMissing = status !== 'ok'}
            <div class="recent-item-row" class:missing={isMissing}>
              <button
                class="recent-item"
                onclick={() => isMissing ? openLocateForWs(repo.id, new MouseEvent('click')) : openWsRepo(repo.path, repo.id)}
                use:tooltip={isMissing ? { content: repo.path, description: reasonHint(status) } : repo.path}
              >
                {#if isMissing}
                  <AlertTriangle size={12} class="recent-item-glyph missing-glyph" />
                {:else}
                  <FolderGit2 size={12} class="recent-item-glyph" />
                {/if}
                <span class="recent-item-name" class:missing-name={isMissing}>{repo.display_name}</span>
                {#if isMissing}
                  <span class="missing-pill" use:tooltip={reasonHint(status)}>missing</span>
                {/if}
                <span class="recent-item-dir">{repoDir(repo.path)}</span>
                {#if !isMissing}
                  <ChevronRight size={12} class="recent-item-arrow" />
                {/if}
              </button>
              {#if isMissing}
                <button
                  class="row-action"
                  use:tooltip={'Locate folder…'}
                  onclick={(e) => openLocateForWs(repo.id, e)}
                ><FolderSearch size={12} /></button>
              {/if}
            </div>
          {/each}
          {#if wsRepoOverflow > 0}
            <button
              class="recent-item more-item"
              onclick={() => uiStore.openCommandPaletteWithVerb('open-project')}
              use:tooltip={`Browse all repositories in ${activeWs.name}`}
            >
              <Search size={12} class="recent-item-glyph" />
              <span class="recent-item-name">Find…</span>
              <span class="recent-item-dir">{wsRepoOverflow} more in this workspace</span>
              <ChevronRight size={12} class="recent-item-arrow" />
            </button>
          {/if}
        </div>
      </div>
    {/if}

    {#if recentRepos.length > 0}
      <div class="recent-section">
        <div class="recent-header">
          <Clock size={12} />
          <span>Recent</span>
          <button class="recent-switcher-btn" onclick={() => uiStore.toggleRecentQuickSwitch()} use:tooltip={tooltipForAction('Quick switch', 'open_recent')}>
            <Kbd action="open_recent" size="sm" />
          </button>
        </div>
        <div class="recent-list">
          {#each recentRepos as path}
            {@const status = recentStatus.get(path) ?? 'ok'}
            {@const isMissing = status !== 'ok'}
            <div class="recent-item-row" class:missing={isMissing}>
              <button
                class="recent-item"
                onclick={() => isMissing ? openLocateForRecent(path, new MouseEvent('click')) : onOpenPath(path)}
                use:tooltip={isMissing ? { content: path, description: reasonHint(status) } : path}
              >
                {#if isMissing}
                  <AlertTriangle size={12} class="recent-item-glyph missing-glyph" />
                {/if}
                <span class="recent-item-name" class:missing-name={isMissing}>{repoName(path)}</span>
                {#if isMissing}
                  <span class="missing-pill" use:tooltip={reasonHint(status)}>missing</span>
                {/if}
                <span class="recent-item-dir">{repoDir(path)}</span>
                {#if !isMissing}
                  <ChevronRight size={12} class="recent-item-arrow" />
                {/if}
              </button>
              {#if isMissing}
                <button
                  class="row-action"
                  use:tooltip={'Locate folder…'}
                  onclick={(e) => openLocateForRecent(path, e)}
                ><FolderSearch size={12} /></button>
                <button
                  class="row-action danger"
                  use:tooltip={'Remove from recents'}
                  onclick={(e) => removeRecent(path, e)}
                ><Trash2 size={12} /></button>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>

{#if locating}
  <FileExplorerModal
    mode="folder"
    title="Locate project folder"
    onConfirm={handleLocateConfirm}
    onCancel={() => locating = null}
  />
{/if}

<style>
  .welcome {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-base);
    overflow-y: auto;
  }

  .content {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 32px 24px;
    width: 100%;
    max-width: 480px;
  }

  .logo {
    /* The arbor-logo.svg already carries its own gradients and shadow, so
       the wrapper just sets the box size and a gentle drop so it sits on
       the welcome screen cleanly. */
    width: 88px;
    height: 88px;
    filter: drop-shadow(0 8px 22px rgba(0, 0, 0, 0.35));
  }

  h1 {
    font-size: 28px;
    font-weight: 300;
    color: var(--text-primary);
    letter-spacing: 2px;
  }

  .subtitle {
    color: var(--text-muted);
    font-size: var(--font-size-sm);
    margin-bottom: 8px;
  }

  .action-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .open-btn, .clone-btn {
    padding: 9px 20px;
    font-size: var(--font-size-md);
    gap: 7px;
  }

  .clone-btn {
    border-color: var(--border);
    color: var(--text-secondary);
  }
  .clone-btn:hover {
    border-color: var(--border-focus);
    color: var(--text-primary);
  }

  .hint {
    color: var(--text-disabled);
    font-size: var(--font-size-xs);
    margin-top: 4px;
  }

  /* ── Plugin quick-action cards (arbor:welcome-action) ─────────────────── */
  .welcome-plugin-card {
    display: inline-flex;
    align-items: center;
    gap: 12px;
    padding: 14px 18px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 8px;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    transition: background 120ms, border-color 120ms;
    width: 100%;
  }
  .welcome-plugin-card:hover {
    background: var(--bg-hover);
    border-color: var(--border);
  }
  .welcome-plugin-icon { color: var(--accent); display: inline-flex; }
  .welcome-plugin-text { display: flex; flex-direction: column; gap: 2px; }
  .welcome-plugin-title { font-size: var(--font-size-md); font-weight: 500; }
  .welcome-plugin-desc  { font-size: var(--font-size-xs); color: var(--text-secondary); }

  /* ── Recent section ── */
  .recent-section {
    width: 100%;
    margin-top: 20px;
  }

  .recent-header {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: var(--font-size-xs);
    font-weight: 600;
    letter-spacing: 0.5px;
    text-transform: uppercase;
    margin-bottom: 8px;
    padding: 0 2px;
  }

  .recent-switcher-btn {
    margin-left: auto;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    color: var(--text-secondary);
    transition: color var(--transition-fast);
  }
  .recent-switcher-btn:hover { color: var(--accent); }

  .recent-list {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    overflow: hidden;
    background: var(--bg-elevated);
  }

  .recent-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border-subtle);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast);
    width: 100%;
  }
  .recent-item:last-child { border-bottom: none; }
  .recent-item:hover { background: rgba(255, 255, 255, 0.04); }

  .recent-item-name {
    font-size: var(--font-size-md);
    font-weight: 500;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
    max-width: 160px;
  }

  .recent-item-dir {
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    font-family: var(--font-code);
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }

  :global(.recent-item-arrow) {
    color: var(--text-disabled);
    flex-shrink: 0;
    opacity: 0;
    transition: opacity var(--transition-fast), color var(--transition-fast);
  }
  .recent-item:hover :global(.recent-item-arrow) {
    opacity: 1;
    color: var(--accent);
  }

  /* Leading icon on workspace-repo rows (distinguishes them from the
     plain-path "Recent" rows visually). */
  :global(.recent-item-glyph) {
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .recent-item:hover :global(.recent-item-glyph) { color: var(--accent); }

  /* "Show all N repos" overflow row — signals it's a meta-action (jumps
     to the command palette) by using the accent-tinted text. */
  .more-item .recent-item-name { color: var(--accent); }

  /* ── Missing-project rows (tombstone in the WelcomeScreen list) ──────── */
  .recent-item-row {
    display: flex;
    align-items: stretch;
    border-bottom: 1px solid var(--border-subtle);
  }
  .recent-item-row:last-child { border-bottom: none; }
  .recent-item-row .recent-item { border-bottom: none; flex: 1; min-width: 0; }
  .recent-item-row.missing { background: rgba(204, 167, 58, 0.04); }
  .recent-item-row.missing:hover { background: rgba(204, 167, 58, 0.10); }

  :global(.recent-item .missing-glyph) { color: var(--warning); }
  .recent-item:hover :global(.missing-glyph) { color: var(--warning); }
  .missing-name {
    color: var(--text-muted);
    font-style: italic;
    text-decoration: line-through;
    text-decoration-color: rgba(204, 167, 58, 0.5);
  }
  .missing-pill {
    font-size: var(--font-size-3xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--warning);
    background: rgba(204, 167, 58, 0.16);
    border-radius: 9px;
    padding: 1px 6px;
    flex-shrink: 0;
  }
  .row-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-left: 1px solid var(--border-subtle);
    color: var(--text-muted);
    width: 32px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .row-action:hover { background: var(--bg-hover); color: var(--accent); }
  .row-action.danger:hover { background: var(--error-subtle); color: var(--error); }
  .more-item .recent-item-dir  { color: var(--text-muted); }
  .more-item:hover :global(.recent-item-glyph) { color: var(--accent); }
</style>

<script lang="ts">
  import {
    GitPullRequest, RefreshCw, Plus, GitMerge,
    AlertCircle, Loader, Circle, Search, X, Link2, Ban,
  } from 'lucide-svelte';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import ContextMenu, { type MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { mrStore } from '$lib/stores/mr.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import type { MergeRequest } from '$lib/types/mr';
  import { tooltip } from '$lib/actions/tooltip';
  import { copyDeepLink } from '$lib/utils/deep-link-builder';

  let { onOpenCreate, onOpenDetail }: {
    onOpenCreate: () => void;
    onOpenDetail: (mr: MergeRequest) => void;
  } = $props();

  const tabId = $derived(tabsStore.activeTabId ?? '');

  // Client-side search — filters the already-loaded list. Backend doesn't
  // need to know about the query (would just add latency for typing).
  let searchQuery = $state('');

  // Right-click context menu — currently only carries the deep-link copy
  // item; future entries (assign / label / open-in-browser) can land here.
  let ctxMenu = $state<{ x: number; y: number; mr: MergeRequest } | null>(null);

  function openCtx(e: MouseEvent, mr: MergeRequest) {
    e.preventDefault();
    e.stopPropagation();
    ctxMenu = { x: e.clientX, y: e.clientY, mr };
  }

  const ctxItems = $derived<MenuItem[]>(ctxMenu ? [
    { id: 'copy-deep-link', label: 'Copy arbor:// link', icon: Link2, iconColor: '#20b2aa' },
  ] : []);

  function handleCtxSelect(id: string) {
    if (!ctxMenu) return;
    const mr = ctxMenu.mr;
    ctxMenu = null;
    if (id === 'copy-deep-link' && tabId) {
      void copyDeepLink({ kind: 'mr_open', number: mr.number }, tabId);
    }
  }

  // Reset the query whenever the active tab changes — searches don't carry
  // over between repos. Tracking `tabId` only ensures we don't clear on
  // every store update.
  $effect(() => { tabId; searchQuery = ''; });

  /** MRs visible in the list after applying the search filter. The backend
   *  state filter (open / merged) is already applied to `mrStore.mrs`. */
  const filteredMrs = $derived.by(() => {
    const q = searchQuery.trim().toLowerCase();
    if (!q) return mrStore.mrs;
    return mrStore.mrs.filter(mr =>
      mr.title.toLowerCase().includes(q) ||
      String(mr.number).includes(q) ||
      mr.sourceBranch.toLowerCase().includes(q) ||
      mr.targetBranch.toLowerCase().includes(q) ||
      mr.author.displayName.toLowerCase().includes(q) ||
      mr.author.login.toLowerCase().includes(q) ||
      mr.labels.some(l => l.name.toLowerCase().includes(q)),
    );
  });

  // Load on mount + tab change
  $effect(() => {
    if (tabId) mrStore.load(tabId);
  });

  async function refresh() {
    if (tabId) await mrStore.load(tabId, undefined, true);
  }

  function setFilter(f: 'open' | 'merged') {
    mrStore.setFilter(f);
    if (tabId) mrStore.load(tabId, f);
  }

  function providerLabel(p?: string | null) {
    if (p === 'gitlab') return 'GitLab';
    return 'GitHub';
  }

  function timeAgo(iso: string): string {
    const d = new Date(iso);
    if (isNaN(d.getTime())) return '';
    const s = Math.floor((Date.now() - d.getTime()) / 1000);
    if (s < 60)      return `${s}s ago`;
    if (s < 3600)    return `${Math.floor(s / 60)}m ago`;
    if (s < 86400)   return `${Math.floor(s / 3600)}h ago`;
    if (s < 2592000) return `${Math.floor(s / 86400)}d ago`;
    return d.toLocaleDateString();
  }

  // Sidebar title based on provider
  const sidebarTitle = $derived(
    mrStore.providerInfo?.provider === 'gitlab' ? 'Merge Requests' : 'Pull Requests'
  );

  // State machine for which view to show
  type ViewState = 'loading' | 'no-remote' | 'no-token' | 'mr-disabled' | 'error' | 'empty' | 'list';
  const viewState = $derived((() => {
    if (mrStore.providerInfo === undefined)            return 'loading' as ViewState;
    if (mrStore.loading)                               return 'loading' as ViewState;
    if (mrStore.providerInfo === null)                 return 'no-remote' as ViewState;
    if (!mrStore.providerInfo.has_token)               return 'no-token' as ViewState;
    if (mrStore.mrFeature && !mrStore.mrFeature.enabled) return 'mr-disabled' as ViewState;
    if (mrStore.error)                                 return 'error' as ViewState;
    if (mrStore.mrs.length === 0)                      return 'empty' as ViewState;
    return 'list' as ViewState;
  })());
</script>

<PanelShell
  title={sidebarTitle}
  count={filteredMrs.length > 0 && viewState === 'list' ? filteredMrs.length : null}
>
  {#snippet icon()}<GitPullRequest size={14} />{/snippet}
  {#snippet actions()}
    <button
      class="ps-btn"
      use:tooltip={'Refresh'}
      onclick={refresh}
      disabled={mrStore.loading}
      aria-label="Refresh"
    >
      <RefreshCw size={13} class={mrStore.loading ? 'spin' : ''} />
    </button>
    {#if mrStore.providerInfo?.has_token && mrStore.mrFeature?.enabled !== false}
      <button
        class="ps-btn ps-btn-accent"
        use:tooltip={'New PR / MR'}
        onclick={onOpenCreate}
        aria-label="Create pull request"
      >
        <Plus size={14} />
      </button>
    {/if}
  {/snippet}
  {#snippet toolbar()}
    <!-- Search + filters — both hidden when there's no provider/token, since
         neither has anything to act on in those states. -->
    {#if viewState !== 'no-remote' && viewState !== 'no-token' && viewState !== 'mr-disabled'}
      <div class="mr-search-wrap">
        <Search size={11} class="mr-search-icon" />
        <input
          class="mr-search"
          type="search"
          placeholder="Filter by title, #number, branch, author…"
          bind:value={searchQuery}
          aria-label="Filter merge requests"
        />
        {#if searchQuery}
          <button
            class="mr-search-clear"
            onclick={() => { searchQuery = ''; }}
            use:tooltip={'Clear filter'}
            aria-label="Clear filter"
          >
            <X size={11} />
          </button>
        {/if}
      </div>
    {/if}

    <!-- Filter tabs — always rendered to avoid layout shift on tab switch -->
    <div
      class="mr-filters"
      class:mr-filters-hidden={viewState === 'no-remote' || viewState === 'no-token' || viewState === 'mr-disabled'}
    >
      <Tabs
        items={[
          { id: 'open',   label: 'Open' },
          { id: 'merged', label: 'Merged' },
        ]}
        value={mrStore.stateFilter}
        variant="underline"
        size="sm"
        fill
        ariaLabel="MR state filter"
        onSelect={(id) => setFilter(id as 'open' | 'merged')}
      />
    </div>
  {/snippet}

  <div class="mr-body">
  {#if viewState === 'loading'}
      <!-- Loading -->
      <div class="state-view">
        <Loader size={24} class="state-icon spin" />
        <p class="state-hint">Loading…</p>
      </div>

    {:else if viewState === 'no-remote'}
      <!-- No GitHub / GitLab remote -->
      <div class="state-view">
        <AlertCircle size={28} class="state-icon state-muted" />
        <p class="state-title">No remote detected</p>
        <p class="state-hint">
          Open a repository with a GitHub or GitLab remote to manage pull requests here.
        </p>
      </div>

    {:else if viewState === 'no-token'}
      <!-- Remote found but no OAuth token -->
      <div class="state-view">
        <AlertCircle size={28} class="state-icon state-warn" />
        <p class="state-title">{providerLabel(mrStore.providerInfo?.provider)} detected</p>
        <p class="state-hint">
          Connect your {providerLabel(mrStore.providerInfo?.provider)} account in
          <strong>Settings → Authentication</strong> to view and manage {sidebarTitle.toLowerCase()}.
        </p>
      </div>

    {:else if viewState === 'mr-disabled'}
      <!-- Provider has the MR feature switched off for this repo -->
      <div class="state-view">
        <Ban size={28} class="state-icon state-muted" />
        <p class="state-title">{sidebarTitle} unavailable</p>
        <p class="state-hint">
          {mrStore.mrFeature?.reason ??
            `${sidebarTitle} are disabled on this repository.`}
        </p>
      </div>

    {:else if viewState === 'error'}
      <!-- API error -->
      <div class="state-view">
        <AlertCircle size={28} class="state-icon state-warn" />
        <p class="state-title">Failed to load</p>
        <p class="state-hint state-error-text">{mrStore.error}</p>
        {#if mrStore.error?.includes('401') || mrStore.error?.toLowerCase().includes('unauthorized')}
          <p class="state-hint state-pat-hint">
            GitLab API requires a <strong>Personal Access Token</strong> (scope: <code>api</code>), not a password.
            Update your credential in <strong>Settings → Authentication</strong>.
          </p>
        {/if}
        <button class="retry-btn" onclick={refresh}>
          <RefreshCw size={12} /> Retry
        </button>
      </div>

    {:else if viewState === 'empty'}
      <!-- Authenticated but no MRs -->
      <div class="state-view">
        <Circle size={28} class="state-icon state-muted" />
        <p class="state-title">No {mrStore.stateFilter} {sidebarTitle.toLowerCase()}</p>
        <p class="state-hint">
          {#if mrStore.stateFilter === 'open'}
            All caught up — no open {sidebarTitle.toLowerCase()} at the moment.
          {:else}
            Nothing here yet.
          {/if}
        </p>
      </div>

    {:else if filteredMrs.length === 0}
      <!-- Search returned nothing -->
      <div class="state-view">
        <Search size={28} class="state-icon state-muted" />
        <p class="state-title">No matches</p>
        <p class="state-hint">
          Nothing matches “<strong>{searchQuery}</strong>” in the current
          {mrStore.stateFilter} list.
        </p>
        <button class="retry-btn" onclick={() => { searchQuery = ''; }}>
          <X size={12} /> Clear filter
        </button>
      </div>

    {:else}
      <!-- MR list -->
      <ul class="mr-list" role="list">
        {#each filteredMrs as mr (mr.number)}
          <li>
            <button
              class="mr-item"
              class:mr-item-open={mr.state === 'open'}
              class:mr-item-merged={mr.state === 'merged'}
              class:mr-item-closed={mr.state === 'closed'}
              onclick={() => onOpenDetail(mr)}
              oncontextmenu={(e) => openCtx(e, mr)}
              use:tooltip={mr.title}
            >
              <!-- State icon -->
              <span class="mr-state-icon">
                {#if mr.state === 'merged'}
                  <GitMerge size={14} />
                {:else}
                  <GitPullRequest size={14} />
                {/if}
              </span>

              <!-- Content -->
              <div class="mr-item-body">
                <div class="mr-item-top">
                  <span class="mr-item-title">
                    {#if mr.isDraft}
                      <span class="draft-badge">Draft</span>
                    {/if}
                    {mr.title}
                  </span>
                  <span class="mr-time">{timeAgo(mr.updatedAt)}</span>
                </div>

                <div class="mr-item-bottom">
                  <span class="mr-item-number">#{mr.number}</span>
                  <span class="mr-dot">·</span>
                  <span class="mr-branch">{mr.sourceBranch}</span>
                  <span class="mr-arrow">→</span>
                  <span class="mr-branch">{mr.targetBranch}</span>
                  <span class="mr-dot">·</span>
                  <span class="mr-author">{mr.author.displayName}</span>
                  {#if mr.commentsCount > 0}
                    <span class="mr-dot">·</span>
                    <span class="mr-comments">💬 {mr.commentsCount}</span>
                  {/if}
                  {#each mr.labels.slice(0, 2) as lbl}
                    <span class="mr-label" style="background: #{lbl.color}22; color: #{lbl.color}; border: 1px solid #{lbl.color}55;">
                      {lbl.name}
                    </span>
                  {/each}
                  {#if mr.labels.length > 2}
                    <span class="mr-label-more">+{mr.labels.length - 2}</span>
                  {/if}
                </div>
              </div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</PanelShell>

{#if ctxMenu}
  <ContextMenu
    x={ctxMenu.x}
    y={ctxMenu.y}
    items={ctxItems}
    onSelect={handleCtxSelect}
    onClose={() => { ctxMenu = null; }}
  />
{/if}

<style>
  /* ── Search bar ──────────────────────────────────────────────────────────── */
  .mr-search-wrap {
    position: relative;
    display: flex;
    align-items: center;
    padding: 6px 8px 4px;
    flex-shrink: 0;
  }
  :global(.mr-search-icon) {
    position: absolute;
    left: 16px;
    color: var(--text-muted);
    pointer-events: none;
  }
  .mr-search {
    width: 100%;
    padding: 4px 24px 4px 26px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 11.5px;
    color: var(--text-primary);
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .mr-search::placeholder { color: var(--text-disabled); }
  .mr-search:focus { border-color: var(--accent); }
  /* Hide native ::-webkit-search-* affordances — we render our own clear btn */
  .mr-search::-webkit-search-cancel-button,
  .mr-search::-webkit-search-decoration { -webkit-appearance: none; appearance: none; }

  .mr-search-clear {
    position: absolute;
    right: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    border-radius: 50%;
    padding: 0;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .mr-search-clear:hover { background: var(--bg-hover); color: var(--text-primary); }

  /* ── Filter tabs ────────────────────────────────────────────────────────────
     Strip rendered by shared <Tabs variant="underline" size="sm" fill>. The
     wrapper just contributes the side-padding + the hidden-state visibility
     trick we use to keep layout stable. */
  .mr-filters {
    display: flex;
    padding: 0 6px;
    flex-shrink: 0;
  }
  .mr-filters-hidden {
    visibility: hidden;
    pointer-events: none;
  }
  .mr-filters :global(.tabs) { flex: 1; }

  /* ── Body ─────────────────────────────────────────────────────────────────── */
  .mr-body {
    display: flex;
    flex-direction: column;
    min-height: 100%;
  }

  /* ── State views ─────────────────────────────────────────────────────────── */
  .state-view {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    flex: 1;
    gap: 8px;
    padding: 32px 24px;
    text-align: center;
  }
  .state-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    margin: 0;
  }
  .state-hint {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    max-width: 220px;
    margin: 0;
  }
  .state-hint strong { color: var(--text-secondary); }
  .state-pat-hint {
    background: color-mix(in srgb, var(--status-warning, #fbbf24) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--status-warning, #fbbf24) 25%, transparent);
    border-radius: var(--radius-sm);
    padding: 6px 10px;
    font-family: var(--font-ui-sans);
    color: var(--text-secondary) !important;
  }
  .state-pat-hint code {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--accent);
  }
  .state-error-text {
    font-family: var(--font-code);
    font-size: 11px;
    color: var(--status-error, #f87171);
    word-break: break-word;
  }

  :global(.state-icon)  { opacity: 0.5; }
  :global(.state-warn)  { opacity: 1; color: var(--status-warning, #fbbf24) !important; }
  :global(.state-muted) { color: var(--text-disabled) !important; }

  .retry-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 12px;
    font-size: 11px;
    font-weight: 500;
    background: transparent;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    cursor: pointer;
    font-family: var(--font-ui-sans);
    margin-top: 4px;
    transition: background var(--transition-fast);
  }
  .retry-btn:hover { background: var(--bg-hover); }

  /* ── MR List ──────────────────────────────────────────────────────────────── */
  .mr-list {
    list-style: none;
    padding: 6px;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .mr-item {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    text-align: left;
    cursor: pointer;
    overflow: hidden;
    transition: background var(--transition-fast), border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .mr-item:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0,0,0,0.15);
  }

  .mr-state-icon { flex-shrink: 0; margin-top: 2px; }
  .mr-item-open   .mr-state-icon { color: var(--success); }
  .mr-item-merged .mr-state-icon { color: var(--color-tag); }

  .mr-item-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .mr-item-top {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .mr-item-title {
    flex: 1;
    min-width: 0;
    font-size: var(--font-size-sm);
    font-weight: 500;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .mr-time {
    flex-shrink: 0;
    font-size: 10px;
    color: var(--text-muted);
    white-space: nowrap;
  }
  .draft-badge {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0 4px;
    flex-shrink: 0;
    line-height: 1.4;
  }
  .mr-item-bottom {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    color: var(--text-muted);
    flex-wrap: nowrap;
    overflow: hidden;
    min-width: 0;
  }
  .mr-item-number {
    flex-shrink: 0;
    font-family: var(--font-code);
    color: var(--text-muted);
  }
  .mr-branch {
    max-width: 70px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-code);
    flex-shrink: 1;
  }
  .mr-arrow { color: var(--text-disabled); flex-shrink: 0; }
  .mr-author { flex-shrink: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 80px; }
  .mr-dot { color: var(--text-disabled); flex-shrink: 0; }
  .mr-comments { flex-shrink: 0; }
  .mr-label {
    font-size: 10px;
    font-weight: 500;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    white-space: nowrap;
    flex-shrink: 0;
  }
  .mr-label-more { font-size: 10px; color: var(--text-muted); flex-shrink: 0; }

  :global(.spin) { animation: spin 1s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>

<script lang="ts">
  import {
    TicketCheck, RefreshCw, Plus, Search, AlertCircle, Loader, Circle,
    SlidersHorizontal, X, User, Filter, Loader2, ExternalLink,
    Eye, EyeOff, ArrowUpDown, ArrowUp, ArrowDown,
  } from 'lucide-svelte';
  import type { IssueSortField } from '$lib/types/issues';
  import { SORT_FIELD_LABELS } from '$lib/types/issues';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { listen } from '@tauri-apps/api/event';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import type { IssueProvider } from '$lib/stores/issues.svelte';
  import { startLinearOAuth, startJiraOAuth } from '$lib/ipc/auth';
  import { getRepoConfig, setRepoConfig } from '$lib/ipc/config';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { uiStore } from '$lib/stores/ui.svelte';
  import type { Issue, IssueStatus } from '$lib/types/issues';
  import PanelShell from '$lib/components/shared/ui/PanelShell.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import FilterButton from '$lib/components/shared/ui/FilterButton.svelte';
  import BrandTile from '$lib/components/shared/internal/BrandTile.svelte';
  import IssueDetailModal from './IssueDetailModal.svelte';
  import CreateIssueModal from './CreateIssueModal.svelte';
  import IssueContextMenu from './IssueContextMenu.svelte';
  import { tooltip } from '$lib/actions/tooltip';

  // ── Per-repo tracker config ───────────────────────────────────────────────
  const tab = $derived(tabsStore.activeTab);
  let trackerForRepo  = $state<string | null>(null);
  let trackerLoading  = $state(false);

  $effect(() => {
    const t = tab;
    if (!t) { trackerForRepo = null; trackerLoading = false; return; }
    trackerLoading = true;
    getRepoConfig(t.id)
      .then(cfg => {
        const tracker = cfg.issue_tracker ?? null;
        trackerForRepo = tracker;
        issuesStore.setDefaultProjectId(cfg.issue_tracker_project_id ?? null, tracker ?? undefined);
        if (tracker === 'linear' || tracker === 'jira') {
          issuesStore.setProvider(tracker as IssueProvider);
        }
      })
      .catch(() => { trackerForRepo = null; })
      .finally(() => { trackerLoading = false; });
  });

  async function selectTracker(tracker: string) {
    if (!tab) return;
    trackerForRepo = tracker || null; // update immediately so viewState transitions now
    if (tracker === 'linear' || tracker === 'jira') {
      issuesStore.setProvider(tracker as IssueProvider);
    } else {
      issuesStore.setProvider(null);
    }
    try {
      const cfg = await getRepoConfig(tab.id);
      await setRepoConfig(tab.id, { ...cfg, issue_tracker: tracker });
    } catch {
      trackerForRepo = null; // revert on error
    }
  }

  // Load auth on mount (provider set by tracker effect above)
  $effect(() => {
    if (issuesStore.authStatus === null && issuesStore.activeProvider !== null) {
      issuesStore.loadAuthStatus();
    }
  });

  // Reload issues when filters change
  $effect(() => {
    issuesStore.filters; // track
    if (issuesStore.authStatus?.authenticated) issuesStore.loadIssues();
  });

  let searchQuery = $state('');
  let searchTimeout: ReturnType<typeof setTimeout> | null = null;
  let showFilters = $state(true);
  // ── Linear no-token state ────────────────────────────────────────────────
  let tokenInput    = $state('');
  let tokenError    = $state('');
  let tokenSaving   = $state(false);
  let showTokenInput = $state(false);
  let oauthWaiting   = $state(false);
  let oauthError     = $state('');
  let oauthUnsub: (() => void) | null = null;

  // ── Jira no-token state ──────────────────────────────────────────────────
  let jiraEmail        = $state('');
  let jiraApiToken     = $state('');
  let jiraDomain       = $state('');
  let jiraShowToken    = $state(false);
  let jiraBasicSaving  = $state(false);
  let jiraBasicError   = $state('');
  let jiraShowBasic    = $state(false);
  let jiraOAuthWaiting = $state(false);
  let jiraOAuthError   = $state('');
  let jiraOAuthUnsub: (() => void) | null = null;

  async function saveJiraBasicAuth() {
    const isCloud = jiraDomain.trim().endsWith('.atlassian.net');
    if (!jiraDomain.trim() || !jiraApiToken.trim() || (isCloud && !jiraEmail.trim())) return;
    jiraBasicSaving = true; jiraBasicError = '';
    try {
      await issuesStore.saveJiraBasicAuth(jiraEmail.trim(), jiraApiToken.trim(), jiraDomain.trim());
      jiraEmail = ''; jiraApiToken = ''; jiraDomain = '';
      uiStore.showToast('Jira connected', 'success');
    } catch (e) {
      jiraBasicError = String(e);
    } finally {
      jiraBasicSaving = false;
    }
  }

  async function startJiraOAuthFlow() {
    jiraOAuthWaiting = true; jiraOAuthError = '';
    jiraOAuthUnsub?.();
    jiraOAuthUnsub = await listen<boolean>('arbor://jira-oauth-done', ({ payload }) => {
      jiraOAuthUnsub?.(); jiraOAuthUnsub = null;
      jiraOAuthWaiting = false;
      if (payload) {
        issuesStore.loadAuthStatus();
        uiStore.showToast('Jira connected via OAuth', 'success');
      } else {
        jiraOAuthError = 'OAuth failed — please retry.';
      }
    });
    try {
      const url = await startJiraOAuth();
      try { await openUrl(url); } catch { /* user can copy */ }
    } catch (e) {
      jiraOAuthWaiting = false; jiraOAuthError = String(e);
      jiraOAuthUnsub?.(); jiraOAuthUnsub = null;
    }
  }

  async function startOAuth() {
    oauthWaiting = true; oauthError = '';
    oauthUnsub?.();
    oauthUnsub = await listen<boolean>('arbor://linear-oauth-done', ({ payload }) => {
      oauthUnsub?.(); oauthUnsub = null;
      oauthWaiting = false;
      if (payload) {
        issuesStore.loadAuthStatus();
        uiStore.showToast('Linear connected via OAuth', 'success');
      } else {
        oauthError = 'OAuth failed — check your Client ID or try again.';
      }
    });
    try {
      const url = await startLinearOAuth();
      try { await openUrl(url); } catch { /* user can copy */ }
    } catch (e) {
      oauthWaiting = false; oauthError = String(e);
      oauthUnsub?.(); oauthUnsub = null;
    }
  }

  const SORT_FIELDS: IssueSortField[] = ['ticket_id', 'updated_at', 'created_at', 'priority', 'title', 'status'];

  let loadingFilterOpts = $state(false);

  // Milestone groups — grouped by project for display
  const milestoneGroups = $derived((() => {
    const milestones = issuesStore.filterOptions?.milestones ?? [];
    const map = new Map<string, { name: string; items: typeof milestones }>();
    for (const ms of milestones) {
      const key  = ms.projectId  ?? '__none__';
      const name = ms.projectName ?? 'No Project';
      if (!map.has(key)) map.set(key, { name, items: [] });
      map.get(key)!.items.push(ms);
    }
    return [...map.values()];
  })());

  function onSearchInput() {
    if (searchTimeout) clearTimeout(searchTimeout);
    searchTimeout = setTimeout(() => {
      issuesStore.setFilters({ query: searchQuery || undefined });
    }, 350);
  }

  async function saveToken() {
    if (!tokenInput.trim()) return;
    tokenError  = '';
    tokenSaving = true;
    try {
      await issuesStore.saveToken(tokenInput.trim());
      tokenInput = '';
    } catch (e) {
      tokenError = String(e);
    } finally {
      tokenSaving = false;
    }
  }

  function toggleAssigneeMe() {
    issuesStore.setFilters({ assigneeMe: !issuesStore.filters.assigneeMe });
  }

  function toggleStatusFilter(id: string) {
    const current = issuesStore.filters.statusIds ?? [];
    const next = current.includes(id) ? current.filter(s => s !== id) : [...current, id];
    issuesStore.setFilters({ statusIds: next });
  }

  function hasStatusFilter(id: string) {
    return (issuesStore.filters.statusIds ?? []).includes(id);
  }

  function clearAllFilters() {
    searchQuery = '';
    issuesStore.clearFilters();
  }

  const hasActiveFilters = $derived(
    !!issuesStore.filters.assigneeMe ||
    (issuesStore.filters.statusIds?.length ?? 0) > 0 ||
    (issuesStore.filters.issueTypeIds?.length ?? 0) > 0 ||
    !!issuesStore.filters.teamId ||
    !!issuesStore.filters.projectId ||
    !!issuesStore.filters.milestoneId ||
    !!issuesStore.filters.query
  );

  function onIssueContextMenu(e: MouseEvent, issue: Issue) {
    e.preventDefault();
    issuesStore.openContextMenu(issue, e.clientX, e.clientY);
  }

  function priorityIcon(p: number): string {
    return ['—', '🔴', '🟠', '🟡', '🔵'][p] ?? '—';
  }

  function statusTypeClass(type: string): string {
    if (type === 'completed') return 'status-done';
    if (type === 'started')   return 'status-progress';
    if (type === 'cancelled') return 'status-cancelled';
    return 'status-todo';
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

  function statusIcon(statusType: string, color: string): string {
    const c = color || '#6b7280';
    const sw = '1.8';
    if (statusType === 'completed') {
      return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6.5" fill="${c}"/><polyline points="4.5,7.5 6.5,9.5 10.5,5" fill="none" stroke="white" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
    }
    if (statusType === 'started') {
      return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/><path d="M7.5,1.5 A6,6 0 0,1 7.5,13.5 L7.5,7.5 Z" fill="${c}"/></svg>`;
    }
    if (statusType === 'cancelled') {
      return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/><line x1="5" y1="5" x2="10" y2="10" stroke="${c}" stroke-width="${sw}" stroke-linecap="round"/><line x1="10" y1="5" x2="5" y2="10" stroke="${c}" stroke-width="${sw}" stroke-linecap="round"/></svg>`;
    }
    if (statusType === 'unstarted') {
      return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/></svg>`;
    }
    // backlog (default)
    return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}" stroke-dasharray="3.5 2.5"/></svg>`;
  }

  // Reload filter options if they got lost (e.g. after error)
  $effect(() => {
    if (issuesStore.authStatus?.authenticated && issuesStore.filterOptions === null && !issuesStore.authLoading) {
      issuesStore.loadFilterOptions();
    }
  });

  function toggleFilters() {
    showFilters = !showFilters;
  }

  async function loadFilterOptsIfNeeded() {
    if (!issuesStore.filterOptions && !loadingFilterOpts) {
      loadingFilterOpts = true;
      try { await issuesStore.loadFilterOptions(); } finally { loadingFilterOpts = false; }
    }
  }

  function labelChipStyle(color: string): string {
    const hex = color.startsWith('#') ? color : `#${color}`;
    if (hex.length < 7) return '';
    const r = parseInt(hex.slice(1, 3), 16) / 255;
    const g = parseInt(hex.slice(3, 5), 16) / 255;
    const b = parseInt(hex.slice(5, 7), 16) / 255;
    const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if (lum < 0.1) {
      return `background:rgba(160,160,160,0.12);color:var(--text-secondary);border:1px solid rgba(160,160,160,0.25)`;
    }
    return `background:${hex}22;color:${hex};border:1px solid ${hex}55`;
  }

  type ViewState = 'loading-auth' | 'no-tracker' | 'no-token' | 'loading' | 'error' | 'empty' | 'list';
  const viewState = $derived((() => {
    if (!tab || trackerLoading)                                      return 'loading-auth' as ViewState;
    if (!trackerForRepo)                                             return 'no-tracker'   as ViewState;
    if (issuesStore.authStatus === null || issuesStore.authLoading)  return 'loading-auth' as ViewState;
    if (!issuesStore.authStatus.authenticated)                       return 'no-token'     as ViewState;
    if (issuesStore.loading)                                         return 'loading'      as ViewState;
    if (issuesStore.error)                                           return 'error'        as ViewState;
    if (issuesStore.issues.length === 0)                             return 'empty'        as ViewState;
    return 'list' as ViewState;
  })());

  // group statuses by type for filter dropdown.
  // Falls back to deriving unique statuses from loaded issues when the API returns none.
  const statusGroups = $derived((() => {
    let opts = issuesStore.filterOptions?.statuses ?? [];
    if (opts.length === 0 && issuesStore.issues.length > 0) {
      const seen = new Map<string, IssueStatus>();
      for (const issue of issuesStore.issues) {
        if (!seen.has(issue.status.id)) seen.set(issue.status.id, issue.status);
      }
      opts = [...seen.values()];
    }
    const order = ['backlog', 'unstarted', 'started', 'completed', 'cancelled'];
    const groups: Record<string, IssueStatus[]> = {};
    for (const s of opts) {
      const g = s.statusType;
      if (!groups[g]) groups[g] = [];
      groups[g].push(s);
    }
    return order.filter(o => groups[o]).map(o => ({ type: o, items: groups[o] }));
  })());

  // Derive teams from loaded issues as fallback when API returns none.
  const effectiveTeams = $derived((() => {
    const teams = issuesStore.filterOptions?.teams ?? [];
    if (teams.length > 0) return teams;
    const seen = new Map<string, { id: string; name: string; key: string }>();
    for (const issue of issuesStore.issues) {
      if (issue.team && !seen.has(issue.team.id)) seen.set(issue.team.id, issue.team);
    }
    return [...seen.values()];
  })());

  // Issue type list from filter options (Jira only).
  const issueTypeOptions = $derived(issuesStore.filterOptions?.issueTypes ?? []);

  function toggleIssueTypeFilter(id: string) {
    const current = issuesStore.filters.issueTypeIds ?? [];
    const next = current.includes(id) ? current.filter(t => t !== id) : [...current, id];
    issuesStore.setFilters({ issueTypeIds: next });
  }

  function hasIssueTypeFilter(id: string) {
    return (issuesStore.filters.issueTypeIds ?? []).includes(id);
  }
</script>

<!-- Context menu -->
{#if issuesStore.contextMenuIssue && issuesStore.contextMenuPos}
  <IssueContextMenu
    issue={issuesStore.contextMenuIssue}
    x={issuesStore.contextMenuPos.x}
    y={issuesStore.contextMenuPos.y}
    onClose={() => issuesStore.closeContextMenu()}
    onOpenDetail={() => { issuesStore.selectAndLoadIssue(issuesStore.contextMenuIssue!); issuesStore.closeContextMenu(); }}
  />
{/if}

<!-- Issue detail modal -->
{#if issuesStore.selectedIssue}
  <IssueDetailModal
    issue={issuesStore.selectedIssue}
    onClose={() => issuesStore.selectIssue(null)}
  />
{/if}

<!-- Create issue modal -->
{#if issuesStore.createOpen}
  <CreateIssueModal onClose={() => issuesStore.closeCreate()} />
{/if}

<PanelShell
  title="Issues"
  count={viewState === 'list' ? issuesStore.issues.length : null}
  scrollable={false}
>
  {#snippet icon()}
    <!-- Brand logo for the active provider when configured for this repo —
         falls back to the generic TicketCheck while the user is still in
         the no-tracker / loading state. Tile renders as a small square so
         it sits cleanly in the panel header without overflowing. -->
    {#if trackerForRepo === 'linear'}
      <BrandTile brand="linear" size={11} tileSize={16} />
    {:else if trackerForRepo === 'jira'}
      <BrandTile brand="jira" size={11} tileSize={16} />
    {:else}
      <TicketCheck size={14} />
    {/if}
  {/snippet}
  {#snippet actions()}
    {#if issuesStore.authStatus?.authenticated}
      <button
        class="ps-btn"
        class:ps-btn-active={showFilters}
        use:tooltip={showFilters ? 'Hide filters' : 'Show filters'}
        onclick={toggleFilters}
        style="position:relative"
      >
        <SlidersHorizontal size={13} />
        {#if hasActiveFilters && !showFilters}
          <span class="hdr-filter-badge"></span>
        {/if}
      </button>
      <Dropdown position="fixed" direction="down" width="180px">
        {#snippet trigger({ open, toggle })}
          <button
            class="ps-btn"
            class:ps-btn-active={open}
            use:tooltip={`Sort: ${SORT_FIELD_LABELS[issuesStore.sortField]} (${issuesStore.sortDir === 'asc' ? '↑' : '↓'})`}
            onclick={toggle}
          >
            <ArrowUpDown size={13} />
          </button>
        {/snippet}
        {#snippet children({ close })}
          <div class="sort-drop-section">Order by</div>
          {#each SORT_FIELDS as field}
            <button
              class="chip-drop-item sort-item"
              class:chip-drop-selected={issuesStore.sortField === field}
              onclick={() => { issuesStore.setSort(field, issuesStore.sortField === field ? (issuesStore.sortDir === 'asc' ? 'desc' : 'asc') : issuesStore.sortDir); close(); }}
            >
              <span class="sort-label">{SORT_FIELD_LABELS[field]}</span>
              {#if issuesStore.sortField === field}
                <span class="sort-dir-icon">
                  {#if issuesStore.sortDir === 'asc'}<ArrowUp size={10} />{:else}<ArrowDown size={10} />{/if}
                </span>
              {/if}
            </button>
          {/each}
          <div class="sort-drop-sep"></div>
          <div class="sort-drop-section">Direction</div>
          <button class="chip-drop-item sort-item" class:chip-drop-selected={issuesStore.sortDir === 'asc'} onclick={() => { issuesStore.setSort(issuesStore.sortField, 'asc'); close(); }}>
            <ArrowUp size={10} /> Ascending
            {#if issuesStore.sortDir === 'asc'}<span class="check">✓</span>{/if}
          </button>
          <button class="chip-drop-item sort-item" class:chip-drop-selected={issuesStore.sortDir === 'desc'} onclick={() => { issuesStore.setSort(issuesStore.sortField, 'desc'); close(); }}>
            <ArrowDown size={10} /> Descending
            {#if issuesStore.sortDir === 'desc'}<span class="check">✓</span>{/if}
          </button>
        {/snippet}
      </Dropdown>
      <button
        class="ps-btn"
        use:tooltip={'Refresh'}
        onclick={() => issuesStore.loadIssues()}
        disabled={issuesStore.loading}
      >
        <RefreshCw size={13} class={issuesStore.loading ? 'spin' : ''} />
      </button>
      {#if issuesStore.activeProvider !== 'jira'}
        <button
          class="ps-btn ps-btn-accent"
          use:tooltip={'New issue'}
          onclick={() => issuesStore.openCreate()}
        >
          <Plus size={14} />
        </button>
      {/if}
    {/if}
  {/snippet}
  {#snippet toolbar()}
    <!-- Filter options error banner -->
    {#if issuesStore.authStatus?.authenticated && issuesStore.filterOptionsError && !issuesStore.filterOptions}
      <div class="filter-opts-error">
        <AlertCircle size={11} />
        <span>Filters unavailable: {issuesStore.filterOptionsError}</span>
        <button onclick={() => issuesStore.loadFilterOptions()}>Retry</button>
      </div>
    {/if}

    <!-- Search + filters bar (when authenticated) -->
    {#if issuesStore.authStatus?.authenticated}
      <div class="is-search-row">
        <div class="is-search-wrap">
          <Search size={12} class="is-search-icon" />
          <input
            class="is-search"
            type="text"
            placeholder="Search by title or code — # for code only, ~ for text only"
            title={'Default: matches both ticket title and code.\n' +
                   '  e.g. "PROJ-42" finds that ticket and anything mentioning it.\n' +
                   'Prefix with # to force code-only search:\n' +
                   '  e.g. "#PROJ-42" matches the ticket key only, ignoring titles/comments.\n' +
                   'Prefix with ~ to force text-only search:\n' +
                   '  e.g. "~PROJ-42" matches titles/comments only, not the key.'}
            bind:value={searchQuery}
            oninput={onSearchInput}
          />
          {#if searchQuery}
            <button class="is-search-clear" onclick={() => { searchQuery = ''; issuesStore.setFilters({ query: undefined }); }}>
              <X size={11} />
            </button>
          {/if}
        </div>
      </div>

      {#if showFilters}
      <!-- Quick filter chips -->
      <div class="is-chips">
        <!-- Assigned to me -->
        <button
          class="chip"
          class:chip-active={issuesStore.filters.assigneeMe}
          onclick={toggleAssigneeMe}
        >
          <User size={10} /> Me
        </button>

        <!-- Status filter -->
        <FilterButton
          label="Status"
          icon={Filter}
          count={issuesStore.filters.statusIds?.length ?? 0}
          loading={loadingFilterOpts}
          onopen={loadFilterOptsIfNeeded}
        >
          {#snippet children({ close: _close })}
            {#if issuesStore.filterOptionsError}
              <div class="chip-drop-error">
                <span>Error: {issuesStore.filterOptionsError}</span>
                <button class="chip-drop-retry" onclick={() => issuesStore.loadFilterOptions()}>Retry</button>
              </div>
            {:else if statusGroups.length === 0}
              <div class="chip-drop-empty">No statuses available</div>
            {:else}
              {#each statusGroups as grp}
                <div class="chip-drop-group">{grp.type}</div>
                {#each grp.items as st}
                  <button class="chip-drop-item" class:chip-drop-selected={hasStatusFilter(st.id)} onclick={() => toggleStatusFilter(st.id)}>
                    <span class="status-dot" style="background:{st.color}"></span>
                    {st.name}
                    {#if hasStatusFilter(st.id)}<span class="check">✓</span>{/if}
                  </button>
                {/each}
              {/each}
            {/if}
          {/snippet}
        </FilterButton>

        <!-- Team (Linear) / Project (Jira) filter -->
        {#if effectiveTeams.length > 0}
          {@const isJira = issuesStore.activeProvider === 'jira'}
          {@const teamLabel = isJira ? 'Project' : 'Team'}
          <FilterButton
            label={effectiveTeams.find(t => t.id === issuesStore.filters.teamId)?.name ?? teamLabel}
            active={!!issuesStore.filters.teamId}
            wide
            searchable={effectiveTeams.length > 5}
            searchPlaceholder="Filter {teamLabel.toLowerCase()}s…"
          >
            {#snippet children({ filter, close })}
              {@const filtered = filter.trim() ? effectiveTeams.filter(t => t.name.toLowerCase().includes(filter.toLowerCase()) || t.key.toLowerCase().includes(filter.toLowerCase())) : effectiveTeams}
              {#if !filter.trim()}
                <button class="chip-drop-item" onclick={() => { issuesStore.setFilters({ teamId: undefined }); close(); }}>
                  All {teamLabel.toLowerCase()}s {!issuesStore.filters.teamId ? '✓' : ''}
                </button>
              {/if}
              {#each filtered as team}
                <button class="chip-drop-item" class:chip-drop-selected={issuesStore.filters.teamId === team.id} onclick={() => { issuesStore.setFilters({ teamId: team.id }); close(); }}>
                  <span class="team-key">{team.key}</span> {team.name}
                  {#if issuesStore.filters.teamId === team.id}<span class="check">✓</span>{/if}
                </button>
              {:else}
                <div class="chip-drop-empty">No results</div>
              {/each}
            {/snippet}
          </FilterButton>
        {/if}

        <!-- Issue Type filter (Jira only) -->
        {#if issueTypeOptions.length > 0}
          <FilterButton
            label="Type"
            count={issuesStore.filters.issueTypeIds?.length ?? 0}
          >
            {#snippet children({ close: _close })}
              {#each issueTypeOptions as it}
                <button class="chip-drop-item" class:chip-drop-selected={hasIssueTypeFilter(it.id)} onclick={() => toggleIssueTypeFilter(it.id)}>
                  <span class="status-dot" style="background:{it.color}"></span>
                  {it.name}
                  {#if hasIssueTypeFilter(it.id)}<span class="check">✓</span>{/if}
                </button>
              {/each}
            {/snippet}
          </FilterButton>
        {/if}

        <!-- Project filter -->
        {#if (issuesStore.filterOptions?.projects?.length ?? 0) > 0}
          <FilterButton
            label={issuesStore.filterOptions?.projects.find(p => p.id === issuesStore.filters.projectId)?.name ?? 'Project'}
            active={!!issuesStore.filters.projectId}
          >
            {#snippet children({ close })}
              <button class="chip-drop-item" onclick={() => { issuesStore.setFilters({ projectId: undefined }); close(); }}>
                All projects {!issuesStore.filters.projectId ? '✓' : ''}
              </button>
              {#each issuesStore.filterOptions?.projects ?? [] as proj}
                <button class="chip-drop-item" class:chip-drop-selected={issuesStore.filters.projectId === proj.id} onclick={() => { issuesStore.setFilters({ projectId: proj.id }); close(); }}>
                  {#if proj.color}<span class="status-dot" style="background:{proj.color}"></span>{/if}
                  {proj.name}
                  {#if issuesStore.filters.projectId === proj.id}<span class="check">✓</span>{/if}
                </button>
              {/each}
            {/snippet}
          </FilterButton>
        {/if}

        <!-- Milestone filter -->
        {#if (issuesStore.filterOptions?.milestones?.length ?? 0) > 0}
          <FilterButton
            label={issuesStore.filterOptions?.milestones.find(m => m.id === issuesStore.filters.milestoneId)?.name ?? 'Milestone'}
            active={!!issuesStore.filters.milestoneId}
          >
            {#snippet children({ close })}
              <button class="chip-drop-item" onclick={() => { issuesStore.setFilters({ milestoneId: undefined }); close(); }}>
                All milestones {!issuesStore.filters.milestoneId ? '✓' : ''}
              </button>
              {#each milestoneGroups as group}
                <div class="chip-drop-group">{group.name}</div>
                {#each group.items as ms}
                  <button class="chip-drop-item" class:chip-drop-selected={issuesStore.filters.milestoneId === ms.id} onclick={() => { issuesStore.setFilters({ milestoneId: ms.id }); close(); }}>
                    {ms.name}
                    {#if ms.targetDate}<span class="ms-date-chip">{ms.targetDate}</span>{/if}
                    {#if issuesStore.filters.milestoneId === ms.id}<span class="check">✓</span>{/if}
                  </button>
                {/each}
              {/each}
            {/snippet}
          </FilterButton>
        {/if}

        {#if hasActiveFilters}
          <button class="chip chip-clear" onclick={clearAllFilters} use:tooltip={'Clear all filters'}>
            <X size={9} />
          </button>
        {/if}
      </div>
      {/if}
    {/if}
  {/snippet}

  <!-- Body -->
  <div class="is-body">

    {#if viewState === 'loading-auth'}
      <div class="state-view">
        <Loader size={22} class="state-icon spin" />
        <p class="state-hint">Connecting…</p>
      </div>

    {:else if viewState === 'no-tracker'}
      <div class="setup-view">
        <TicketCheck size={32} class="setup-icon" />
        <p class="setup-title">Issue Tracker</p>
        <p class="setup-hint">Choose the issue tracker for this project.</p>

        <button class="tracker-option" onclick={() => selectTracker('linear')}>
          <BrandTile brand="linear" size={16} tileSize={28} />
          <span class="tracker-name">Linear</span>
        </button>

        <button class="tracker-option" onclick={() => selectTracker('jira')}>
          <BrandTile brand="jira" size={16} tileSize={28} />
          <span class="tracker-name">Jira</span>
        </button>
      </div>

    {:else if viewState === 'no-token'}

      {#if trackerForRepo === 'jira'}
        <!-- ── Jira setup ── -->
        <div class="setup-view">
          <BrandTile brand="jira" size={22} tileSize={42} class="setup-logo" />
          <p class="setup-title">Connect Jira</p>

          {#if !jiraShowBasic && !jiraOAuthWaiting}
            <p class="setup-hint">Use your Atlassian API token to connect.</p>
            <button class="setup-oauth-btn jira-connect-btn" onclick={() => (jiraShowBasic = true)}>
              Connect with API Token
            </button>
            <div class="setup-divider"><span>or</span></div>
            <button class="setup-pat-toggle" onclick={() => { jiraShowBasic = false; startJiraOAuthFlow(); }}>
              Use OAuth 2.0
            </button>
          {/if}

          {#if jiraShowBasic}
            {@const isCloud = jiraDomain.trim().endsWith('.atlassian.net')}
            <div class="setup-jira-form">
              <input class="setup-input" type="text" placeholder="mycompany.atlassian.net" bind:value={jiraDomain} />
              {#if isCloud}
                <input class="setup-input" type="email" placeholder="your@email.com" bind:value={jiraEmail} />
              {/if}
              <div class="setup-token-row">
                <input class="setup-input" style="flex:1"
                  type={jiraShowToken ? 'text' : 'password'}
                  placeholder={isCloud ? 'API token' : 'Personal Access Token (PAT)'}
                  bind:value={jiraApiToken}
                  onkeydown={(e) => e.key === 'Enter' && saveJiraBasicAuth()}
                />
                <button class="setup-eye-btn" onclick={() => jiraShowToken = !jiraShowToken}>
                  {#if jiraShowToken}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
                </button>
              </div>
              {#if isCloud}
                <p class="setup-hint" style="font-size:10px">
                  Get API token at <code>id.atlassian.com → Security → API tokens</code>
                </p>
              {:else}
                <p class="setup-hint" style="font-size:10px">
                  Generate a PAT in Jira → Profile → Personal Access Tokens
                </p>
              {/if}
              <div class="setup-row">
                <button class="setup-btn jira-connect-btn"
                        onclick={saveJiraBasicAuth}
                        disabled={jiraBasicSaving || (isCloud && !jiraEmail.trim()) || !jiraApiToken.trim() || !jiraDomain.trim()}>
                  {jiraBasicSaving ? 'Connecting…' : 'Connect'}
                </button>
                <button class="setup-pat-toggle" onclick={() => { jiraShowBasic = false; jiraBasicError = ''; }}>
                  Cancel
                </button>
              </div>
              {#if jiraBasicError}<p class="setup-error">{jiraBasicError}</p>{/if}
            </div>
          {/if}

          {#if jiraOAuthWaiting}
            <button class="setup-oauth-btn jira-connect-btn" disabled>
              <Loader2 size={13} class="spin" /> Waiting for browser…
            </button>
            <p class="setup-hint">Approve access in Atlassian, then return here.</p>
            <button class="setup-pat-toggle" onclick={() => { jiraOAuthWaiting = false; jiraOAuthUnsub?.(); }}>Cancel</button>
          {/if}

          {#if jiraOAuthError}<p class="setup-error">{jiraOAuthError}</p>{/if}
          <button class="setup-back-btn" onclick={() => selectTracker('')}>← Change tracker</button>
        </div>

      {:else}
        <!-- ── Linear setup ── -->
        <div class="setup-view">
          <TicketCheck size={32} class="setup-icon" />
          <p class="setup-title">Connect Linear</p>

          <!-- OAuth option (primary) -->
          <button class="setup-oauth-btn" onclick={startOAuth} disabled={oauthWaiting}>
            {#if oauthWaiting}
              <Loader2 size={13} class="spin" /> Waiting for browser…
            {:else}
              <ExternalLink size={13} /> Authorize with OAuth
            {/if}
          </button>
          {#if oauthWaiting}
            <p class="setup-hint">Approve access in your browser, then return here.</p>
          {/if}

          {#if oauthError}
            <p class="setup-error">{oauthError}</p>
          {/if}

          <!-- Divider + PAT toggle -->
          <div class="setup-divider">
            <span>or use a Personal API Key</span>
          </div>

          {#if !showTokenInput}
            <button class="setup-pat-toggle" onclick={() => (showTokenInput = true)}>
              Enter API Key manually
            </button>
          {:else}
            <div class="setup-input-wrap">
              <input
                class="setup-input"
                type="password"
                placeholder="lin_api_…"
                bind:value={tokenInput}
                onkeydown={(e) => e.key === 'Enter' && saveToken()}
              />
              <button
                class="setup-btn"
                onclick={saveToken}
                disabled={tokenSaving || !tokenInput.trim()}
              >
                {tokenSaving ? '…' : 'Save'}
              </button>
            </div>
            <p class="setup-hint" style="font-size:10px">
              Generate at <code>linear.app → Settings → API → Personal API keys</code>
            </p>
          {/if}

          {#if tokenError}
            <p class="setup-error">{tokenError}</p>
          {/if}
          <button class="setup-back-btn" onclick={() => selectTracker('')}>← Change tracker</button>
        </div>
      {/if}

    {:else if viewState === 'loading'}
      <div class="state-view">
        <Loader size={22} class="state-icon spin" />
        <p class="state-hint">Loading…</p>
      </div>

    {:else if viewState === 'error'}
      <div class="state-view">
        <AlertCircle size={26} class="state-icon state-warn" />
        <p class="state-title">Failed to load</p>
        <p class="state-hint state-error-text">{issuesStore.error}</p>
        <button class="retry-btn" onclick={() => issuesStore.loadIssues()}>
          <RefreshCw size={12} /> Retry
        </button>
      </div>

    {:else if viewState === 'empty'}
      <div class="state-view">
        <Circle size={26} class="state-icon state-muted" />
        <p class="state-title">No issues found</p>
        <p class="state-hint">
          {hasActiveFilters ? 'Try changing the filters.' : 'Nothing here yet.'}
        </p>
      </div>

    {:else}
      <ul class="is-list" role="list">
        {#each issuesStore.sortedIssues as issue (issue.id)}
          <li>
            <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
            <button
              class="is-item"
              onclick={() => issuesStore.selectAndLoadIssue(issue)}
              oncontextmenu={(e) => onIssueContextMenu(e, issue)}
              use:tooltip={issue.title}
            >
              <!-- Status icon column -->
              <span
                class="is-status-icon"
                use:tooltip={`${issue.status.name} (${issue.status.statusType})`}
              >
                <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                {@html statusIcon(issue.status.statusType, issue.status.color)}
              </span>

              <div class="is-item-content">
                <div class="is-item-top">
                  <span class="is-item-title">{issue.title}</span>
                  <span class="is-time">{timeAgo(issue.updatedAt)}</span>
                </div>
                <div class="is-item-bottom">
                  <span class="is-identifier">{issue.identifier}</span>
                  <span class="is-item-sep">·</span>
                  <span class="is-status-name">{issue.status.name}</span>
                  {#each issue.labels.slice(0, 2) as lbl}
                    <span class="is-label" style={labelChipStyle(lbl.color)}>{lbl.name}</span>
                  {/each}
                  {#if issue.labels.length > 2}
                    <span class="is-label-more">+{issue.labels.length - 2}</span>
                  {/if}
                </div>
              </div>
              <!-- Assignee avatar lives OUTSIDE .is-item-content so it stays
                   pinned to the right edge of the row regardless of how many
                   labels / how long the status name are.  Putting it inside
                   .is-item-bottom with `margin-left: auto` caused the avatar to
                   be pushed past the sidebar boundary when the bottom row's
                   total fixed-width children exceeded the container (all of
                   them had `flex-shrink: 0`, so nothing compressed and the
                   avatar was clipped by `overflow: hidden`). -->
              {#if issue.assignee}
                <span class="is-assignee-slot">
                  {#if issue.assignee.avatarUrl}
                    <img class="is-avatar" src={issue.assignee.avatarUrl} alt="" use:tooltip={issue.assignee.displayName} />
                  {:else}
                    <span class="is-avatar-placeholder" use:tooltip={issue.assignee.displayName}>
                      {(issue.assignee.displayName ?? issue.assignee.email ?? '?')[0]}
                    </span>
                  {/if}
                </span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}

  </div>

  <!-- Footer: user info + disconnect.
       Only show when the CURRENT repo has a tracker configured AND we're
       authenticated for it. Without the trackerForRepo guard, switching to a
       repo without an issue tracker would still surface the previous repo's
       provider footer (the auth state lives on the global issuesStore and
       persists across tabs). -->
  {#if trackerForRepo && issuesStore.authStatus?.authenticated && issuesStore.authStatus.user}
    <div class="is-footer">
      <span class="is-footer-user">
        {issuesStore.authStatus.user.displayName} · {issuesStore.activeProvider === 'jira' ? 'Jira' : 'Linear'}
      </span>
      <button class="is-footer-logout" onclick={() => issuesStore.logout()} use:tooltip={`Disconnect ${issuesStore.activeProvider === 'jira' ? 'Jira' : 'Linear'}`}>
        Disconnect
      </button>
    </div>
  {/if}
</PanelShell>

<style>
  .hdr-filter-badge {
    position: absolute; top: 3px; right: 3px;
    width: 5px; height: 5px; border-radius: 50%;
    background: var(--accent);
  }

  /* ── Sort dropdown ──────────────────────────────────────────────────────── */
  .sort-drop-section {
    padding: 4px 8px 2px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .sort-drop-sep { height: 1px; background: var(--border-subtle); margin: 4px 4px; }
  .sort-item { display: flex; align-items: center; gap: 6px; }
  .sort-label { flex: 1; }
  .sort-dir-icon { display: flex; align-items: center; color: var(--accent); }

  /* ── Filter options error banner ────────────────────────────────────────── */
  .filter-opts-error {
    display: flex; align-items: center; gap: 5px; flex-shrink: 0;
    padding: 4px 8px; font-size: 10px;
    background: rgba(248,113,113,0.08); border-bottom: 1px solid rgba(248,113,113,0.2);
    color: var(--error);
  }
  .filter-opts-error span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .filter-opts-error button {
    flex-shrink: 0; background: transparent; border: 1px solid currentColor;
    color: inherit; font-size: 9px; padding: 1px 5px; border-radius: var(--radius-sm);
    cursor: pointer; font-family: var(--font-ui-sans);
  }

  /* ── Search ──────────────────────────────────────────────────────────────── */
  .is-search-row {
    padding: 6px 8px 0;
    flex-shrink: 0;
  }
  .is-search-wrap {
    display: flex; align-items: center; gap: 5px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 0 6px;
  }
  :global(.is-search-icon) { color: var(--text-muted); flex-shrink: 0; }
  .is-search {
    flex: 1; border: none; background: transparent;
    font-size: 11px; font-family: var(--font-ui-sans);
    color: var(--text-primary); padding: 5px 0; outline: none;
  }
  .is-search::placeholder { color: var(--text-muted); }
  .is-search-clear {
    border: none; background: transparent; color: var(--text-muted);
    cursor: pointer; display: flex; align-items: center; padding: 0;
  }
  .is-search-clear:hover { color: var(--text-secondary); }

  /* ── Filter chips ────────────────────────────────────────────────────────── */
  .is-chips {
    display: flex; flex-wrap: wrap; gap: 4px;
    padding: 5px 8px 6px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .chip {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 3px 7px;
    font-size: 10px; font-weight: 500;
    font-family: var(--font-ui-sans);
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 99px; cursor: pointer;
    transition: all var(--transition-fast);
    white-space: nowrap;
  }
  .chip:hover { border-color: var(--border); color: var(--text-secondary); }
  .chip-active {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
  .chip-clear { background: transparent; border-color: transparent; }
  .chip-clear:hover { background: var(--bg-hover); border-color: var(--border-subtle); }
  .chip-drop-group {
    padding: 5px 8px 2px;
    font-size: 9px; font-weight: 600; letter-spacing: 0.5px;
    text-transform: uppercase; color: var(--text-muted);
  }
  .chip-drop-item {
    display: flex; align-items: center; gap: 6px;
    width: 100%; padding: 5px 8px; text-align: left;
    font-size: 11px; font-family: var(--font-ui-sans);
    color: var(--text-primary); background: transparent; border: none;
    border-radius: var(--radius-sm); cursor: pointer;
    transition: background var(--transition-fast);
  }
  .chip-drop-item:hover { background: var(--bg-hover); }
  .chip-drop-selected { color: var(--accent); }
  .chip-drop-empty {
    display: flex; align-items: center; gap: 6px; justify-content: center;
    padding: 10px 8px; font-size: 11px; color: var(--text-muted); font-style: italic;
  }
  .chip-drop-error {
    display: flex; flex-direction: column; gap: 6px;
    padding: 8px 10px; font-size: 10px;
    color: var(--error); font-family: var(--font-code);
    word-break: break-word;
  }
  .chip-drop-retry {
    align-self: flex-start; font-size: 10px; font-weight: 500;
    padding: 2px 8px; background: var(--bg-hover); border: 1px solid var(--border);
    border-radius: var(--radius-sm); color: var(--text-secondary);
    cursor: pointer; font-family: var(--font-ui-sans);
  }
  .chip-drop-retry:hover { background: var(--bg-overlay); }
  .status-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .team-key {
    font-family: var(--font-code); font-size: 10px;
    color: var(--text-muted); background: var(--bg-elevated);
    padding: 0 4px; border-radius: var(--radius-sm);
  }
  .check { margin-left: auto; font-size: 11px; }
  .ms-date-chip {
    margin-left: auto; font-size: 10px;
    color: var(--text-muted); font-variant-numeric: tabular-nums;
  }

  /* ── Body ─────────────────────────────────────────────────────────────────── */
  .is-body {
    flex: 1; overflow-y: auto; overflow-x: hidden;
    display: flex; flex-direction: column;
  }

  /* ── State views ─────────────────────────────────────────────────────────── */
  .state-view {
    display: flex; flex-direction: column; align-items: center;
    justify-content: center; flex: 1; gap: 8px;
    padding: 32px 24px; text-align: center;
  }
  .state-title { font-size: 13px; font-weight: 600; color: var(--text-primary); margin: 0; }
  .state-hint { font-size: 12px; color: var(--text-muted); line-height: 1.5; max-width: 220px; margin: 0; }
  .state-error-text { font-family: var(--font-code); font-size: 11px; color: var(--error); word-break: break-word; }
  :global(.state-icon)  { opacity: 0.5; }
  :global(.state-warn)  { opacity: 1; color: var(--status-warning, #fbbf24) !important; }
  :global(.state-muted) { color: var(--text-disabled) !important; }
  .retry-btn {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 5px 12px; font-size: 11px; font-weight: 500;
    background: transparent; border: 1px solid var(--border);
    border-radius: var(--radius-md); color: var(--text-secondary);
    cursor: pointer; font-family: var(--font-ui-sans); margin-top: 4px;
    transition: background var(--transition-fast);
  }
  .retry-btn:hover { background: var(--bg-hover); }

  /* ── Setup view ──────────────────────────────────────────────────────────── */
  .setup-view {
    display: flex; flex-direction: column; align-items: center;
    padding: 28px 20px; gap: 10px; text-align: center;
  }
  :global(.setup-icon) { color: var(--accent); opacity: 0.8; }
  .setup-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0; }
  .setup-hint {
    font-size: 11px; color: var(--text-muted); line-height: 1.55;
    max-width: 220px; margin: 0;
  }
  .setup-hint code { font-family: var(--font-code); font-size: 10px; color: var(--accent); }
  .setup-input-wrap { display: flex; gap: 6px; width: 100%; max-width: 240px; }
  .setup-input {
    flex: 1; padding: 6px 8px; font-size: 11px;
    font-family: var(--font-code);
    background: var(--bg-elevated); border: 1px solid var(--border);
    border-radius: var(--radius-md); color: var(--text-primary); outline: none;
    transition: border-color var(--transition-fast);
  }
  .setup-input:focus { border-color: var(--accent); }
  .setup-btn {
    padding: 6px 12px; font-size: 11px; font-weight: 600;
    background: var(--accent); color: var(--bg-base);
    border: none; border-radius: var(--radius-md); cursor: pointer;
    transition: background var(--transition-fast);
    white-space: nowrap;
  }
  .setup-btn:hover { background: var(--accent-hover); }
  .setup-btn:disabled { opacity: 0.5; cursor: default; }
  .setup-error { font-size: 10px; color: var(--error); max-width: 240px; }
  .setup-oauth-btn {
    display: flex; align-items: center; justify-content: center; gap: 6px;
    width: 100%; max-width: 240px;
    padding: 8px 14px; font-size: 12px; font-weight: 600;
    background: var(--accent); color: var(--text-on-accent);
    border: none; border-radius: var(--radius-md); cursor: pointer;
    font-family: var(--font-ui-sans);
    transition: background var(--transition-fast);
  }
  .setup-oauth-btn:hover:not(:disabled) { background: var(--accent-hover); }
  .setup-oauth-btn:disabled { opacity: 0.55; cursor: not-allowed; }

  .setup-divider {
    display: flex; align-items: center; gap: 8px;
    width: 100%; max-width: 240px; color: var(--text-disabled);
    font-size: 10px; text-transform: uppercase; letter-spacing: 0.04em;
  }
  .setup-divider::before, .setup-divider::after {
    content: ''; flex: 1; height: 1px; background: var(--border-subtle);
  }

  .setup-pat-toggle {
    font-size: 11px; color: var(--text-muted);
    background: transparent; border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md); padding: 4px 12px; cursor: pointer;
    font-family: var(--font-ui-sans);
    transition: all var(--transition-fast);
  }
  .setup-pat-toggle:hover { color: var(--text-secondary); border-color: var(--border); }

  .setup-back-btn {
    font-size: 10px; color: var(--text-muted);
    background: transparent; border: none; cursor: pointer;
    font-family: var(--font-ui-sans); padding: 2px 4px;
    margin-top: 4px;
    transition: color var(--transition-fast);
  }
  .setup-back-btn:hover { color: var(--text-secondary); }

  /* Setup-flow brand tiles render via the shared BrandTile widget. */
  /* Brand-coloured CTA buttons use absolute #fff foreground (theme-independent). */
  .jira-connect-btn { background: #0052cc !important; color: #fff !important; }
  .jira-connect-btn:hover:not(:disabled) { background: #003e99 !important; }

  .setup-jira-form {
    display: flex; flex-direction: column; gap: 6px;
    width: 100%; max-width: 240px; align-items: stretch;
  }
  .setup-token-row { display: flex; gap: 0; }
  .setup-token-row .setup-input { border-radius: var(--radius-md) 0 0 var(--radius-md); flex: 1; }
  .setup-eye-btn {
    display: flex; align-items: center; justify-content: center;
    width: 28px; background: var(--bg-elevated); border: 1px solid var(--border);
    border-left: none; border-radius: 0 var(--radius-md) var(--radius-md) 0;
    cursor: pointer; color: var(--text-muted); flex-shrink: 0;
    transition: color var(--transition-fast);
  }
  .setup-eye-btn:hover { color: var(--text-primary); }
  .setup-row { display: flex; align-items: center; gap: 8px; justify-content: center; flex-wrap: wrap; }

  /* ── Tracker selector ────────────────────────────────────────────────────── */
  .tracker-option {
    display: flex; align-items: center; gap: 10px;
    width: 200px; padding: 10px 14px;
    background: var(--bg-base); border: 1px solid var(--border);
    border-radius: var(--radius-md); cursor: pointer;
    font-family: var(--font-ui-sans); font-size: 12px; font-weight: 500;
    color: var(--text-primary);
    transition: border-color var(--transition-fast), background var(--transition-fast);
  }
  .tracker-option:hover:not(:disabled) { border-color: var(--accent); background: var(--accent-subtle); }
  /* Tracker logos render via the shared BrandTile widget. */
  .tracker-name { flex: 1; text-align: left; }

  /* ── Issue list (cards) ──────────────────────────────────────────────────── */
  .is-list { list-style: none; padding: 6px; margin: 0; display: flex; flex-direction: column; gap: 4px; }
  .is-item {
    display: flex; align-items: flex-start; gap: 0;
    width: 100%; padding: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    text-align: left; cursor: pointer;
    overflow: hidden;
    transition: background var(--transition-fast), border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .is-item:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0,0,0,0.15);
  }

  /* Status icon column */
  .is-status-icon {
    width: 36px; flex-shrink: 0;
    display: flex; align-items: center; justify-content: center;
    padding-top: 10px;
    align-self: flex-start;
  }
  :global(.is-status-icon svg) { display: block; }

  .is-item-content {
    flex: 1; min-width: 0;
    display: flex; flex-direction: column; gap: 3px;
    padding: 8px 10px 8px 0;
  }

  /* Row 1: title + time */
  .is-item-top {
    display: flex; align-items: baseline; gap: 6px;
  }
  .is-item-title {
    flex: 1; min-width: 0;
    font-size: var(--font-size-sm); font-weight: 500;
    color: var(--text-primary); line-height: 1.35;
    overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .is-time {
    flex-shrink: 0;
    font-size: 9px; color: var(--text-muted); white-space: nowrap;
  }

  /* Ticket code — stays in the bottom metadata row, but accent-coloured so
     the key reference for commits/branches/PRs still pops out at a glance. */
  .is-identifier {
    flex-shrink: 0;
    font-family: var(--font-code);
    font-size: 9px;
    font-weight: 600;
    color: var(--accent);
    white-space: nowrap;
    letter-spacing: 0.2px;
    text-transform: uppercase;
  }

  /* Row 2: identifier · status · labels · assignee */
  .is-item-bottom {
    display: flex; align-items: center; gap: 4px;
    overflow: hidden;
  }
  .is-item-sep { font-size: 9px; color: var(--text-disabled); flex-shrink: 0; }
  .is-status-name {
    font-size: 9px; color: var(--text-muted); white-space: nowrap; flex-shrink: 0;
    max-width: 70px; overflow: hidden; text-overflow: ellipsis;
  }
  .is-label {
    font-size: 10px; font-weight: 500;
    padding: 1px 5px; border-radius: var(--radius-sm); white-space: nowrap; flex-shrink: 0;
    max-width: 80px; overflow: hidden; text-overflow: ellipsis;
  }
  .is-label-more {
    font-size: 10px; color: var(--text-muted); flex-shrink: 0;
  }
  .is-assignee-slot {
    /* Pinned to the right edge of the flex row (.is-item) — not inside the
       overflow-hidden bottom row. flex-shrink: 0 keeps it at 14px even when
       the title/labels take up all other space. */
    flex-shrink: 0;
    display: flex; align-items: center;
    padding: 0 8px;
    align-self: center;
  }
  .is-avatar { width: 14px; height: 14px; border-radius: 50%; object-fit: cover; }
  .is-avatar-placeholder {
    width: 14px; height: 14px; border-radius: 50%;
    background: var(--accent-subtle); color: var(--accent);
    font-size: 8px; font-weight: 700;
    display: flex; align-items: center; justify-content: center;
  }

  /* ── Footer ──────────────────────────────────────────────────────────────── */
  .is-footer {
    display: flex; align-items: center; justify-content: space-between;
    padding: 6px 10px;
    border-top: 2px solid var(--border);
    background: var(--bg-base);
    flex-shrink: 0;
  }
  .is-footer-user { font-size: 10px; color: var(--text-muted); }
  .is-footer-logout {
    font-size: 10px; color: var(--text-muted);
    background: transparent; border: none; cursor: pointer;
    padding: 2px 6px; border-radius: var(--radius-sm);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .is-footer-logout:hover { background: var(--bg-hover); color: var(--error); }

</style>

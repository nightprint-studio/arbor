<script lang="ts">
  import {
    X, Search, Filter, User, Loader, TicketCheck, Circle,
    ArrowUpDown, ArrowUp, ArrowDown,
  } from 'lucide-svelte';
  import type { IssueSortField, IssueSortDir } from '$lib/types/issues';
  import { SORT_FIELD_LABELS } from '$lib/types/issues';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import FilterButton from '$lib/components/shared/ui/FilterButton.svelte';
  import Dropdown from '$lib/components/shared/ui/Dropdown.svelte';
  import {
    linearSearchIssues, linearGetFilterOptions,
    jiraSearchIssues,   jiraGetFilterOptions,
  } from '$lib/ipc/issues';
  import { issuesStore } from '$lib/stores/issues.svelte';
  import { tabsStore } from '$lib/stores/tabs.svelte';
  import { getRepoConfig } from '$lib/ipc/config';
  import type { Issue, IssueFilters, IssueFilterOptions, IssueStatus } from '$lib/types/issues';
  import { tooltip } from '$lib/actions/tooltip';

  let {
    onSelect,
    onClose,
  }: {
    onSelect: (issue: Issue) => void;
    onClose:  () => void;
  } = $props();

  // ── Active provider: resolved from repo config, falls back to issues store ──
  let provider = $state<'linear' | 'jira'>(
    (issuesStore.activeProvider as 'linear' | 'jira' | null) ?? 'linear'
  );

  // ── Filter / search state ────────────────────────────────────────────────
  let query         = $state('');
  let queryEl: HTMLInputElement | undefined = $state();
  $effect(() => { queryEl?.focus(); });
  let statusIds     = $state<string[]>([]);
  let issueTypeIds  = $state<string[]>([]);
  let teamId        = $state<string | undefined>(undefined);
  let projectId     = $state<string | undefined>(undefined);
  let milestoneId   = $state<string | undefined>(undefined);
  let assigneeMe    = $state(false);

  // ── Results state ────────────────────────────────────────────────────────
  let issues       = $state<Issue[]>([]);
  let loading      = $state(false);
  let error        = $state<string | null>(null);

  // ── Filter options ───────────────────────────────────────────────────────
  let filterOptions     = $state<IssueFilterOptions | null>(null);
  let filterOptsLoading = $state(false);

  // ── Sort state (initialized from global store default) ───────────────────
  let sortField = $state<IssueSortField>(issuesStore.sortField);
  let sortDir   = $state<IssueSortDir>(issuesStore.sortDir);

  const SORT_FIELDS: IssueSortField[] = ['ticket_id', 'updated_at', 'created_at', 'priority', 'title', 'status'];

  function compareIdentifier(a: string, b: string): number {
    const re = /^(.*?)(\d+)$/;
    const ma = re.exec(a);
    const mb = re.exec(b);
    if (ma && mb) {
      const prefixCmp = ma[1].localeCompare(mb[1]);
      if (prefixCmp !== 0) return prefixCmp;
      return parseInt(ma[2], 10) - parseInt(mb[2], 10);
    }
    return a.localeCompare(b);
  }

  const sortedIssues = $derived((() => {
    const arr = [...issues];
    const dir = sortDir === 'asc' ? 1 : -1;
    arr.sort((a, b) => {
      switch (sortField) {
        case 'ticket_id':  return dir * compareIdentifier(a.identifier, b.identifier);
        case 'updated_at': return dir * (new Date(a.updatedAt).getTime() - new Date(b.updatedAt).getTime());
        case 'created_at': return dir * (new Date(a.createdAt).getTime() - new Date(b.createdAt).getTime());
        case 'priority':   return dir * (a.priority - b.priority);
        case 'title':      return dir * a.title.localeCompare(b.title);
        case 'status': {
          const order = ['backlog','unstarted','started','completed','cancelled'];
          const ai = order.indexOf(a.status.statusType);
          const bi = order.indexOf(b.status.statusType);
          return dir * ((ai === -1 ? 99 : ai) - (bi === -1 ? 99 : bi));
        }
        default: return 0;
      }
    });
    return arr;
  })());

  // ── Load filter options + initial search once provider is resolved ──────────
  $effect(() => {
    const tabId = tabsStore.activeTab?.id;
    const init = async () => {
      if (tabId) {
        try {
          const cfg = await getRepoConfig(tabId);
          const t = cfg.issue_tracker;
          if (t === 'linear' || t === 'jira') provider = t;
          if (cfg.issue_tracker_project_id) {
            if (t === 'jira' && !teamId) {
              teamId = cfg.issue_tracker_project_id;
            } else if (t !== 'jira' && !projectId) {
              projectId = cfg.issue_tracker_project_id;
            }
          }
        } catch { /* use default */ }
      }
      await Promise.all([loadFilterOpts(), search()]);
    };
    void init();
  });

  // ── Re-search whenever filters change ────────────────────────────────────
  let searchTimer: ReturnType<typeof setTimeout> | null = null;
  function scheduleSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(search, 200);
  }

  async function loadFilterOpts() {
    filterOptsLoading = true;
    try {
      filterOptions = provider === 'jira'
        ? await jiraGetFilterOptions()
        : await linearGetFilterOptions();
    }
    catch { /* non-fatal */ }
    finally { filterOptsLoading = false; }
  }

  async function search() {
    loading = true;
    error   = null;
    const filters: IssueFilters = {
      query:        query.trim() || undefined,
      statusIds:    statusIds.length     ? statusIds    : undefined,
      issueTypeIds: issueTypeIds.length  ? issueTypeIds : undefined,
      teamId,
      projectId,
      milestoneId,
      assigneeMe:   assigneeMe || undefined,
      limit:        30,
    };
    try {
      issues = provider === 'jira'
        ? await jiraSearchIssues(filters)
        : await linearSearchIssues(filters);
    } catch (e) {
      error  = String(e).replace(/^Error: /, '');
      issues = [];
    } finally {
      loading = false;
    }
  }

  // ── Filter helpers ────────────────────────────────────────────────────────
  function toggleStatus(id: string) {
    statusIds = statusIds.includes(id)
      ? statusIds.filter(s => s !== id)
      : [...statusIds, id];
    scheduleSearch();
  }

  function hasActiveFilters() {
    return statusIds.length > 0 || issueTypeIds.length > 0 || !!teamId || !!projectId || !!milestoneId || assigneeMe;
  }

  function clearFilters() {
    statusIds = []; issueTypeIds = []; teamId = undefined; projectId = undefined;
    milestoneId = undefined; assigneeMe = false;
    query = '';
    scheduleSearch();
  }

  // Group statuses by type. Fallback: derive from loaded issues if API returns none.
  const statusGroups = $derived((() => {
    let opts = filterOptions?.statuses ?? [];
    if (opts.length === 0 && issues.length > 0) {
      const seen = new Map<string, IssueStatus>();
      for (const issue of issues) {
        if (!seen.has(issue.status.id)) seen.set(issue.status.id, issue.status);
      }
      opts = [...seen.values()];
    }
    const order = ['backlog', 'unstarted', 'started', 'completed', 'cancelled'];
    const map: Record<string, IssueStatus[]> = {};
    for (const s of opts) {
      const t = s.statusType ?? 'backlog';
      (map[t] ??= []).push(s);
    }
    return order.filter(t => map[t]?.length).map(t => ({ type: t, items: map[t] }));
  })());

  // Teams: API result or derived from loaded issues.
  const effectiveTeams = $derived((() => {
    const teams = filterOptions?.teams ?? [];
    if (teams.length > 0) return teams;
    const seen = new Map<string, { id: string; name: string; key: string }>();
    for (const issue of issues) {
      if (issue.team && !seen.has(issue.team.id)) seen.set(issue.team.id, issue.team);
    }
    return [...seen.values()];
  })());

  // Issue types (Jira only).
  const issueTypeOptions = $derived(filterOptions?.issueTypes ?? []);

  const teamLabel = $derived(provider === 'jira' ? 'Project' : 'Team');

  const milestoneGroups = $derived((() => {
    if (!filterOptions) return [];
    const map: Record<string, typeof filterOptions.milestones> = {};
    for (const m of filterOptions.milestones) {
      const key = m.projectName ?? 'Other';
      (map[key] ??= []).push(m);
    }
    return Object.entries(map).map(([name, items]) => ({ name, items }));
  })());

  // ── Rendering helpers (mirrored from IssuesSidebar) ───────────────────────
  function statusIcon(statusType: string, color: string): string {
    const c = color || '#6b7280';
    const sw = '1.8';
    if (statusType === 'completed')
      return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6.5" fill="${c}"/><polyline points="4.5,7.5 6.5,9.5 10.5,5" fill="none" stroke="white" stroke-width="${sw}" stroke-linecap="round" stroke-linejoin="round"/></svg>`;
    if (statusType === 'started')
      return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/><path d="M7.5,1.5 A6,6 0 0,1 7.5,13.5 L7.5,7.5 Z" fill="${c}"/></svg>`;
    if (statusType === 'cancelled')
      return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/><line x1="5" y1="5" x2="10" y2="10" stroke="${c}" stroke-width="${sw}" stroke-linecap="round"/><line x1="10" y1="5" x2="5" y2="10" stroke="${c}" stroke-width="${sw}" stroke-linecap="round"/></svg>`;
    if (statusType === 'unstarted')
      return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}"/></svg>`;
    return `<svg xmlns="http://www.w3.org/2000/svg" width="15" height="15" viewBox="0 0 15 15"><circle cx="7.5" cy="7.5" r="6" fill="none" stroke="${c}" stroke-width="${sw}" stroke-dasharray="3.5 2.5"/></svg>`;
  }

  function labelChipStyle(color: string): string {
    const hex = color.startsWith('#') ? color : `#${color}`;
    if (hex.length < 7) return '';
    const r = parseInt(hex.slice(1, 3), 16) / 255;
    const g = parseInt(hex.slice(3, 5), 16) / 255;
    const b = parseInt(hex.slice(5, 7), 16) / 255;
    const lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    if (lum < 0.1) return `background:rgba(160,160,160,0.12);color:var(--text-secondary);border:1px solid rgba(160,160,160,0.25)`;
    return `background:${hex}22;color:${hex};border:1px solid ${hex}55`;
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
</script>

<Modal {onClose} width="540px" padBody={false} ariaLabel="Pick a ticket">
  {#snippet header()}
    <ModalHeader {onClose}>
      <TicketCheck size={13} class="tpm-header-icon" />
      <span class="modal-title">Pick a {provider === 'jira' ? 'Jira issue' : 'Linear issue'}</span>
    </ModalHeader>
  {/snippet}

  <div class="tpm-shell">
  <!-- ── Search bar ── -->
  <div class="tpm-search-wrap">
    <Search size={12} class="tpm-search-icon" />
    <input
      class="tpm-search"
      type="text"
      placeholder="Search by title or ID…"
      bind:value={query}
      oninput={scheduleSearch}
      bind:this={queryEl}
      autocomplete="off"
    />
    {#if query}
      <button class="tpm-search-clear" onclick={() => { query = ''; scheduleSearch(); }}><X size={11} /></button>
    {/if}
  </div>

  <!-- ── Filter chips ── -->
  <div class="tpm-chips">
    <!-- Assigned to me -->
    <button
      class="chip"
      class:chip-active={assigneeMe}
      onclick={() => { assigneeMe = !assigneeMe; scheduleSearch(); }}
    >
      <User size={10} /> Me
    </button>

    <!-- Status -->
    <FilterButton
      label="Status"
      icon={Filter}
      count={statusIds.length}
      loading={filterOptsLoading}
    >
      {#snippet children({ close })}
        {#if statusGroups.length === 0}
          <div class="chip-drop-empty">No statuses</div>
        {:else}
          {#each statusGroups as grp}
            <div class="chip-drop-group">{grp.type}</div>
            {#each grp.items as st}
              <button class="chip-drop-item" class:chip-drop-selected={statusIds.includes(st.id)}
                      onclick={() => toggleStatus(st.id)}>
                <span class="status-dot" style="background:{st.color}"></span>
                {st.name}
                {#if statusIds.includes(st.id)}<span class="check">✓</span>{/if}
              </button>
            {/each}
          {/each}
        {/if}
      {/snippet}
    </FilterButton>

    <!-- Team (Linear) / Project (Jira) filter -->
    {#if effectiveTeams.length > 0}
      <FilterButton
        label={effectiveTeams.find(t => t.id === teamId)?.name ?? teamLabel}
        active={!!teamId}
        wide={true}
        searchable={effectiveTeams.length > 5}
        searchPlaceholder="Filter {teamLabel.toLowerCase()}s…"
      >
        {#snippet children({ filter, close })}
          {@const q = filter.trim().toLowerCase()}
          {@const filtered = q
            ? effectiveTeams.filter(t =>
                t.name.toLowerCase().includes(q) ||
                t.key.toLowerCase().includes(q))
            : effectiveTeams}
          {#if !q}
            <button class="chip-drop-item" onclick={() => { teamId = undefined; close(); scheduleSearch(); }}>
              All {teamLabel.toLowerCase()}s {!teamId ? '✓' : ''}
            </button>
          {/if}
          {#each filtered as team}
            <button class="chip-drop-item" class:chip-drop-selected={teamId === team.id}
                    onclick={() => { teamId = team.id; close(); scheduleSearch(); }}>
              <span class="team-key">{team.key}</span> {team.name}
              {#if teamId === team.id}<span class="check">✓</span>{/if}
            </button>
          {:else}
            <div class="chip-drop-empty">No results</div>
          {/each}
        {/snippet}
      </FilterButton>
    {/if}

    <!-- Issue Type filter (Jira only) -->
    {#if issueTypeOptions.length > 0}
      <FilterButton label="Type" count={issueTypeIds.length}>
        {#snippet children({ close })}
          {#each issueTypeOptions as it}
            <button class="chip-drop-item" class:chip-drop-selected={issueTypeIds.includes(it.id)}
                    onclick={() => {
                      issueTypeIds = issueTypeIds.includes(it.id)
                        ? issueTypeIds.filter(t => t !== it.id)
                        : [...issueTypeIds, it.id];
                      scheduleSearch();
                    }}>
              <span class="status-dot" style="background:{it.color}"></span>
              {it.name}
              {#if issueTypeIds.includes(it.id)}<span class="check">✓</span>{/if}
            </button>
          {/each}
        {/snippet}
      </FilterButton>
    {/if}

    <!-- Project (Linear) -->
    {#if (filterOptions?.projects?.length ?? 0) > 0}
      <FilterButton
        label={filterOptions?.projects.find(p => p.id === projectId)?.name ?? 'Project'}
        active={!!projectId}
      >
        {#snippet children({ close })}
          <button class="chip-drop-item" onclick={() => { projectId = undefined; close(); scheduleSearch(); }}>
            All projects {!projectId ? '✓' : ''}
          </button>
          {#each filterOptions?.projects ?? [] as proj}
            <button class="chip-drop-item" class:chip-drop-selected={projectId === proj.id}
                    onclick={() => { projectId = proj.id; close(); scheduleSearch(); }}>
              {#if proj.color}<span class="status-dot" style="background:{proj.color}"></span>{/if}
              {proj.name}
              {#if projectId === proj.id}<span class="check">✓</span>{/if}
            </button>
          {/each}
        {/snippet}
      </FilterButton>
    {/if}

    <!-- Milestone -->
    {#if (filterOptions?.milestones?.length ?? 0) > 0}
      <FilterButton
        label={filterOptions?.milestones.find(m => m.id === milestoneId)?.name ?? 'Milestone'}
        active={!!milestoneId}
      >
        {#snippet children({ close })}
          <button class="chip-drop-item" onclick={() => { milestoneId = undefined; close(); scheduleSearch(); }}>
            All milestones {!milestoneId ? '✓' : ''}
          </button>
          {#each milestoneGroups as group}
            <div class="chip-drop-group">{group.name}</div>
            {#each group.items as ms}
              <button class="chip-drop-item" class:chip-drop-selected={milestoneId === ms.id}
                      onclick={() => { milestoneId = ms.id; close(); scheduleSearch(); }}>
                {ms.name}
                {#if ms.targetDate}<span class="ms-date">{ms.targetDate}</span>{/if}
                {#if milestoneId === ms.id}<span class="check">✓</span>{/if}
              </button>
            {/each}
          {/each}
        {/snippet}
      </FilterButton>
    {/if}

    <!-- Clear all -->
    {#if hasActiveFilters()}
      <button class="chip chip-clear" onclick={clearFilters} use:tooltip={'Clear filters'}><X size={9} /></button>
    {/if}

    <!-- Sort -->
    <div class="sort-dd-wrap">
      <Dropdown position="fixed" direction="down">
        {#snippet trigger({ open, toggle })}
          <button
            class="chip"
            class:chip-active={open}
            onclick={toggle}
            type="button"
            use:tooltip={`Sort: ${SORT_FIELD_LABELS[sortField]} (${sortDir === 'asc' ? '↑' : '↓'})`}
          >
            <ArrowUpDown size={10} />
            {SORT_FIELD_LABELS[sortField]}
            {#if sortDir === 'asc'}<ArrowUp size={8} />{:else}<ArrowDown size={8} />{/if}
          </button>
        {/snippet}
        {#snippet children({ close })}
          <div class="sort-section-label">Order by</div>
          {#each SORT_FIELDS as field}
            <button
              class="chip-drop-item sort-item"
              class:chip-drop-selected={sortField === field}
              onclick={() => {
                const newDir = sortField === field ? (sortDir === 'asc' ? 'desc' : 'asc') : sortDir;
                sortField = field;
                sortDir = newDir;
                issuesStore.setSort(field, newDir);
                close();
              }}
            >
              <span class="sort-label">{SORT_FIELD_LABELS[field]}</span>
              {#if sortField === field}
                <span class="sort-dir-icon">
                  {#if sortDir === 'asc'}<ArrowUp size={10} />{:else}<ArrowDown size={10} />{/if}
                </span>
              {/if}
            </button>
          {/each}
          <div class="sort-sep"></div>
          <div class="sort-section-label">Direction</div>
          <button class="chip-drop-item sort-item" class:chip-drop-selected={sortDir === 'asc'}
                  onclick={() => { sortDir = 'asc'; issuesStore.setSort(sortField, 'asc'); close(); }}>
            <ArrowUp size={10} /> Ascending
            {#if sortDir === 'asc'}<span class="check">✓</span>{/if}
          </button>
          <button class="chip-drop-item sort-item" class:chip-drop-selected={sortDir === 'desc'}
                  onclick={() => { sortDir = 'desc'; issuesStore.setSort(sortField, 'desc'); close(); }}>
            <ArrowDown size={10} /> Descending
            {#if sortDir === 'desc'}<span class="check">✓</span>{/if}
          </button>
        {/snippet}
      </Dropdown>
    </div>
  </div>

  <!-- ── Issue list ── -->
  <div class="tpm-body">
    {#if loading}
      <div class="tpm-state">
        <Loader size={20} class="spin tpm-state-icon" />
        <span>Loading…</span>
      </div>
    {:else if error}
      <div class="tpm-state tpm-state-error">
        <span>{error}</span>
        <button class="tpm-retry" onclick={search}>Retry</button>
      </div>
    {:else if issues.length === 0}
      <div class="tpm-state">
        <Circle size={20} class="tpm-state-icon tpm-state-muted" />
        <span>{query || hasActiveFilters() ? 'No results.' : 'No issues found.'}</span>
      </div>
    {:else}
      <ul class="tpm-list" role="list">
        {#each sortedIssues as issue (issue.id)}
          <li>
            <button class="tpm-item" onclick={() => onSelect(issue)} use:tooltip={issue.title}>
              <span class="tpm-status-icon" use:tooltip={issue.status.name}>
                <!-- eslint-disable-next-line svelte/no-at-html-tags -->
                {@html statusIcon(issue.status.statusType, issue.status.color)}
              </span>
              <div class="tpm-item-content">
                <div class="tpm-item-top">
                  <span class="tpm-item-title">{issue.title}</span>
                  <span class="tpm-time">{timeAgo(issue.updatedAt)}</span>
                </div>
                <div class="tpm-item-bottom">
                  <span class="tpm-identifier">{issue.identifier}</span>
                  <span class="tpm-sep">·</span>
                  <span class="tpm-status-name">{issue.status.name}</span>
                  {#each issue.labels.slice(0, 2) as lbl}
                    <span class="tpm-label" style={labelChipStyle(lbl.color)}>{lbl.name}</span>
                  {/each}
                  {#if issue.labels.length > 2}
                    <span class="tpm-label-more">+{issue.labels.length - 2}</span>
                  {/if}
                  {#if issue.assignee}
                    <span class="tpm-assignee">
                      {#if issue.assignee.avatarUrl}
                        <img class="tpm-avatar" src={issue.assignee.avatarUrl} alt="" use:tooltip={issue.assignee.displayName} />
                      {:else}
                        <span class="tpm-avatar-placeholder" use:tooltip={issue.assignee.displayName}>
                          {(issue.assignee.displayName ?? '?')[0]}
                        </span>
                      {/if}
                    </span>
                  {/if}
                </div>
              </div>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
  </div>
</Modal>

<style>
  /* ── Modal body shell ── */
  .tpm-shell {
    display: flex;
    flex-direction: column;
    height: 100%;
    font-family: var(--font-ui-sans);
    font-size: 12px;
    color: var(--text-primary);
  }
  :global(.tpm-header-icon) { color: var(--accent); flex-shrink: 0; }

  /* ── Search ── */
  .tpm-search-wrap {
    position: relative;
    display: flex;
    align-items: center;
    padding: 8px 12px 6px;
    flex-shrink: 0;
  }
  :global(.tpm-search-icon) {
    position: absolute;
    left: 20px;
    color: var(--text-muted);
    pointer-events: none;
  }
  .tpm-search {
    width: 100%;
    padding: 6px 28px 6px 30px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-ui-sans);
    font-size: 12px;
    color: var(--text-primary);
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .tpm-search:focus { border-color: var(--accent); }
  .tpm-search-clear {
    position: absolute;
    right: 20px;
    display: flex;
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
    padding: 2px;
  }
  .tpm-search-clear:hover { color: var(--text-primary); }

  /* ── Filter chips row ── */
  .tpm-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding: 0 12px 8px;
    flex-shrink: 0;
    align-items: center;
  }

  /* Plain chip (Me, Clear, Sort trigger) */
  .chip {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px;
    background: var(--bg-base);
    border: 1px solid var(--border);
    border-radius: 999px;
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-secondary);
    cursor: pointer;
    white-space: nowrap;
    transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
  }
  .chip:hover { background: var(--bg-hover); color: var(--text-primary); }
  .chip-active {
    background: var(--accent-subtle);
    border-color: var(--accent);
    color: var(--accent);
  }
  .chip-active:hover { background: var(--accent-subtle); }
  .chip-clear { padding: 3px 7px; }

  /* Sort dropdown wrapper (pushes to the right) */
  .sort-dd-wrap { margin-left: auto; }

  /* ── FilterButton / sort dropdown children content ── */
  .chip-drop-group {
    padding: 5px 10px 3px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--text-disabled);
  }
  .chip-drop-item {
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    padding: 6px 10px;
    background: transparent;
    border: none;
    font-family: var(--font-ui-sans);
    font-size: 11px;
    color: var(--text-primary);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast);
  }
  .chip-drop-item:hover { background: var(--bg-hover); }
  .chip-drop-selected { color: var(--accent); }

  .chip-drop-empty {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px;
    font-size: 11px;
    color: var(--text-muted);
  }

  /* Sort-specific content */
  .sort-section-label {
    padding: 4px 10px 2px;
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .sort-sep { height: 1px; background: var(--border-subtle); margin: 4px 8px; }
  .sort-item { gap: 6px; }
  .sort-label { flex: 1; }
  .sort-dir-icon { display: flex; align-items: center; color: var(--accent); }

  /* Shared chip content helpers */
  .status-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .team-key {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
  }
  .check { margin-left: auto; color: var(--accent); font-size: 11px; }
  .ms-date {
    margin-left: auto;
    font-size: 10px;
    color: var(--text-muted);
  }

  /* ── Body / list ──
     Floats as a --bg-base card inside the --bg-elevated modal chrome. */
  .tpm-body {
    flex: 1;
    overflow-y: auto;
    scrollbar-width: thin;
    scrollbar-color: var(--border) transparent;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-lg);
    margin: 0 4px 4px;
  }
  .tpm-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px 20px;
    color: var(--text-muted);
    font-size: 12px;
  }
  :global(.tpm-state-icon) { opacity: 0.4; }
  :global(.tpm-state-muted) { color: var(--text-disabled); }
  .tpm-state-error { color: var(--color-error, #f87171); }
  .tpm-retry {
    padding: 4px 10px;
    background: transparent;
    border: 1px solid currentColor;
    border-radius: var(--radius-sm);
    font-size: 11px;
    color: inherit;
    cursor: pointer;
    opacity: 0.8;
  }
  .tpm-retry:hover { opacity: 1; }

  .tpm-list {
    list-style: none;
    margin: 0;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  /* Ticket cards — raised on the --bg-base body card. */
  .tpm-item {
    display: flex;
    align-items: flex-start;
    gap: 0;
    width: 100%;
    padding: 7px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition-fast), border-color var(--transition-fast), box-shadow var(--transition-fast);
  }
  .tpm-item:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0,0,0,0.15);
  }
  .tpm-status-icon {
    width: 32px;
    display: flex;
    align-items: center;
    justify-content: flex-start;
    padding-top: 1px;
    flex-shrink: 0;
  }
  .tpm-item-content {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .tpm-item-top {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }
  .tpm-item-title {
    flex: 1;
    font-size: 12px;
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    line-height: 1.4;
  }
  .tpm-time {
    flex-shrink: 0;
    font-size: 10px;
    color: var(--text-disabled);
    padding-top: 1px;
  }
  .tpm-item-bottom {
    display: flex;
    align-items: center;
    gap: 5px;
    flex-wrap: wrap;
  }
  .tpm-identifier {
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .tpm-sep { color: var(--text-disabled); font-size: 10px; }
  .tpm-status-name {
    font-size: 10px;
    color: var(--text-muted);
    max-width: 80px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tpm-label {
    font-size: 10px;
    padding: 1px 5px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .tpm-label-more {
    font-size: 10px;
    color: var(--text-muted);
  }
  .tpm-assignee { margin-left: auto; }
  .tpm-avatar {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    object-fit: cover;
  }
  .tpm-avatar-placeholder {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--bg-hover);
    border: 1px solid var(--border);
    font-size: 9px;
    font-weight: 600;
    color: var(--text-muted);
  }

</style>

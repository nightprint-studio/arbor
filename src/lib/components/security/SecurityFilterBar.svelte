<script lang="ts">
  /**
   * Filter row for the Security panel — search + severity multiselect +
   * report-type multiselect + clear-all chip. Mirrors the IssuesSidebar
   * filter strip pattern (chips + `<FilterButton>` dropdowns) so the visual
   * language stays consistent across the app.
   *
   * Filter changes call `securityStore.loadFindings(tabId)` directly:
   *   - severity / report-type toggles fire immediately
   *   - search input is debounced 250ms (host-side substring match — the
   *     backend round-trip is cheap, but typing-rate refetches still feel
   *     wasteful)
   *
   * The counter grid + chart series + detail modal all read from the
   * filtered findings array exposed by the store, so they stay in sync
   * automatically whenever this bar mutates filters.
   */
  import { Filter, ShieldAlert, Search, X, Loader } from 'lucide-svelte';

  import FilterButton from '$lib/components/shared/ui/FilterButton.svelte';
  import { SEVERITY_META } from './severity-meta';

  import { securityStore } from '$lib/stores/security.svelte';
  import { SEVERITY_ORDER, type Severity } from '$lib/types/security';
  import { tooltip } from '$lib/actions/tooltip';

  interface Props {
    tabId: string;
  }

  let { tabId }: Props = $props();

  // Local mirror of the search input. Two-way bound to the input so the
  // user gets instant character feedback; debounced before pushing to the
  // store + triggering a re-fetch.
  let searchInput = $state(securityStore.filters.search ?? '');
  let searchTimer: ReturnType<typeof setTimeout> | null = null;

  function scheduleSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      const cur  = securityStore.filters.search ?? '';
      const next = searchInput.trim();
      if (cur === next || (cur === '' && next === '')) return;
      securityStore.setSearch(searchInput);
      securityStore.loadFindings(tabId);
    }, 250);
  }

  function clearSearch() {
    if (searchTimer) clearTimeout(searchTimer);
    searchInput = '';
    if ((securityStore.filters.search ?? '') !== '') {
      securityStore.setSearch('');
      securityStore.loadFindings(tabId);
    }
  }

  function toggleSeverity(sev: Severity) {
    securityStore.toggleSeverity(sev);
    securityStore.loadFindings(tabId);
  }

  function toggleReportType(rt: string) {
    securityStore.toggleReportType(rt);
    securityStore.loadFindings(tabId);
  }

  function clearAll() {
    if (searchTimer) clearTimeout(searchTimer);
    searchInput = '';
    securityStore.clearFilters();
    securityStore.loadFindings(tabId);
  }

  // Pretty-print provider report-type strings (snake_case → Title Case).
  // GitLab returns "dependency_scanning", "secret_detection", … ; GitHub
  // (Phase 6) will return "code_scanning" / "dependabot" / "secret_scanning".
  // Falling back to the raw token if it's already camelCased is safe.
  function formatReportType(rt: string): string {
    return rt
      .split(/[_\-]/)
      .filter(Boolean)
      .map(w => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase())
      .join(' ');
  }

  const severityCount   = $derived(securityStore.filters.severities.length);
  const reportTypeCount = $derived(securityStore.filters.report_types.length);
  const hasActive       = $derived(securityStore.hasActiveFilters());
  const reportOptions   = $derived(securityStore.availableReportTypes());

  // Sync the input when filters are cleared from elsewhere (e.g. tab
  // switch / external reset).
  $effect(() => {
    const fromStore = securityStore.filters.search ?? '';
    if (fromStore !== searchInput && document.activeElement?.tagName !== 'INPUT') {
      searchInput = fromStore;
    }
  });
</script>

<div class="sf-row" role="search" aria-label="Filter security findings">
  <!-- Search input — same compact look as the IssuesSidebar "is-search" -->
  <div class="sf-search">
    <Search size={11} class="sf-search-icon" />
    <input
      type="text"
      placeholder="Filter results…"
      bind:value={searchInput}
      oninput={scheduleSearch}
      aria-label="Search findings"
    />
    {#if searchInput}
      <button class="sf-search-clear" onclick={clearSearch} use:tooltip={'Clear search'}>
        <X size={11} />
      </button>
    {/if}
  </div>

  <div class="sf-chips">
    <!-- Severity multiselect -->
    <FilterButton
      label="Severity"
      icon={ShieldAlert}
      count={severityCount}
    >
      {#snippet children({ close: _close })}
        {#each SEVERITY_ORDER as sev (sev)}
          {@const meta = SEVERITY_META[sev]}
          {@const selected = securityStore.filters.severities.includes(sev)}
          <button
            class="chip-drop-item"
            class:chip-drop-selected={selected}
            onclick={() => toggleSeverity(sev)}
          >
            <span class="sev-dot" style:background={meta.color}></span>
            {meta.label}
            {#if selected}<span class="check">✓</span>{/if}
          </button>
        {/each}
      {/snippet}
    </FilterButton>

    <!-- Report type multiselect (options derived from currently loaded findings) -->
    <FilterButton
      label="Type"
      icon={Filter}
      count={reportTypeCount}
    >
      {#snippet children({ close: _close })}
        {#if reportOptions.length === 0}
          <div class="chip-drop-empty">No report types</div>
        {:else}
          {#each reportOptions as rt (rt)}
            {@const selected = securityStore.filters.report_types.includes(rt)}
            <button
              class="chip-drop-item"
              class:chip-drop-selected={selected}
              onclick={() => toggleReportType(rt)}
            >
              {formatReportType(rt)}
              {#if selected}<span class="check">✓</span>{/if}
            </button>
          {/each}
        {/if}
      {/snippet}
    </FilterButton>

    {#if hasActive}
      <button class="chip chip-clear" onclick={clearAll} use:tooltip={'Clear all filters'}>
        <X size={9} /> Clear
      </button>
    {/if}

    {#if securityStore.findingsLoading}
      <span class="sf-spinner" aria-hidden="true"><Loader size={11} class="spin" /></span>
    {/if}
  </div>
</div>

<style>
  .sf-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 12px 8px;
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }

  /* ── Search input ────────────────────────────────────────────────────── */
  .sf-search {
    position: relative;
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .sf-search input {
    flex: 1; width: 100%;
    padding: 4px 24px 4px 24px;
    font-size: 11px; font-family: var(--font-ui-sans);
    background: var(--bg-base);
    color: var(--text-primary);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    outline: none;
    transition: border-color var(--transition-fast);
  }
  .sf-search input:focus { border-color: var(--accent); }
  .sf-search input::placeholder { color: var(--text-muted); }
  :global(.sf-row .sf-search-icon) {
    position: absolute;
    left: 7px; top: 50%;
    transform: translateY(-50%);
    color: var(--text-muted);
    pointer-events: none;
  }
  .sf-search-clear {
    position: absolute;
    right: 4px; top: 50%;
    transform: translateY(-50%);
    background: transparent;
    border: none;
    padding: 2px;
    color: var(--text-muted);
    cursor: pointer;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: var(--radius-sm);
  }
  .sf-search-clear:hover { color: var(--text-secondary); background: var(--bg-hover); }

  /* ── Chip strip ──────────────────────────────────────────────────────── */
  .sf-chips {
    display: flex; flex-wrap: wrap; align-items: center; gap: 4px;
  }
  .chip {
    display: inline-flex; align-items: center; gap: 3px;
    padding: 3px 7px;
    font-size: 10px; font-weight: 500;
    font-family: var(--font-ui-sans);
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: 99px;
    cursor: pointer;
    transition: all var(--transition-fast);
    white-space: nowrap;
  }
  .chip-clear { background: transparent; border-color: transparent; }
  .chip-clear:hover { background: var(--bg-hover); border-color: var(--border-subtle); color: var(--text-secondary); }

  /* ── Dropdown items (re-defined per FilterButton consumer; styles
        from the widget's parent are not propagated to its children) ───── */
  :global(.sf-row .chip-drop-item) {
    display: flex; align-items: center; gap: 6px;
    width: 100%;
    padding: 5px 8px;
    text-align: left;
    font-size: 11px; font-family: var(--font-ui-sans);
    color: var(--text-primary);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background var(--transition-fast);
  }
  :global(.sf-row .chip-drop-item:hover) { background: var(--bg-hover); }
  :global(.sf-row .chip-drop-selected)   { color: var(--accent); }
  :global(.sf-row .chip-drop-empty) {
    display: flex; align-items: center; justify-content: center;
    padding: 10px 8px;
    font-size: 11px; color: var(--text-muted); font-style: italic;
  }
  :global(.sf-row .check) { margin-left: auto; font-size: 11px; }
  :global(.sf-row .sev-dot) {
    width: 8px; height: 8px;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .sf-spinner {
    display: inline-flex;
    align-items: center;
    color: var(--text-muted);
    margin-left: 2px;
  }
</style>

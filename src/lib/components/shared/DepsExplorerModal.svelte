<script lang="ts">
  /**
   * IntelliJ-style dependency explorer modal.
   *
   * Two-pane layout:
   *   · LEFT  — flat / grouped list of all resolved dependencies, with
   *             filters (text, scope), group-by (none / scope / group),
   *             and toggles (outdated only, conflicts only).
   *   · RIGHT — "Usages of <selected>": every path from the project root
   *             down to an occurrence of the selected artifact. Mirrors
   *             IntelliJ's Maven/Gradle dep-analysis usages view.
   *
   * Data flow:
   *   The backing snapshot lives in `contributionStore.tree("deps-explorer",
   *   "deps:<request_id>")`. The deps-explorer plugin pushes it through
   *   `arbor.ui.tree.set` and updates it in place when Maven Central
   *   responses land. We re-derive the flat list, the artifact index and
   *   the usages map reactively from the snapshot.
   */
  import {
    Search, Loader, AlertCircle, AlertTriangle, Package,
    ChevronDown, ChevronRight, ArrowUpRight, RefreshCw,
    Trash2, ListTree, Boxes, GitBranch, Filter as FilterIcon,
    GitCommit, Cloud,
  } from 'lucide-svelte';
  import { fade } from 'svelte/transition';
  import Modal from './Modal.svelte';
  import ModalHeader from './ModalHeader.svelte';
  import Dropdown from './ui/Dropdown.svelte';
  import type { DropdownItem } from './ui/Dropdown.svelte';
  import { contributionStore } from '$lib/stores/contribution.svelte';
  import { depsExplorerStore } from '$lib/stores/depsExplorer.svelte';
  import { firePluginAction } from '$lib/ipc/plugin';
  import type { TreeNode } from '$lib/types/contribution';
  import { tooltip } from '$lib/actions/tooltip';

  // ── Snapshot subscription ────────────────────────────────────────────────
  const sidebarId = $derived(depsExplorerStore.currentSidebarId ?? '');
  const snapshot  = $derived(
    sidebarId ? contributionStore.tree(depsExplorerStore.pluginName, sidebarId) : null
  );

  $effect(() => {
    // Force the contribution store to fetch the snapshot if the listener
    // landed before we mounted (race with the plugin's first tree.set).
    if (sidebarId) contributionStore.ensureTree(depsExplorerStore.pluginName, sidebarId);
  });

  const title = $derived(snapshot?.title ?? 'Analyze dependencies');
  const rootNodes = $derived(
    Array.isArray(snapshot?.nodes) ? (snapshot!.nodes as TreeNode[]) : []
  );

  // ── Status (loading / error / ready) ────────────────────────────────────
  // The plugin signals state through a single sentinel node of kind
  // `deps:status` when there are no real deps to render; once the tree is
  // ready, real `dep` nodes take over and we treat the snapshot as ready.
  type Status = 'loading' | 'error' | 'empty' | 'ready';
  const status = $derived<Status>((() => {
    if (!snapshot) return 'loading';
    if (rootNodes.length === 0) return 'empty';
    const first = rootNodes[0];
    if (first?.kind === 'deps:status') {
      const s = (first.data as any)?.status;
      if (s === 'loading') return 'loading';
      if (s === 'error')   return 'error';
    }
    return 'ready';
  })());
  const statusMessage = $derived(
    status === 'error'   ? (rootNodes[0]?.label ?? 'Resolver error.') :
    status === 'loading' ? (rootNodes[0]?.label ?? 'Resolving dependency graph…') :
    ''
  );

  // ── Flatten the tree once per snapshot update ───────────────────────────
  interface DepEntry {
    nodeId:    string;        // unique tree-node id (for selection)
    artifact:  string;        // display name without version
    group:     string;        // may be "" for cargo/npm
    version:   string;
    scope:     string;
    /** "<group>:<artifact>" with empty group folded out — keys the index. */
    key:       string;
    path:      string[];      // ancestor labels from root → this node (excl. self)
    pathIds:   string[];      // ancestor node ids (excl. self)
    raw:       TreeNode;
  }

  const allDeps = $derived(flatten(rootNodes));

  // Lua's empty `{}` round-trips through JSON as an object, not an array, so a
  // leaf node's `children = {}` arrives here as `{}` and `?? []` doesn't catch
  // it. Coerce anything non-array to `[]` at every iteration boundary.
  const toArr = <T,>(v: unknown): T[] => Array.isArray(v) ? (v as T[]) : [];

  function flatten(nodes: TreeNode[]): DepEntry[] {
    const out: DepEntry[] = [];
    function visit(n: TreeNode, path: string[], pathIds: string[]) {
      if (n.kind === 'dep') {
        const d = (n.data ?? {}) as any;
        const group = (d.group ?? '') as string;
        const artifact = (d.artifact ?? n.label) as string;
        const key = group ? `${group}:${artifact}` : artifact;
        out.push({
          nodeId:   n.id,
          artifact, group,
          version:  (d.version ?? '') as string,
          scope:    (d.scope ?? '') as string,
          key,
          path,
          pathIds,
          raw: n,
        });
      }
      for (const c of toArr<TreeNode>(n.children)) {
        const childPath    = n.kind === 'dep' ? [...path, `${(n.data as any)?.artifact ?? n.label} ${(n.data as any)?.version ?? ''}`.trim()] : path;
        const childPathIds = n.kind === 'dep' ? [...pathIds, n.id] : pathIds;
        visit(c, childPath, childPathIds);
      }
    }
    for (const r of toArr<TreeNode>(nodes)) visit(r, [], []);
    return out;
  }

  /** Index of unique artifacts. Each artifact carries the cumulative metadata
   *  derived from all its occurrences (versions seen, scopes seen, latest
   *  central, conflict flag). One row per key in the LEFT pane. */
  interface ArtifactIndex {
    key:        string;
    group:      string;
    artifact:   string;
    versions:   string[];          // sorted unique versions seen
    scopes:     string[];          // sorted unique scopes seen
    occurrences: DepEntry[];       // every flattened occurrence
    latestCentral: string | null;
    isOutdated:   boolean;
    isConflict:   boolean;         // 2+ versions for the same key
  }

  const artifactIndex = $derived(buildIndex(allDeps));

  function buildIndex(deps: DepEntry[]): ArtifactIndex[] {
    const map = new Map<string, ArtifactIndex>();
    for (const d of deps) {
      const existing = map.get(d.key);
      if (!existing) {
        const dataAny = d.raw.data as any;
        map.set(d.key, {
          key:           d.key,
          group:         d.group,
          artifact:      d.artifact,
          versions:      d.version ? [d.version] : [],
          scopes:        d.scope ? [d.scope] : [],
          occurrences:   [d],
          latestCentral: (dataAny?.latest_central as string) ?? null,
          isOutdated:    !!dataAny?.is_outdated,
          isConflict:    false,
        });
      } else {
        if (d.version && !existing.versions.includes(d.version)) {
          existing.versions.push(d.version);
        }
        if (d.scope && !existing.scopes.includes(d.scope)) {
          existing.scopes.push(d.scope);
        }
        existing.occurrences.push(d);
        const dataAny = d.raw.data as any;
        if (!existing.latestCentral && dataAny?.latest_central) {
          existing.latestCentral = dataAny.latest_central;
        }
        if (dataAny?.is_outdated) existing.isOutdated = true;
      }
    }
    for (const a of map.values()) {
      a.versions.sort();
      a.scopes.sort();
      a.isConflict = a.versions.length > 1;
    }
    return [...map.values()].sort((x, y) => x.key.localeCompare(y.key));
  }

  // ── Filters / grouping ──────────────────────────────────────────────────
  let filterText      = $state('');
  let filterScope     = $state<string | null>(null);   // null = all
  let onlyOutdated    = $state(false);
  let onlyConflicts   = $state(false);
  let groupBy         = $state<'none' | 'scope' | 'group'>('scope');

  const allScopes = $derived((() => {
    const set = new Set<string>();
    for (const a of artifactIndex) for (const s of a.scopes) if (s) set.add(s);
    return [...set].sort();
  })());

  const filteredArtifacts = $derived((() => {
    const q = filterText.trim().toLowerCase();
    return artifactIndex.filter(a => {
      if (q && !a.key.toLowerCase().includes(q) && !a.artifact.toLowerCase().includes(q)) return false;
      if (filterScope && !a.scopes.includes(filterScope)) return false;
      if (onlyOutdated && !a.isOutdated) return false;
      if (onlyConflicts && !a.isConflict) return false;
      return true;
    });
  })());

  /** Grouped view: array of (group_label, group_count, artifacts[]). */
  interface ArtifactGroup { label: string; items: ArtifactIndex[]; }
  const groupedArtifacts = $derived(groupArtifacts(filteredArtifacts, groupBy));

  // ── Toolbar dropdown items ────────────────────────────────────────────────
  const scopeItems = $derived<DropdownItem[]>([
    {
      kind:    'item',
      id:      '__all__',
      label:   'All scopes',
      active:  filterScope === null,
      onclick: () => { filterScope = null; },
    },
    ...allScopes.map(s => ({
      kind:    'item' as const,
      id:      s,
      label:   s,
      active:  filterScope === s,
      onclick: () => { filterScope = s; },
    })),
  ]);
  const scopeLabel = $derived(filterScope ?? 'All scopes');

  const groupByItems = $derived<DropdownItem[]>([
    { kind: 'item', id: 'none',  label: 'No grouping',             active: groupBy === 'none',  onclick: () => { groupBy = 'none'; } },
    { kind: 'item', id: 'scope', label: 'Group by scope',          active: groupBy === 'scope', onclick: () => { groupBy = 'scope'; } },
    { kind: 'item', id: 'group', label: 'Group by group/namespace',active: groupBy === 'group', onclick: () => { groupBy = 'group'; } },
  ]);
  const groupByLabel = $derived(
    groupBy === 'none'  ? 'No grouping' :
    groupBy === 'scope' ? 'Group by scope' :
    'Group by group/namespace'
  );

  function groupArtifacts(items: ArtifactIndex[], by: 'none' | 'scope' | 'group'): ArtifactGroup[] {
    if (by === 'none') return [{ label: '', items }];
    const map = new Map<string, ArtifactIndex[]>();
    for (const a of items) {
      let key: string;
      if (by === 'scope') {
        // An artifact may carry multiple scopes (different occurrences) —
        // group it under the scope of its FIRST occurrence so each row
        // appears in exactly one group. Multi-scope artifacts are visually
        // marked by the scope chip on the row itself.
        key = a.scopes[0] ?? '(no scope)';
      } else {
        key = a.group || '(no group)';
      }
      const arr = map.get(key) ?? [];
      arr.push(a);
      map.set(key, arr);
    }
    return [...map.entries()]
      .sort(([x], [y]) => x.localeCompare(y))
      .map(([label, items]) => ({ label, items }));
  }

  // ── Selection ────────────────────────────────────────────────────────────
  let selectedKey = $state<string | null>(null);
  $effect(() => {
    // Auto-select first artifact when ready / when filter changes the list.
    if (selectedKey && filteredArtifacts.some(a => a.key === selectedKey)) return;
    selectedKey = filteredArtifacts[0]?.key ?? null;
  });
  const selectedArtifact = $derived(
    selectedKey ? (artifactIndex.find(a => a.key === selectedKey) ?? null) : null
  );

  // ── Stats footer ─────────────────────────────────────────────────────────
  const totalDeps = $derived(artifactIndex.length);
  const outdatedCount = $derived(artifactIndex.filter(a => a.isOutdated).length);
  const conflictCount = $derived(artifactIndex.filter(a => a.isConflict).length);

  // ── Close handling ───────────────────────────────────────────────────────
  function close() {
    depsExplorerStore.close();
  }
  function handleKey(e: KeyboardEvent) {
    if (e.key === 'Escape') close();
  }

  // ── Refresh (split button) ───────────────────────────────────────────────
  // Fires `deps-explorer:refresh` with `{ request_id, mode }`. Mode controls
  // which cache layer gets bypassed:
  //   · `all`    — drop registry "miss" entries + re-run the resolver.
  //   · `tree`   — re-run the resolver only; keep registry caches intact.
  //   · `latest` — keep the resolved tree; fully wipe the registry caches so
  //                the latest-version pass re-fetches everything.
  const requestId = $derived(sidebarId.startsWith('deps:') ? sidebarId.slice(5) : '');
  type RefreshMode = 'all' | 'tree' | 'latest';
  function refresh(mode: RefreshMode = 'all') {
    refreshMenuOpen = false;
    if (!requestId) return;
    firePluginAction(depsExplorerStore.pluginName, 'deps-explorer:refresh',
      JSON.stringify({ request_id: requestId, mode })).catch(() => {});
  }
  let refreshMenuOpen = $state(false);
  function closeRefreshMenu(e: MouseEvent) {
    // Close on outside click. The buttons themselves stop propagation so
    // their handlers run before this.
    const target = e.target as Node | null;
    if (!target) { refreshMenuOpen = false; return; }
    const root = document.querySelector('.refresh-split');
    if (root && !root.contains(target)) refreshMenuOpen = false;
  }
  $effect(() => {
    if (!refreshMenuOpen) return;
    window.addEventListener('mousedown', closeRefreshMenu, true);
    return () => window.removeEventListener('mousedown', closeRefreshMenu, true);
  });

  // ── Group-collapse state (LEFT pane) ─────────────────────────────────────
  let collapsedGroups = $state(new Set<string>());
  function toggleGroup(label: string) {
    const next = new Set(collapsedGroups);
    next.has(label) ? next.delete(label) : next.add(label);
    collapsedGroups = next;
  }

  // ── Scope chip palette ───────────────────────────────────────────────────
  function scopeKind(scope: string): 'compile' | 'runtime' | 'test' | 'dev' | 'prod' | 'peer' | 'other' {
    const s = scope.toLowerCase();
    if (s === 'compile' || s === 'normal') return 'compile';
    if (s === 'runtime') return 'runtime';
    if (s === 'test')    return 'test';
    if (s === 'dev')     return 'dev';
    if (s === 'prod')    return 'prod';
    if (s === 'peer')    return 'peer';
    return 'other';
  }
</script>

{#if depsExplorerStore.isOpen}
<Modal onClose={close} width="min(1100px, 96vw)" height="min(760px, 88vh)" padBody={false} ariaLabel="Dependency Explorer">
  {#snippet header()}
    <ModalHeader onClose={close}>
      <div class="header-title">
        <span class="title-text">Dependency Explorer</span>
        <span class="title-sub">{title}</span>
      </div>
    </ModalHeader>
  {/snippet}

  <div class="de-body">
    {#if status === 'loading' || status === 'empty'}
      <div class="state state-loading">
        <Loader size={20} class="spin" />
        <div>
          <div class="state-title">Resolving dependencies…</div>
          <div class="state-sub">{statusMessage || 'Running the toolchain — this may take a few seconds on the first run.'}</div>
        </div>
      </div>

    {:else if status === 'error'}
      <div class="state state-error">
        <AlertCircle size={20} />
        <div>
          <div class="state-title">Couldn't resolve dependencies</div>
          <div class="state-sub">{statusMessage}</div>
        </div>
      </div>

    {:else}
      <!-- ── Toolbar ────────────────────────────────────────────────────── -->
      <div class="card-toolbar">
        <span class="search-wrap">
          <Search size={12} class="search-ic" />
          <input
            type="text"
            placeholder="Filter by group / artifact…"
            bind:value={filterText}
          />
          {#if filterText}
            <button class="clear" onclick={() => filterText = ''}>×</button>
          {/if}
        </span>

        <span class="tb-sep"></span>

        <!-- Scope filter -->
        <Dropdown position="fixed" direction="down" items={scopeItems}>
          {#snippet trigger({ open, toggle })}
            <button class="tb-select" onclick={toggle} type="button" use:tooltip={'Scope'} aria-expanded={open}>
              <span class="tb-select-label">{scopeLabel}</span>
              <ChevronDown size={11} />
            </button>
          {/snippet}
        </Dropdown>

        <!-- Group-by -->
        <Dropdown position="fixed" direction="down" items={groupByItems}>
          {#snippet trigger({ open, toggle })}
            <button class="tb-select" onclick={toggle} type="button" use:tooltip={'Group by'} aria-expanded={open}>
              <span class="tb-select-label">{groupByLabel}</span>
              <ChevronDown size={11} />
            </button>
          {/snippet}
        </Dropdown>

        <span class="tb-sep"></span>

        <button
          class="tb-toggle tb-outdated"
          class:active={onlyOutdated}
          use:tooltip={{ content: 'Outdated only', description: 'Show only artifacts with a newer version published in the registry (Maven Central / npmjs / crates.io)' }}
          onclick={() => onlyOutdated = !onlyOutdated}
        >
          <ArrowUpRight size={12} />
          Outdated
        </button>
        <button
          class="tb-toggle tb-conflicts"
          class:active={onlyConflicts}
          use:tooltip={'Show only artifacts pulled in at multiple versions'}
          onclick={() => onlyConflicts = !onlyConflicts}
        >
          <AlertTriangle size={12} />
          Conflicts
        </button>

        <span class="spacer"></span>

        {#if filterText || filterScope || onlyOutdated || onlyConflicts}
          <button
            class="tb-toggle"
            use:tooltip={'Clear all filters'}
            onclick={() => { filterText=''; filterScope=null; onlyOutdated=false; onlyConflicts=false; }}
          >
            <Trash2 size={12} />
            Reset
          </button>
        {/if}

        <span class="refresh-split">
          <button
            class="tb-toggle tb-refresh refresh-main"
            use:tooltip={{ content: 'Refresh everything', description: 'Drop cache misses and re-run the resolver' }}
            onclick={() => refresh('all')}
          >
            <RefreshCw size={12} />
            Refresh
          </button>
          <button
            class="tb-toggle refresh-chevron"
            class:active={refreshMenuOpen}
            use:tooltip={'More refresh options'}
            aria-label="More refresh options"
            onclick={(e) => { e.stopPropagation(); refreshMenuOpen = !refreshMenuOpen; }}
          >
            <ChevronDown size={11} />
          </button>
          {#if refreshMenuOpen}
            <div class="refresh-menu" transition:fade={{ duration: 80 }}>
              <button
                type="button"
                class="refresh-menu-item"
                onclick={(e) => { e.stopPropagation(); refresh('all'); }}
              >
                <span class="ri-icon"><RefreshCw size={12} /></span>
                <span class="ri-body">
                  <span class="ri-title">Refresh all</span>
                  <span class="ri-sub">Re-run the resolver and refetch missing latest versions</span>
                </span>
              </button>
              <button
                type="button"
                class="refresh-menu-item"
                onclick={(e) => { e.stopPropagation(); refresh('tree'); }}
              >
                <span class="ri-icon"><GitCommit size={12} /></span>
                <span class="ri-body">
                  <span class="ri-title">Refresh dependencies only</span>
                  <span class="ri-sub">Re-run the resolver. Keep cached latest-version data.</span>
                </span>
              </button>
              <button
                type="button"
                class="refresh-menu-item"
                onclick={(e) => { e.stopPropagation(); refresh('latest'); }}
              >
                <span class="ri-icon"><Cloud size={12} /></span>
                <span class="ri-body">
                  <span class="ri-title">Refresh latest versions only</span>
                  <span class="ri-sub">Keep the resolved tree. Refetch all latest versions from the registry.</span>
                </span>
              </button>
            </div>
          {/if}
        </span>
      </div>

      <!-- ── Two-pane body ──────────────────────────────────────────────── -->
      <div class="card-body">
        <div class="pane pane-left">
          <div class="pane-header pane-header-left">
            <ListTree size={12} />
            <span>Resolved Dependencies</span>
            <span class="pane-count">{filteredArtifacts.length} / {totalDeps}</span>
          </div>
          <div class="pane-body">
            {#each groupedArtifacts as g (g.label || '__none__')}
              {#if groupBy !== 'none' && g.label}
                <button
                  class="group-row"
                  type="button"
                  onclick={() => toggleGroup(g.label)}
                >
                  {#if collapsedGroups.has(g.label)}
                    <ChevronRight size={12} />
                  {:else}
                    <ChevronDown size={12} />
                  {/if}
                  <span class="group-label">{g.label}</span>
                  <span class="group-count">{g.items.length}</span>
                </button>
              {/if}
              {#if !collapsedGroups.has(g.label)}
                {#each g.items as a (a.key)}
                  <div
                    class="dep-row"
                    class:selected={selectedKey === a.key}
                    role="button"
                    tabindex="0"
                    onclick={() => selectedKey = a.key}
                    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); selectedKey = a.key; } }}
                  >
                    <span class="dep-icon scope-{scopeKind(a.scopes[0] ?? '')}-fg" use:tooltip={a.scopes[0] ?? ''}>
                      <Package size={11} />
                    </span>
                    <span class="dep-name">
                      {#if a.group}
                        <span class="dep-group">{a.group}</span>:<wbr/>
                      {/if}<span class="dep-artifact">{a.artifact}</span>
                    </span>
                    <span class="dep-versions">
                      {#each a.versions as v, i}
                        <span class="ver-tag" class:ver-multi={a.versions.length > 1}>
                          {v}{i < a.versions.length - 1 ? ',' : ''}
                        </span>
                      {/each}
                      {#if a.versions.length === 0}
                        <span class="ver-tag ver-unknown">?</span>
                      {/if}
                    </span>
                    <span class="dep-tags">
                      {#each a.scopes as s}
                        <span class="scope-chip scope-{scopeKind(s)}">{s}</span>
                      {/each}
                      {#if a.isConflict}
                        <span class="badge badge-warn" use:tooltip={'Multiple versions resolved'}>
                          <AlertTriangle size={10} /> conflict
                        </span>
                      {/if}
                      {#if a.isOutdated && a.latestCentral}
                        <span class="badge badge-out" use:tooltip={`Latest published version: ${a.latestCentral}`}>
                          <ArrowUpRight size={10} /> {a.latestCentral}
                        </span>
                      {/if}
                    </span>
                  </div>
                {/each}
              {/if}
            {/each}

            {#if filteredArtifacts.length === 0}
              <div class="pane-empty">
                <FilterIcon size={14} />
                <span>No dependencies match the current filters.</span>
              </div>
            {/if}
          </div>
        </div>

        <div class="pane pane-right">
          <div class="pane-header pane-header-right">
            <GitBranch size={12} />
            <span>
              Usages of
              <code>{selectedArtifact ? selectedArtifact.key : '—'}</code>
            </span>
          </div>
          <div class="pane-body">
            {#if !selectedArtifact}
              <div class="pane-empty">
                <Boxes size={14} />
                <span>Select a dependency on the left to see who brings it in.</span>
              </div>
            {:else}
              {@const occ = selectedArtifact.occurrences}
              <div class="usages-summary">
                {occ.length} occurrence{occ.length === 1 ? '' : 's'} across the tree
                {#if selectedArtifact.versions.length > 1}
                  · resolved at {selectedArtifact.versions.join(', ')}
                {/if}
                {#if selectedArtifact.latestCentral}
                  · latest on Maven Central: <strong>{selectedArtifact.latestCentral}</strong>
                {/if}
              </div>

              {#each occ as o, i}
                <div class="usage-block">
                  <div class="usage-header">
                    <span class="usage-idx">#{i + 1}</span>
                    <span class="usage-version">{selectedArtifact.artifact} {o.version || '?'}</span>
                    {#if o.scope}
                      <span class="scope-chip scope-{scopeKind(o.scope)}">{o.scope}</span>
                    {/if}
                  </div>
                  <div class="usage-path">
                    {#if o.path.length === 0}
                      <span class="path-direct">direct dependency</span>
                    {:else}
                      <span class="path-root">project</span>
                      {#each o.path as step, idx}
                        <ChevronRight size={10} class="path-arrow" />
                        <span class="path-step" class:path-leaf={idx === o.path.length - 1}>{step}</span>
                      {/each}
                      <ChevronRight size={10} class="path-arrow" />
                      <span class="path-target">{selectedArtifact.artifact} {o.version}</span>
                    {/if}
                  </div>
                </div>
              {/each}
            {/if}
          </div>
        </div>
      </div>

      <!-- ── Footer ─────────────────────────────────────────────────────── -->
      <div class="card-footer">
        <span class="stat stat-accent">
          <Package size={11} />
          <strong>{totalDeps}</strong> dependencies
        </span>
        <span class="stat stat-success" class:stat-dim={outdatedCount === 0}>
          <ArrowUpRight size={11} />
          <strong>{outdatedCount}</strong> outdated
        </span>
        <span class="stat stat-warn" class:stat-dim={conflictCount === 0}>
          <AlertTriangle size={11} />
          <strong>{conflictCount}</strong> with conflicts
        </span>
        <span class="spacer"></span>
        {#if !snapshot}
          <span class="stat muted">awaiting first snapshot…</span>
        {:else}
          <span class="stat muted">snapshot v{snapshot.version}</span>
        {/if}
      </div>
    {/if}
  </div>
</Modal>
{/if}

<style>
  .de-body {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  /* Header content */
  .header-title { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
  .title-text {
    font-size: 12px; font-weight: 600; color: var(--text-primary);
    text-transform: uppercase; letter-spacing: 0.04em;
  }
  .title-sub {
    font-size: 11px; color: var(--text-muted);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }

  /* Toolbar */
  .card-toolbar {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    flex-shrink: 0;
  }
  .search-wrap {
    display: flex; align-items: center; gap: 6px;
    background: var(--bg-base);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 3px 8px;
    width: 260px;
  }
  .search-wrap :global(.search-ic) { color: var(--text-muted); flex-shrink: 0; }
  .search-wrap input {
    flex: 1; background: transparent; border: none; outline: none;
    color: var(--text-primary); font-family: var(--font-ui-sans); font-size: 12px;
    min-width: 0;
  }
  .clear {
    background: transparent; border: none; color: var(--text-muted);
    font-size: 14px; cursor: pointer; padding: 0 4px;
  }
  .clear:hover { color: var(--text-primary); }

  .tb-select {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    background: var(--bg-input);
    color: var(--text-primary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 3px 8px;
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    text-align: left;
    transition: border-color var(--transition-fast);
  }
  .tb-select:hover,
  .tb-select[aria-expanded='true'] { border-color: var(--accent); }
  .tb-select-label { white-space: nowrap; }
  .tb-toggle {
    display: inline-flex; align-items: center; gap: 4px;
    background: var(--bg-base);
    color: var(--text-secondary);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    padding: 3px 8px;
    font-family: var(--font-ui-sans);
    font-size: 11px;
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .tb-toggle:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tb-toggle.active {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .tb-sep {
    width: 1px; height: 16px; background: var(--border-subtle);
    flex-shrink: 0; margin: 0 2px;
  }
  .spacer { flex: 1 1 auto; }

  /* Body */
  .card-body {
    flex: 1; min-height: 0; display: flex;
    background: var(--bg-base);
  }
  .pane {
    display: flex; flex-direction: column; min-height: 0;
    overflow: hidden;
  }
  .pane-left  { flex: 1 1 58%; border-right: 1px solid var(--border-subtle); }
  .pane-right { flex: 1 1 42%; }

  .pane-header {
    display: flex; align-items: center; gap: 6px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    color: var(--text-secondary);
    font-size: 11px; text-transform: uppercase; letter-spacing: 0.04em;
    font-weight: 600;
    flex-shrink: 0;
  }
  /* Tint the leading icon only — keeps the header text neutral while
     making each pane visually identifiable at a glance. */
  .pane-header-left  > :global(svg:first-child) { color: var(--accent); }
  .pane-header-right > :global(svg:first-child) { color: var(--success); }
  .pane-count {
    margin-left: auto;
    font-family: var(--font-code);
    font-size: 10px;
    color: var(--text-muted);
    text-transform: none; letter-spacing: 0;
    background: var(--bg-overlay);
    padding: 1px 6px; border-radius: 999px;
  }
  .pane-body {
    flex: 1; min-height: 0; overflow: auto;
    padding: 4px 0;
  }

  .pane-empty {
    display: flex; align-items: center; justify-content: center;
    gap: 8px; padding: 40px 16px;
    color: var(--text-muted); font-size: 12px;
  }

  /* Group rows */
  .group-row {
    display: flex; align-items: center; gap: 6px;
    width: 100%;
    padding: 4px 12px;
    background: transparent; border: none;
    color: var(--text-secondary);
    font-family: var(--font-ui-sans); font-size: 11px; font-weight: 600;
    cursor: pointer;
    text-transform: uppercase; letter-spacing: 0.04em;
  }
  .group-row:hover { background: var(--bg-hover); }
  .group-label { flex: 1; text-align: left; }
  .group-count {
    color: var(--text-muted);
    font-family: var(--font-code); font-size: 10px;
    background: var(--bg-overlay); padding: 1px 6px; border-radius: 999px;
  }

  /* Dep rows */
  .dep-row {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 8px;
    padding: 4px 12px 4px 28px;
    cursor: pointer;
    font-family: var(--font-code); font-size: 11px;
    color: var(--text-primary);
    border-left: 2px solid transparent;
  }
  .dep-row:hover { background: var(--bg-hover); }
  .dep-row.selected {
    background: var(--accent-subtle);
    border-left-color: var(--accent);
  }
  .dep-icon { color: var(--text-muted); display: inline-flex; }
  .dep-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dep-group { color: var(--text-muted); }
  .dep-artifact { color: var(--text-primary); font-weight: 500; }
  .dep-versions { display: inline-flex; gap: 3px; }
  .ver-tag {
    color: var(--text-secondary);
    background: var(--bg-overlay);
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    font-size: 10px;
  }
  .ver-tag.ver-multi { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); }
  .ver-tag.ver-unknown { color: var(--text-muted); }

  .dep-tags { display: inline-flex; gap: 4px; align-items: center; }

  .scope-chip {
    font-family: var(--font-code); font-size: 9px;
    padding: 1px 5px;
    border-radius: 999px;
    border: 1px solid transparent;
    text-transform: lowercase; letter-spacing: 0.02em;
  }
  .scope-compile  { color: var(--accent);  background: var(--accent-subtle); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
  .scope-runtime  { color: var(--success);        background: color-mix(in srgb, var(--success) 14%, transparent); border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  .scope-test     { color: var(--warning);        background: color-mix(in srgb, var(--warning) 14%, transparent); border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
  .scope-dev      { color: var(--color-tag);        background: color-mix(in srgb, var(--color-tag) 14%, transparent); border-color: color-mix(in srgb, var(--color-tag) 30%, transparent); }
  .scope-prod     { color: var(--accent);  background: var(--accent-subtle); border-color: color-mix(in srgb, var(--accent) 30%, transparent); }
  .scope-peer     { color: var(--color-stash);        background: color-mix(in srgb, var(--color-stash) 14%, transparent); border-color: color-mix(in srgb, var(--color-stash) 30%, transparent); }
  .scope-other    { color: var(--text-muted); background: var(--bg-overlay); }

  /* Foreground-only variants for the row icon. Mirrors the chip palette so
     a glance at the icon column already telegraphs scope distribution. */
  .scope-compile-fg { color: var(--accent); }
  .scope-runtime-fg { color: var(--success); }
  .scope-test-fg    { color: var(--warning); }
  .scope-dev-fg     { color: var(--color-tag); }
  .scope-prod-fg    { color: var(--accent); }
  .scope-peer-fg    { color: var(--color-stash); }
  .scope-other-fg   { color: var(--text-muted); }

  .badge {
    display: inline-flex; align-items: center; gap: 3px;
    font-family: var(--font-code); font-size: 9px;
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
  }
  .badge-warn { color: var(--warning); background: color-mix(in srgb, var(--warning) 14%, transparent); border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
  .badge-out  { color: var(--success); background: color-mix(in srgb, var(--success) 14%, transparent); border-color: color-mix(in srgb, var(--success) 30%, transparent); }

  /* RIGHT pane: usages */
  .usages-summary {
    padding: 8px 14px;
    color: var(--text-muted);
    font-size: 11px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .usage-block {
    padding: 8px 14px;
    border-bottom: 1px solid var(--border-subtle);
  }
  .usage-block:last-child { border-bottom: none; }
  .usage-header {
    display: flex; align-items: center; gap: 8px;
    font-family: var(--font-code); font-size: 11px;
    color: var(--text-primary);
    margin-bottom: 4px;
  }
  .usage-idx {
    color: var(--text-muted);
    font-size: 10px;
  }
  .usage-version { font-weight: 500; }
  .usage-path {
    display: flex; align-items: center; gap: 4px; flex-wrap: wrap;
    font-family: var(--font-code); font-size: 10.5px;
    color: var(--text-secondary);
  }
  .usage-path :global(.path-arrow) { color: var(--text-muted); }
  .path-root, .path-target {
    background: var(--bg-overlay);
    padding: 1px 6px; border-radius: var(--radius-sm);
  }
  .path-root { color: var(--accent); }
  .path-target { color: var(--text-primary); font-weight: 500; }
  .path-step { color: var(--text-secondary); }
  .path-step.path-leaf { color: var(--text-primary); }
  .path-direct { color: var(--accent); font-weight: 500; }

  /* Footer */
  .card-footer {
    display: flex; align-items: center; gap: 14px;
    padding: 6px 14px;
    border-top: 1px solid var(--border-subtle);
    background: var(--bg-elevated);
    font-family: var(--font-ui-sans); font-size: 11px;
    color: var(--text-secondary);
    flex-shrink: 0;
  }
  .stat { display: inline-flex; align-items: center; gap: 4px; }
  .stat strong { color: var(--text-primary); font-weight: 600; }
  .stat.muted { color: var(--text-muted); }
  /* Stat-icon tints — only the leading icon picks up the colour so the
     count stays readable. `stat-dim` greys the icon back out when the
     count is zero (no outdated → no green dot screaming for attention). */
  .stat-accent  > :global(svg:first-child) { color: var(--accent); }
  .stat-success > :global(svg:first-child) { color: var(--success); }
  .stat-warn    > :global(svg:first-child) { color: var(--warning); }
  .stat-dim     > :global(svg:first-child) { color: var(--text-muted); opacity: 0.7; }

  /* Refresh button — accent on the icon, default text. The button-as-a-
     whole reuses the .tb-toggle base style so the surface stays uniform. */
  .tb-refresh   > :global(svg:first-child) { color: var(--accent); }

  /* Split-button: primary "Refresh" + chevron. Visually one widget — a
     hairline separates the two halves and the inner radii are flattened. */
  .refresh-split {
    position: relative;
    display: inline-flex;
    align-items: center;
  }
  .refresh-main {
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    border-right: none;
  }
  .refresh-chevron {
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    padding: 3px 5px;
    display: inline-flex; align-items: center; justify-content: center;
  }
  .refresh-chevron.active {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .refresh-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 1100;
    min-width: 280px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
    padding: 4px;
    display: flex; flex-direction: column;
  }
  .refresh-menu-item {
    display: flex; align-items: flex-start; gap: 8px;
    padding: 6px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    cursor: pointer;
    text-align: left;
    color: var(--text-primary);
    font-family: var(--font-ui-sans);
  }
  .refresh-menu-item:hover { background: var(--bg-hover); }
  .ri-icon {
    flex-shrink: 0;
    width: 18px; height: 18px;
    display: inline-flex; align-items: center; justify-content: center;
    color: var(--accent);
    margin-top: 1px;
  }
  .ri-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .ri-title { font-size: 12px; font-weight: 500; }
  .ri-sub   { font-size: 10.5px; color: var(--text-muted); line-height: 1.35; }
  /* Outdated / Conflicts toggles — match the footer + badge palette so the
     same concept gets the same colour wherever it surfaces. The .active
     state already paints the whole button in the accent tint; we only
     colour the icon when the toggle is OFF, otherwise it'd clash. */
  .tb-outdated:not(.active)  > :global(svg:first-child) { color: var(--success); }
  .tb-conflicts:not(.active) > :global(svg:first-child) { color: var(--warning); }

  /* Loading / error states (full-card overlay below header) */
  .state {
    flex: 1; min-height: 0;
    display: flex; align-items: center; justify-content: center;
    gap: 14px; padding: 40px;
    color: var(--text-secondary);
    text-align: left;
  }
  .state-title {
    font-size: 13px; font-weight: 600; color: var(--text-primary);
    margin-bottom: 4px;
  }
  .state-sub  { font-size: 11px; color: var(--text-muted); max-width: 480px; line-height: 1.5; }
  .state-error { color: var(--error); }
</style>

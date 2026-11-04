<script lang="ts">
  /**
   * Everything inspectable in the running game, as one lazy tree with three roots — **Entities** (the
   * hierarchy), **Resources** and **States** — all closed until opened.
   *
   * Entities load one level at a time from the backend's snapshot; typing in the box filters all three roots at
   * once (entity hits stay under their parents; resources and states match by name), and the roots that have
   * nothing to show disappear. Selecting a leaf shows its detail in the bottom panel.
   *
   * Keyboard: ↑↓ move, →/← open/close, Enter selects (or opens a root), `/` — or just typing a letter —
   * goes to the filter, Esc clears it and returns to the tree, Delete despawns the selected entity.
   */
  import { SvelteSet } from 'svelte/reactivity';
  import { ChevronsDownUp, ChevronsUpDown, Copy, LocateFixed, Plus, RefreshCw, Trash2, Unlink } from 'lucide-svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { bennuUiStore } from '$lib/stores/bennu/ui.svelte';
  import { bennuBrpStore as brp, type BrpLevel, type BrpRoot } from '$lib/stores/bennu/brp.svelte';
  import type { BrpResourceEntry, BrpRow } from '$lib/ipc/bennu/brp';
  import BrpIcon from './icons/BrpIcon.svelte';
  import { entityIcon } from './icons/icon-kinds';
  import BrpQueryMenu from './BrpQueryMenu.svelte';
  import { isStateResource, type StateEntry } from './brp-state';
  import { branchIds, buildFilterTree, isFilterKey, type FilterNode } from './brp-tree';
  import { entityTag, shortTypeName } from './brp-value';
  import { parseSearch } from './brp-search';

  let { onDespawn }: { onDespawn: (id: number) => void } = $props();

  interface TNode {
    id: string;
    kind: 'root' | 'entity' | 'resource' | 'state' | 'more';
    root?: BrpRoot;
    label?: string;
    count?: number;
    row?: BrpRow;
    res?: BrpResourceEntry;
    state?: StateEntry;
    /** Filtered entity tree: an ancestor shown only to give a hit its place. */
    context?: boolean;
    kids?: number;
    more?: { parent?: number; remaining: number };
    children: TNode[] | null;
  }

  // ── Entities ──

  function levelNodes(level: BrpLevel, parent?: number): TNode[] {
    const out: TNode[] = level.rows.map((row) => ({ id: `e:${row.id}`, kind: 'entity', row, kids: row.child_count, children: childNodes(row) }));
    if (level.total > level.rows.length) {
      out.push({ id: `more:${parent ?? 'root'}`, kind: 'more', more: { parent, remaining: level.total - level.rows.length }, children: [] });
    }
    return out;
  }

  function childNodes(row: BrpRow): TNode[] | null {
    if (row.child_count === 0) return [];
    const level = brp.children.get(row.id);
    return level ? levelNodes(level, row.id) : null;
  }

  const flat = $derived(brp.flat);
  const filterTree = $derived<FilterNode[]>(flat ? buildFilterTree(flat.rows, flat.context ?? []) : []);
  const closedInFilter = new SvelteSet<number>();
  const fromFilter = (n: FilterNode): TNode => ({
    id: `e:${n.row.id}`, kind: 'entity', row: n.row, context: !n.hit, kids: n.row.child_count, children: n.children.map(fromFilter),
  });

  // ── The three roots ──

  /** The words of the box that name things (what resources and states match on). */
  const needle = $derived(parseSearch(brp.search).text.trim().toLowerCase());
  const filtering = $derived(brp.search.trim().length > 0);

  /** Resources and states match by name; a box holding only `has:` / `no:` words is about entities. */
  const named = (path: string) => !filtering || (needle !== '' && path.toLowerCase().includes(needle));

  const entityNodes = $derived<TNode[]>(flat ? filterTree.map(fromFilter) : levelNodes(brp.roots));
  const resourceNodes = $derived<TNode[]>(
    brp.resources
      .filter((r) => !isStateResource(r.type_path) && named(r.type_path))
      .map((res) => ({ id: `r:${res.type_path}`, kind: 'resource', res, children: [] })),
  );
  const stateNodes = $derived<TNode[]>(
    brp.states
      .filter((s) => named(s.enumPath))
      .map((state) => ({ id: `s:${state.enumPath}`, kind: 'state', state, children: [] })),
  );

  const roots = $derived.by<TNode[]>(() => {
    const entityTotal = flat ? flat.total : brp.roots.total;
    const all: TNode[] = [
      { id: 'root:entities', kind: 'root', root: 'entities', label: 'Entities', count: entityTotal, children: entityNodes },
      ...(brp.status.can.resources
        ? [{ id: 'root:resources', kind: 'root' as const, root: 'resources' as const, label: 'Resources', count: resourceNodes.length, children: resourceNodes }]
        : []),
      { id: 'root:states', kind: 'root', root: 'states', label: 'States', count: stateNodes.length, children: stateNodes },
    ];
    // While filtering, a root with nothing to show is noise.
    return filtering ? all.filter((r) => (r.children?.length ?? 0) > 0) : all;
  });

  const expandedIds = $derived.by(() => {
    const ids = new Set<string>();
    for (const r of roots) {
      // A filter opens every root that has hits; otherwise the user's own choice stands.
      if (filtering ? true : brp.rootsOpen.has(r.root!)) ids.add(r.id);
    }
    if (flat) branchIds(filterTree).filter((id) => !closedInFilter.has(id)).forEach((id) => ids.add(`e:${id}`));
    else for (const id of brp.expanded) ids.add(`e:${id}`);
    return ids;
  });

  const selectedId = $derived(
    brp.focus === 'entity' && brp.selected !== null ? `e:${brp.selected}`
      : brp.focus === 'resource' && brp.selectedResource ? `r:${brp.selectedResource}`
      : brp.focus === 'state' && brp.selectedState ? `s:${brp.selectedState}`
      : null,
  );

  // The box shows what the store holds, so `reveal()` clearing the search clears the box too.
  let query = $state('');
  $effect(() => { query = brp.search; });
  $effect(() => { if (flat) closedInFilter.clear(); });

  let box = $state<{ focus: () => void } | undefined>();
  let region = $state<HTMLElement | undefined>();

  function onToggle(id: string, open: boolean) {
    if (id.startsWith('root:')) brp.openRoot(id.slice(5) as BrpRoot, open);
    else if (id.startsWith('e:')) {
      const n = Number(id.slice(2));
      if (!flat) void brp.toggle(n, open);
      else if (open) closedInFilter.delete(n);
      else closedInFilter.add(n);
    }
  }

  function expandAll() {
    (['entities', 'resources', 'states'] as BrpRoot[]).forEach((r) => brp.openRoot(r, true));
    if (flat) closedInFilter.clear();
    else void brp.expandAll();
  }

  function collapseAll() {
    (['entities', 'resources', 'states'] as BrpRoot[]).forEach((r) => brp.openRoot(r, false));
    if (flat) branchIds(filterTree).forEach((id) => closedInFilter.add(id));
    else brp.collapseAll();
  }

  /** A leaf is picked: its detail goes to the bottom panel. */
  function showDetail() {
    brp.setTab('detail');
    bennuUiStore.showBottom('bevyremote');
  }

  function onSelect(n: TNode) {
    switch (n.kind) {
      case 'root': brp.openRoot(n.root!, !brp.rootsOpen.has(n.root!)); break;
      case 'more': void brp.loadMore(n.more!.parent); break;
      case 'entity': void brp.select(n.row!.id); showDetail(); break;
      case 'resource': void brp.selectResource(n.res!.type_path); showDetail(); break;
      case 'state': void brp.selectState(n.state!.enumPath); showDetail(); break;
    }
  }

  function onContext(n: TNode, e: MouseEvent) {
    const row = n.row;
    if (n.kind !== 'entity' || !row) return;
    e.preventDefault();
    const items: MenuItem[] = [
      ...(flat ? [{ id: 'reveal', label: 'Reveal in hierarchy', icon: LocateFixed }] : []),
      { id: 'copy', label: 'Copy entity id', icon: Copy },
      ...(row.parent !== null && brp.status.can.reparent ? [{ id: 'detach', label: 'Detach from parent', icon: Unlink }] : []),
      ...(brp.status.can.despawn ? [{ id: 'despawn', label: 'Despawn…', icon: Trash2 }] : []),
    ];
    bennuContextMenuStore.show(e.clientX, e.clientY, items, (id) => {
      switch (id) {
        case 'reveal':  void brp.reveal(row.id); break;
        case 'copy':    void navigator.clipboard?.writeText(String(row.id)).catch(() => { /* clipboard denied */ }); break;
        case 'detach':  void brp.reparent(row.id); break;
        case 'despawn': onDespawn(row.id); break;
      }
    });
  }

  function onRowKeydown(n: TNode, e: KeyboardEvent) {
    if (e.key === 'Delete' && n.kind === 'entity' && n.row && brp.status.can.despawn) {
      e.preventDefault();
      onDespawn(n.row.id);
    }
  }

  /** `/` or a typed letter, with the tree focused, starts the filter — no click needed. */
  function onRegionKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.closest('input, textarea, select')) {
      if (e.key === 'Escape' && brp.search) {
        e.stopPropagation();
        brp.setSearch('');
        region?.querySelector<HTMLElement>('[role="tree"] [tabindex="0"], [role="tree"]')?.focus();
      }
      return;
    }
    if (e.key === '/') {
      e.preventDefault();
      box?.focus();
    } else if (isFilterKey(e)) {
      e.preventDefault();
      brp.setSearch(brp.search + e.key);
      box?.focus();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="bt" bind:this={region} role="region" aria-label="Bevy world" onkeydown={onRegionKey}>
  <div class="bt-search">
    <SearchBar
      bind:this={box}
      bind:query
      showRegex={false}
      showCounter={false}
      placeholder="Filter  ·  /  ·  name  has:Comp  no:Comp"
      ariaLabel="Filter the game's world"
      oninput={(v) => brp.setSearch(v)}
      onClear={() => brp.setSearch('')}
    />
    <BrpQueryMenu />
  </div>
  <div class="bt-tools">
    <IconButton tooltip="Expand all" size={20} onclick={expandAll}><ChevronsUpDown size={12} /></IconButton>
    <IconButton tooltip="Collapse all" size={20} onclick={collapseAll}><ChevronsDownUp size={12} /></IconButton>
    {#if brp.status.can.spawn}
      <IconButton tooltip="Spawn an empty entity" size={20} onclick={() => void brp.spawn()}><Plus size={12} /></IconButton>
    {/if}
    <IconButton tooltip="Refresh" size={20} onclick={() => void brp.reload()}><RefreshCw size={12} /></IconButton>
  </div>

  {#if brp.searchNote}
    <Alert variant="warning" compact text={brp.searchNote} />
  {/if}

  <div class="bt-body">
    {#if roots.length === 0}
      <EmptyState message="Nothing matches." compact />
    {:else}
      <Tree
        nodes={roots}
        {expandedIds}
        onExpandToggle={onToggle}
        hasChildren={(n) => (n.children === null ? true : n.children.length > 0)}
        {selectedId}
        {onSelect}
        onContextMenu={onContext}
        {onRowKeydown}
        toggleOnClick={false}
        ariaLabel="Bevy world"
      >
        {#snippet row({ node })}
          {#if node.kind === 'root'}
            <span class="bt-icon root">
              <BrpIcon name={node.root === 'entities' ? 'group' : node.root === 'resources' ? 'resource' : 'state'} size={14} />
            </span>
            <span class="bt-root">{node.label}</span>
            <span class="bt-kids">{node.count}</span>
          {:else if node.kind === 'more'}
            <span class="bt-more">Load {node.more?.remaining} more…</span>
          {:else if node.kind === 'resource'}
            <span class="bt-icon"><BrpIcon name="resource" size={13} /></span>
            <span class="bt-name" title={node.res?.type_path}>{node.res?.short_path}</span>
          {:else if node.kind === 'state'}
            <span class="bt-icon"><BrpIcon name="state" size={13} /></span>
            <span class="bt-name" title={node.state?.enumPath}>{shortTypeName(node.state?.enumPath ?? '')}</span>
          {:else if node.row}
            <span class="bt-icon" class:ctx={node.context}>
              <BrpIcon name={entityIcon(node.row.kind, node.kids ?? 0)} size={13} />
            </span>
            <span class="bt-name" class:unnamed={!node.row.name} class:ctx={node.context}>{node.row.name ?? 'Entity'}</span>
            {#if node.kids}<span class="bt-kids" title="{node.kids} children">{node.kids}</span>{/if}
            <span class="bt-id">{entityTag(node.row.id)}</span>
          {/if}
        {/snippet}
      </Tree>
    {/if}
  </div>
</div>

<style>
  .bt { display: flex; flex-direction: column; height: 100%; min-height: 0; }
  .bt-search { padding: 4px 6px 2px; flex: none; display: flex; align-items: center; gap: 4px; }
  .bt-search > :global(:first-child) { flex: 1; min-width: 0; }
  .bt-tools { flex: none; display: flex; justify-content: flex-end; gap: 2px; padding: 0 6px 2px; }
  .bt-body { flex: 1; min-height: 0; overflow-y: auto; }
  .bt-icon { display: inline-flex; color: var(--text-muted); flex: none; }
  .bt-icon.root { color: var(--accent); }
  .bt-icon.ctx, .bt-name.ctx { opacity: 0.55; }
  .bt-root { font-size: var(--font-size-xs); font-weight: 700; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.05em; }
  .bt-name { font-size: var(--font-size-sm); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .bt-name.unnamed { color: var(--text-muted); font-style: italic; }
  .bt-kids {
    flex: none; min-width: 16px; padding: 0 5px; border-radius: 8px; text-align: center;
    background: var(--bg-overlay); color: var(--text-muted);
    font-size: var(--font-size-2xs); font-variant-numeric: tabular-nums; line-height: 15px;
  }
  .bt-id { margin-left: auto; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-disabled); }
  .bt-more { font-size: var(--font-size-xs); color: var(--accent); }
</style>

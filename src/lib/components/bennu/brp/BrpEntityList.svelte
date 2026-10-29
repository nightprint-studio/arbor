<script lang="ts">
  /**
   * The entity hierarchy of the running game — a search box over a lazy tree.
   *
   * The tree is served level by level from the backend's snapshot, so opening a node with ten
   * thousand children costs one page, not ten thousand rows. Typing in the box swaps the tree for a
   * flat list of matches (a hit deep in the hierarchy has to be shown, and would be hidden under
   * collapsed ancestors); `Reveal in hierarchy` takes a hit back to its place in the tree.
   *
   * The box takes `has:Component` / `no:Component` words besides plain text, so filtering by
   * component needs no second control and no mouse.
   */
  import { Box, Copy, Plus, RefreshCw, Trash2, Unlink, LocateFixed } from 'lucide-svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import Alert from '$lib/components/shared/ui/Alert.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import PaneHeader from '$lib/components/shared/ui/PaneHeader.svelte';
  import SearchBar from '$lib/components/shared/ui/SearchBar.svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import type { MenuItem } from '$lib/components/shared/ContextMenu.svelte';
  import { bennuContextMenuStore } from '$lib/stores/bennu/contextmenu.svelte';
  import { bennuBrpStore as brp, type BrpLevel } from '$lib/stores/bennu/brp.svelte';
  import type { BrpRow } from '$lib/ipc/bennu/brp';
  import { entityTag } from './brp-value';

  let { onDespawn }: { onDespawn: (id: number) => void } = $props();

  /** A tree row: an entity, or the "load more" stub that ends a level cut at the page size. */
  interface EntNode {
    id: string;
    row?: BrpRow;
    more?: { parent?: number; remaining: number };
    children: EntNode[] | null;
  }

  function levelNodes(level: BrpLevel, parent?: number): EntNode[] {
    const out: EntNode[] = level.rows.map((row) => ({ id: String(row.id), row, children: childNodes(row) }));
    if (level.total > level.rows.length) {
      out.push({
        id: `more:${parent ?? 'root'}`,
        more: { parent, remaining: level.total - level.rows.length },
        children: [],
      });
    }
    return out;
  }

  /** `null` = has children that are not loaded yet (the Tree shows an expander and asks). */
  function childNodes(row: BrpRow): EntNode[] | null {
    if (row.child_count === 0) return [];
    const level = brp.children.get(row.id);
    return level ? levelNodes(level, row.id) : null;
  }

  const flat = $derived(brp.flat);
  const nodes = $derived<EntNode[]>(
    flat
      ? flat.rows.map((row) => ({ id: String(row.id), row, children: [] }))
      : levelNodes(brp.roots),
  );
  const total = $derived(flat ? flat.total : brp.roots.total);
  const expandedIds = $derived(new Set([...brp.expanded].map(String)));

  // The box shows what the store holds, so `reveal()` clearing the search clears the box too.
  let query = $state('');
  $effect(() => { query = brp.search; });

  function onSelect(n: EntNode) {
    if (n.more) void brp.loadMore(n.more.parent);
    else if (n.row) void brp.select(n.row.id);
  }

  function onContext(n: EntNode, e: MouseEvent) {
    const row = n.row;
    if (!row) return;
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

  function onRowKeydown(n: EntNode, e: KeyboardEvent) {
    if (e.key === 'Delete' && n.row && brp.status.can.despawn) {
      e.preventDefault();
      onDespawn(n.row.id);
    }
  }
</script>

<PaneHeader title="Entities" loading={brp.loadingTree}>
  {#snippet end()}
    <span class="el-count">{total}</span>
    {#if brp.status.can.spawn}
      <IconButton tooltip="Spawn an empty entity" size={22} onclick={() => void brp.spawn()}>
        <Plus size={13} />
      </IconButton>
    {/if}
    <IconButton tooltip="Refresh" size={22} onclick={() => void brp.reload()}>
      <RefreshCw size={13} />
    </IconButton>
  {/snippet}
</PaneHeader>

<div class="el-search">
  <SearchBar
    bind:query
    showRegex={false}
    showCounter={false}
    placeholder="Name or id · has:Component · no:Component"
    ariaLabel="Search entities"
    oninput={(v) => brp.setSearch(v)}
    onClear={() => brp.setSearch('')}
  />
</div>

{#if brp.searchNote}
  <Alert variant="warning" compact text={brp.searchNote} />
{/if}

<div class="el-body">
  {#if nodes.length === 0}
    <EmptyState
      message={flat ? 'No entity matches.' : 'The world has no entities.'}
      description={flat ? undefined : 'Entities appear here as the game spawns them.'}
    />
  {:else}
    <Tree
      {nodes}
      {expandedIds}
      onExpandToggle={(id, next) => void brp.toggle(Number(id), next)}
      hasChildren={(n) => !!n.row && n.row.child_count > 0}
      selectedId={brp.selected === null ? null : String(brp.selected)}
      {onSelect}
      onContextMenu={onContext}
      {onRowKeydown}
      toggleOnClick={false}
      ariaLabel="Entities"
    >
      {#snippet row({ node })}
        {#if node.more}
          <span class="el-more">Load {node.more.remaining} more…</span>
        {:else if node.row}
          <Box size={12} />
          <span class="el-name" class:unnamed={!node.row.name}>{node.row.name ?? 'Entity'}</span>
          <span class="el-id">{entityTag(node.row.id)}</span>
        {/if}
      {/snippet}
    </Tree>
  {/if}
</div>

<style>
  .el-search { padding: 6px 8px; flex: none; }
  .el-body { flex: 1; min-height: 0; overflow-y: auto; }
  .el-count { font-size: var(--font-size-2xs); color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .el-name { font-size: var(--font-size-sm); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .el-name.unnamed { color: var(--text-muted); font-style: italic; }
  .el-id { margin-left: auto; font-family: var(--font-code); font-size: var(--font-size-2xs); color: var(--text-muted); }
  .el-more { font-size: var(--font-size-xs); color: var(--accent); }
</style>

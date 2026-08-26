<script lang="ts">
  /**
   * The reviewable half of a bulk naming fix: what it would rename, narrowed however you like.
   *
   * A project-wide fix on a legacy tree reaches thousands of names, and "apply all or nothing" is
   * not a review — the whole reason to look is to disagree with part of it. So every name has a
   * tick, every group has a tick, and each kind of declaration can be switched off wholesale.
   *
   * The list is **windowed** (`VirtualList`): four thousand rows of real DOM is a modal that takes
   * seconds to open and stutters when scrolled. Headers and rows are laid out to the same height
   * so one flat array covers both.
   */
  import { FileCode2, Search } from 'lucide-svelte';
  import Checkbox from '$lib/components/shared/ui/Checkbox.svelte';
  import Input from '$lib/components/shared/ui/Input.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import VirtualList from '$lib/components/shared/ui/VirtualList.svelte';
  import EmptyState from '$lib/components/shared/ui/EmptyState.svelte';
  import ValueChange from '$lib/components/shared/ui/ValueChange.svelte';
  import type { RenamedName } from '$lib/ipc/bennu/naming';
  import {
    buildLines,
    indicesInGroup,
    targetCounts,
    type FixFilter,
    type FixLine,
    type GroupBy,
  } from './naming-fix-selection';

  interface Props {
    renamed: RenamedName[];
    /** The live filter. Bound, so the parent can read the selection it implies. */
    filter: FixFilter;
  }

  let { renamed, filter = $bindable() }: Props = $props();

  /** Every row is exactly this tall — the window arithmetic depends on it. */
  const ROW_H = 26;

  const kinds = $derived(targetCounts(renamed));
  const lines = $derived(buildLines(renamed, filter));

  const GROUPINGS: { value: GroupBy; label: string }[] = [
    { value: 'file', label: 'Group by file' },
    { value: 'target', label: 'Group by kind' },
    { value: 'none', label: 'No grouping' },
  ];

  /** Replace one field of the filter. A new object every time, so `$derived` re-runs — mutating a
   *  `Set` in place would leave the list showing the previous answer. */
  function update(patch: Partial<FixFilter>) {
    filter = { ...filter, ...patch };
  }

  function toggleKind(target: string, on: boolean) {
    const next = new Set(filter.hiddenTargets);
    if (on) next.delete(target);
    else next.add(target);
    update({ hiddenTargets: next });
  }

  function toggleGroup(key: string, on: boolean) {
    const next = new Set(filter.hiddenGroups);
    if (on) next.delete(key);
    else next.add(key);
    // A group ticked back on should come back whole, rather than staying half-empty because of
    // rows that were unticked individually before it was switched off.
    const excluded = new Set(filter.excluded);
    if (on) for (const i of indicesInGroup(renamed, filter, key)) excluded.delete(i);
    update({ hiddenGroups: next, excluded });
  }

  function toggleRow(index: number, on: boolean) {
    const next = new Set(filter.excluded);
    if (on) next.delete(index);
    else next.add(index);
    update({ excluded: next });
  }

  /** Tick or untick everything currently visible. */
  function setAll(on: boolean) {
    const excluded = new Set(filter.excluded);
    const hiddenGroups = new Set(filter.hiddenGroups);
    for (const line of lines) {
      if (line.kind !== 'item') continue;
      if (on) excluded.delete(line.index);
      else excluded.add(line.index);
    }
    if (on) for (const line of lines) if (line.kind === 'group') hiddenGroups.delete(line.key);
    update({ excluded, hiddenGroups });
  }

  const shownItems = $derived(lines.filter((l) => l.kind === 'item').length);
  const shownSelected = $derived(
    lines.filter((l): l is Extract<FixLine, { kind: 'item' }> => l.kind === 'item' && l.selected)
      .length,
  );
  const allShown = $derived(shownItems > 0 && shownSelected === shownItems);
  const someShown = $derived(shownSelected > 0 && shownSelected < shownItems);

  function lineKey(line: FixLine): string {
    return line.kind === 'group' ? `g:${line.key}` : `i:${line.index}`;
  }
</script>

<div class="review">
  <div class="toolbar">
    <Checkbox
      checked={allShown}
      indeterminate={someShown}
      disabled={shownItems === 0}
      ariaLabel="Select every name shown"
      onchange={setAll}
    />
    <div class="search">
      <Search size={12} />
      <Input
        value={filter.search}
        placeholder="Filter by name…"
        oninput={(v) => update({ search: v })}
      />
    </div>
    <Select
      value={filter.by}
      options={GROUPINGS}
      onchange={(v) => update({ by: v as GroupBy })}
      ariaLabel="Grouping"
    />
  </div>

  <!-- Each kind of declaration can be switched off wholesale: a run that should only touch methods
       does not need every local unticked one at a time. -->
  {#if kinds.length > 1}
    <div class="kinds">
      {#each kinds as k (k.target)}
        <Checkbox
          checked={!filter.hiddenTargets.has(k.target)}
          label={`${k.target} (${k.count})`}
          onchange={(on) => toggleKind(k.target, on)}
        />
      {/each}
    </div>
  {/if}

  {#if lines.length === 0}
    <EmptyState message="Nothing matches this filter." />
  {:else}
    <!-- A grid rather than a flex child: a grid item stretches to the track by default, which is
         how the windowed list gets a definite height to measure. -->
    <div class="list-area">
      <VirtualList
        items={lines}
        rowHeight={ROW_H}
        getKey={lineKey}
        role="list"
        ariaLabel="Names this fix would change"
      >
        {#snippet row({ item }: { item: FixLine })}
          {#if item.kind === 'group'}
            <div class="grp">
              <Checkbox
                checked={item.selected > 0}
                indeterminate={item.selected > 0 && item.selected < item.total}
                ariaLabel={`Select ${item.label}`}
                onchange={(on) => toggleGroup(item.key, on)}
              />
              {#if filter.by === 'file'}<FileCode2 size={12} />{/if}
              <span class="grp-label">{item.label}</span>
              <span class="grp-n">{item.selected}/{item.total}</span>
            </div>
          {:else}
            <div class="itm" class:off={!item.selected}>
              <Checkbox
                checked={item.selected}
                ariaLabel={`Rename ${item.name.from} to ${item.name.to}`}
                onchange={(on) => toggleRow(item.index, on)}
              />
              <ValueChange from={item.name.from} to={item.name.to} />
              {#if filter.by !== 'target'}<span class="meta">{item.name.target}</span>{/if}
              <span class="where" title={item.name.file}>
                {filter.by === 'file'
                  ? `:${item.name.line}`
                  : `${item.name.file.split(/[\\/]/).pop() ?? ''}:${item.name.line}`}
              </span>
            </div>
          {/if}
        {/snippet}
      </VirtualList>
    </div>
  {/if}
</div>

<style>
  .review { display: flex; flex-direction: column; gap: 8px; min-height: 0; flex: 1; }
  .list-area { flex: 1; min-height: 0; display: grid; }

  .toolbar { display: flex; align-items: center; gap: 8px; }
  .search { display: flex; align-items: center; gap: 6px; flex: 1; min-width: 0; color: var(--text-muted); }

  .kinds {
    display: flex; flex-wrap: wrap; gap: 4px 14px;
    padding: 6px 8px;
    background: var(--bg-elevated);
    border-radius: var(--radius-sm);
  }

  /* Both line kinds are exactly ROW_H tall — the window maths assumes it. */
  .grp, .itm {
    display: flex; align-items: center; gap: 8px;
    height: 26px;
    padding: 0 6px;
    font-size: 12px;
    box-sizing: border-box;
  }
  .grp {
    background: var(--bg-elevated);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-weight: 600;
  }
  .grp-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .grp-n { margin-left: auto; font-size: 10px; color: var(--text-muted); font-variant-numeric: tabular-nums; }

  .itm.off { opacity: 0.45; }
  .meta { font-size: 10px; color: var(--text-muted); }
  .where { margin-left: auto; font-size: 10px; color: var(--text-muted); }
</style>

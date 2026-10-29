<script lang="ts">
  /** Memory — sampled allocations by type and by site, and garbage-collection pauses. */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import { fmtBytes, fmtMs } from './profiler-format';

  let part = $state<'types' | 'sites' | 'gc'>('types');
  const view = $derived(store.view);

  const ALLOC_COLS: DataGridColumn[] = [
    { id: 'name', label: 'Name', width: 460 },
    { id: 'bytes', label: 'Allocated', type: 'number', width: 110 },
    { id: 'count', label: 'Samples', type: 'number', width: 90 },
  ];
  const GC_COLS: DataGridColumn[] = [
    { id: 'gc', label: 'Collector', width: 200 },
    { id: 'n', label: 'Collections', type: 'number', width: 100 },
    { id: 'total', label: 'Total pause', type: 'number', width: 110 },
    { id: 'max', label: 'Longest pause', type: 'number', width: 120 },
    { id: 'cause', label: 'Most frequent cause', width: 240 },
  ];

  const allocRows = $derived(
    ((part === 'types' ? view?.alloc_types : view?.alloc_sites) ?? []).map((a) => [a.name, a.bytes, a.count]),
  );
  const gcRows = $derived((view?.gc ?? []).map((g) => [g.collector, g.count, g.total_pause_ms, g.max_pause_ms, g.top_cause]));
  const hasAlloc = $derived((view?.alloc_types.length ?? 0) > 0);
</script>

<div class="mem">
  <div class="mem-bar">
    <Tabs
      size="sm"
      variant="pill"
      value={part}
      ariaLabel="Memory view"
      items={[{ id: 'types', label: 'Allocated types' }, { id: 'sites', label: 'Allocation sites' }, { id: 'gc', label: 'GC pauses' }]}
      onSelect={(id) => (part = id as typeof part)}
    />
    {#if !hasAlloc && part !== 'gc'}
      <span class="hint">No allocation samples — only JFR recordings carry them.</span>
    {/if}
  </div>
  {#if part === 'gc'}
    <DataGrid columns={GC_COLS} rows={gcRows} sortable ariaLabel="Garbage collection" emptyMessage="No garbage collections were recorded.">
      {#snippet cell({ value, column })}
        {#if column.id === 'total' || column.id === 'max'}{fmtMs(Number(value))}{:else}{value}{/if}
      {/snippet}
    </DataGrid>
  {:else}
    <DataGrid columns={ALLOC_COLS} rows={allocRows} sortable ariaLabel="Allocations" emptyMessage="No allocations were sampled.">
      {#snippet cell({ value, column })}
        {#if column.id === 'bytes'}{fmtBytes(Number(value))}{:else}{value}{/if}
      {/snippet}
    </DataGrid>
  {/if}
</div>

<style>
  .mem { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .mem-bar { display: flex; align-items: center; gap: 10px; padding: 4px 8px; }
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
</style>

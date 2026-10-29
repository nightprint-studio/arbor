<script lang="ts">
  /** Compare the open recording with an earlier one: which methods took a larger or smaller share of the run. */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import { fmtMs } from './profiler-format';

  const COLUMNS: DataGridColumn[] = [
    { id: 'method', label: 'Method', width: 420 },
    { id: 'delta', label: 'Change (points of the run)', type: 'number', width: 190 },
    { id: 'old', label: 'Before', type: 'number', width: 90 },
    { id: 'new', label: 'Now', type: 'number', width: 90 },
    { id: 'oldms', label: 'Before (self)', type: 'number', width: 110 },
    { id: 'newms', label: 'Now (self)', type: 'number', width: 110 },
  ];

  const others = $derived(store.recordings.filter((m) => m.id !== store.activeId && m.lang === store.activeMeta?.lang));
  const options = $derived([
    { value: '', label: others.length ? 'Compare with…' : 'No other recording of this language' },
    ...others.map((m) => ({ value: m.id, label: `${m.label} — ${new Date(m.created_ms).toLocaleString()}` })),
  ]);
  const rows = $derived(
    store.deltas.map((d) => [
      `${d.class}.${d.method}`,
      Number(d.delta_pp.toFixed(2)),
      Number(d.old_self_pct.toFixed(2)),
      Number(d.new_self_pct.toFixed(2)),
      d.old_self_ms,
      d.new_self_ms,
    ]),
  );
</script>

<div class="cmp">
  <div class="cmp-bar">
    <Select
      size="sm"
      value={store.compareWith ?? ''}
      {options}
      disabled={others.length === 0}
      onchange={(v) => void store.compare(v || null)}
    />
    <span class="hint">Positive = the method now takes a larger share of the run.</span>
  </div>
  <DataGrid
    columns={COLUMNS}
    {rows}
    sortable
    resizable
    ariaLabel="Recording comparison"
    emptyMessage="Pick an earlier recording to compare with."
  >
    {#snippet cell({ value, column })}
      {#if column.id === 'delta'}{Number(value) > 0 ? '+' : ''}{value} pp
      {:else if column.id === 'old' || column.id === 'new'}{value}%
      {:else if column.id === 'oldms' || column.id === 'newms'}{fmtMs(Number(value))}
      {:else}{value}{/if}
    {/snippet}
  </DataGrid>
</div>

<style>
  .cmp { display: flex; flex-direction: column; min-height: 0; flex: 1; }
  .cmp-bar { display: flex; align-items: center; gap: 10px; padding: 4px 8px; }
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
</style>

<script lang="ts">
  /**
   * Compare the open recording with an earlier one: which methods took a larger or smaller share of
   * the run. A method that now takes more is coloured on the self-time scale (the size of the
   * change, in points of the run); one that takes less is green.
   */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import Select from '$lib/components/shared/ui/Select.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import ProfilerFilterBar from './ProfilerFilterBar.svelte';
  import { fmtMs } from './profiler-format';
  import { severityColor, severityOf } from './profiler-filter';

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
  const visible = $derived.by(() => {
    const q = store.query.trim().toLowerCase();
    return store.deltas.filter((d) => store.keep(d.class) && (!q || `${d.class}.${d.method}`.toLowerCase().includes(q)));
  });
  const rows = $derived(
    visible.map((d) => [
      `${d.class}.${d.method}`,
      Number(d.delta_pp.toFixed(2)),
      Number(d.old_self_pct.toFixed(2)),
      Number(d.new_self_pct.toFixed(2)),
      d.old_self_ms,
      d.new_self_ms,
    ]),
  );

  /** Worse is coloured by size of the change; better is green; a negligible change keeps the default. */
  function changeColor(delta: number): string | undefined {
    if (delta < 0) return -delta >= store.thresholds.cpu_self_pct.yellow ? 'var(--success)' : undefined;
    return severityColor(severityOf(delta, store.thresholds.cpu_self_pct));
  }
</script>

<ProfilerFilterBar searchPlaceholder="Filter methods…">
  <Select
    size="sm"
    value={store.compareWith ?? ''}
    {options}
    disabled={others.length === 0}
    onchange={(v) => void store.compare(v || null)}
  />
  <span class="hint">Positive = the method now takes a larger share of the run.</span>
</ProfilerFilterBar>
<DataGrid
  columns={COLUMNS}
  {rows}
  sortable
  resizable
  ariaLabel="Recording comparison"
  emptyMessage={store.compareWith ? 'Nothing matches the filter.' : 'Pick an earlier recording to compare with.'}
>
  {#snippet cell({ value, column })}
    {#if column.id === 'delta'}
      <span class="delta" style:color={changeColor(Number(value))}>{Number(value) > 0 ? '+' : ''}{value} pp</span>
    {:else if column.id === 'old' || column.id === 'new'}{value}%
    {:else if column.id === 'oldms' || column.id === 'newms'}{fmtMs(Number(value))}
    {:else}{value}{/if}
  {/snippet}
</DataGrid>

<style>
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .delta { font-weight: 600; font-variant-numeric: tabular-nums; }
</style>

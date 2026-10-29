<script lang="ts">
  /** Lock contention — monitors threads waited on, longest total wait first. */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import { fmtMs } from './profiler-format';

  const COLUMNS: DataGridColumn[] = [
    { id: 'monitor', label: 'Monitor class', width: 280 },
    { id: 'site', label: 'Blocked most often in', width: 360 },
    { id: 'n', label: 'Waits', type: 'number', width: 80 },
    { id: 'total', label: 'Total wait', type: 'number', width: 100 },
    { id: 'max', label: 'Longest wait', type: 'number', width: 110 },
  ];
  const rows = $derived((store.view?.locks ?? []).map((l) => [l.monitor, l.site, l.count, l.total_ms, l.max_ms]));
</script>

<DataGrid
  columns={COLUMNS}
  {rows}
  sortable
  resizable
  ariaLabel="Lock contention"
  emptyMessage="No lock contention was recorded (JFR only reports waits over 20 ms)."
>
  {#snippet cell({ value, column })}
    {#if column.id === 'total' || column.id === 'max'}{fmtMs(Number(value))}{:else}{value}{/if}
  {/snippet}
</DataGrid>

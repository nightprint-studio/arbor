<script lang="ts">
  /** Hot methods — the recording's methods by self time. Enter / double-click opens the source. */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import { fmtMs, fmtPct } from './profiler-format';

  const COLUMNS: DataGridColumn[] = [
    { id: 'method', label: 'Method', width: 420 },
    { id: 'self', label: 'Self %', type: 'number', width: 80 },
    { id: 'selfms', label: 'Self time', type: 'number', width: 90 },
    { id: 'total', label: 'Total %', type: 'number', width: 80 },
    { id: 'totalms', label: 'Total time', type: 'number', width: 90 },
    { id: 'lines', label: 'Hottest lines', width: 180 },
  ];

  const view = $derived(store.view);
  const methods = $derived(view?.methods ?? []);
  const rows = $derived.by(() => {
    if (!view) return [];
    const total = Math.max(1, view.meta.samples);
    const k = view.meta.ms_per_unit;
    return methods.map((m) => [
      m.class ? `${m.class}.${m.method}` : m.method,
      Number(((m.self_w * 100) / total).toFixed(2)),
      Math.round(m.self_w * k),
      Number(((m.total_w * 100) / total).toFixed(2)),
      Math.round(m.total_w * k),
      m.hot_lines.slice(0, 4).map(([line, w]) => `${line} (${fmtPct((w * 100) / total)})`).join('  '),
    ]);
  });
  let selected = $state<number | null>(null);
</script>

<DataGrid
  columns={COLUMNS}
  {rows}
  bind:selectedRow={selected}
  sortable
  resizable
  ariaLabel="Hot methods"
  emptyMessage="No samples in this recording."
  onActivate={(i) => void store.goTo(methods[i])}
>
  {#snippet cell({ value, column })}
    {#if column.id === 'selfms' || column.id === 'totalms'}{fmtMs(Number(value))}
    {:else if column.id === 'self' || column.id === 'total'}{fmtPct(Number(value))}
    {:else}{value}{/if}
  {/snippet}
</DataGrid>

<script lang="ts">
  /**
   * Hot methods — the recording's methods by self time, each with a bar and a colour for how much of
   * the run it is. Enter / double-click opens the source.
   *
   * Self time is where the CPU actually was; total adds what the method called. A method with a big
   * total and a small self is a coordinator — look at what it calls.
   */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import ProfilerFilterBar from './ProfilerFilterBar.svelte';
  import ProfilerLegend from './ProfilerLegend.svelte';
  import { fmtMs, fmtPct } from './profiler-format';
  import { packageOf, severityColor, severityOf } from './profiler-filter';

  const COLUMNS: DataGridColumn[] = [
    { id: 'method', label: 'Method', width: 320 },
    { id: 'pkg', label: 'Package', width: 220 },
    { id: 'self', label: 'Self', type: 'number', width: 170, title: 'Share of the recording spent in the method itself' },
    { id: 'selfms', label: 'Self time', type: 'number', width: 90 },
    { id: 'total', label: 'Total', type: 'number', width: 80, title: 'Share including everything the method called' },
    { id: 'totalms', label: 'Total time', type: 'number', width: 90 },
    { id: 'lines', label: 'Hottest lines', width: 180 },
  ];

  const view = $derived(store.view);
  const simpleClass = (cls: string) => cls.slice(cls.lastIndexOf('.') + 1);

  /** The methods that pass the search and the class filter. Row `i` of the grid is `visible[i]`. */
  const visible = $derived.by(() => {
    const q = store.query.trim().toLowerCase();
    return (view?.methods ?? []).filter(
      (m) => store.keep(m.class) && (!q || `${m.class}.${m.method}`.toLowerCase().includes(q)),
    );
  });

  const total = $derived(Math.max(1, view?.meta.samples ?? 1));
  const rows = $derived.by(() => {
    if (!view) return [];
    const k = view.meta.ms_per_unit;
    return visible.map((m) => [
      m.class ? `${m.class}.${m.method}` : m.method,
      packageOf(m.class),
      Number(((m.self_w * 100) / total).toFixed(2)),
      Math.round(m.self_w * k),
      Number(((m.total_w * 100) / total).toFixed(2)),
      Math.round(m.total_w * k),
      m.hot_lines.slice(0, 4).map(([line, w]) => `${line} (${fmtPct((w * 100) / total)})`).join('  '),
    ]);
  });
  /** The bars are drawn against the hottest method on screen, so they stay readable on a flat profile. */
  const maxSelf = $derived(Math.max(0.01, ...visible.map((m) => (m.self_w * 100) / total)));
  let selected = $state<number | null>(null);
</script>

<ProfilerFilterBar searchPlaceholder="Filter methods…">
  <span class="legend"><ProfilerLegend levels={store.thresholds.cpu_self_pct} label="Self time" /></span>
  <span class="count">{visible.length} of {view?.methods.length ?? 0}</span>
</ProfilerFilterBar>

<DataGrid
  columns={COLUMNS}
  {rows}
  bind:selectedRow={selected}
  sortable
  resizable
  ariaLabel="Hot methods"
  emptyMessage={store.filterActive || store.query ? 'No method matches the filter.' : 'No samples in this recording.'}
  onActivate={(i) => visible[i] && void store.goTo(visible[i])}
>
  {#snippet cell({ value, column, rowIndex })}
    {@const m = visible[rowIndex]}
    {#if column.id === 'method' && m}
      <span class="name">{m.method}</span><span class="cls">{simpleClass(m.class)}</span>
    {:else if column.id === 'self'}
      {@const color = severityColor(severityOf(Number(value), store.thresholds.cpu_self_pct))}
      <span class="meter">
        <span class="bar"><ProgressBar pct={(Number(value) / maxSelf) * 100} height={6} {color} ariaLabel="Self share" /></span>
        <span class="pct" style:color>{fmtPct(Number(value))}</span>
      </span>
    {:else if column.id === 'selfms' || column.id === 'totalms'}{fmtMs(Number(value))}
    {:else if column.id === 'total'}{fmtPct(Number(value))}
    {:else}{value}{/if}
  {/snippet}
</DataGrid>

<style>
  .legend { margin-left: auto; }
  .count { font-size: var(--font-size-xs); color: var(--text-muted); white-space: nowrap; }
  .name { font-weight: 600; }
  .cls { margin-left: 6px; color: var(--text-muted); }
  .meter { display: flex; align-items: center; gap: 8px; width: 100%; }
  .bar { flex: 1; min-width: 30px; }
  .pct { min-width: 44px; text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; }
</style>

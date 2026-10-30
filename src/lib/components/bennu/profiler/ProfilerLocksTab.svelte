<script lang="ts">
  /**
   * Waits — where threads stood still instead of working.
   *
   * **I/O and parks** are time blocked off the CPU: a thread waiting for the database to answer, for a file, for a
   * pooled connection. The execution sampler never sees it — it samples threads that are *running*, so a method
   * whose whole cost is a slow query shows no CPU at all — which is why this exists. Each row names what was waited
   * on (`db.internal:5432`) and the method that waited longest.
   *
   * **Lock contention** is the monitors threads queued for, longest total wait first.
   *
   * Total wait is rated on the project's own scale, so a target that cost a second reads differently from one that
   * cost a few milliseconds.
   */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import ProfilerFilterBar from './ProfilerFilterBar.svelte';
  import ProfilerLegend from './ProfilerLegend.svelte';
  import { fmtMs } from './profiler-format';
  import { classOfMember, severityColor, severityOf } from './profiler-filter';

  type Part = 'waits' | 'locks';
  let part = $state<Part>('waits');
  const view = $derived(store.view);
  const hasWaits = $derived((view?.waits.length ?? 0) > 0);
  /** A recording with no wait events opens on the locks instead of an empty table. */
  const shown = $derived<Part>(part === 'waits' && !hasWaits && (view?.locks.length ?? 0) > 0 ? 'locks' : part);

  const WAIT_COLS: DataGridColumn[] = [
    { id: 'kind', label: 'Kind', width: 80 },
    { id: 'target', label: 'Waiting on', width: 280 },
    { id: 'site', label: 'Blocked most in', width: 340 },
    { id: 'n', label: 'Waits', type: 'number', width: 80 },
    { id: 'total', label: 'Total wait', type: 'number', width: 170 },
    { id: 'max', label: 'Longest wait', type: 'number', width: 110 },
  ];
  const LOCK_COLS: DataGridColumn[] = [
    { id: 'monitor', label: 'Monitor class', width: 280 },
    { id: 'site', label: 'Blocked most often in', width: 360 },
    { id: 'n', label: 'Waits', type: 'number', width: 80 },
    { id: 'total', label: 'Total wait', type: 'number', width: 170 },
    { id: 'max', label: 'Longest wait', type: 'number', width: 110 },
  ];

  const matches = (text: string) => {
    const q = store.query.trim().toLowerCase();
    return !q || text.toLowerCase().includes(q);
  };

  const waitsVisible = $derived((view?.waits ?? []).filter((w) => store.keep(classOfMember(w.site)) && matches(`${w.kind} ${w.target} ${w.site}`)));
  const waitRows = $derived(waitsVisible.map((w) => [w.kind, w.target, w.site, w.count, w.total_ms, w.max_ms]));
  const locksVisible = $derived(
    (view?.locks ?? []).filter((l) => (store.keep(l.monitor) || store.keep(classOfMember(l.site))) && matches(`${l.monitor} ${l.site}`)),
  );
  const lockRows = $derived(locksVisible.map((l) => [l.monitor, l.site, l.count, l.total_ms, l.max_ms]));
  const maxTotal = $derived(Math.max(1, ...(shown === 'waits' ? waitsVisible : locksVisible).map((r) => r.total_ms)));
  const levels = $derived(store.thresholds.lock_total_ms);
</script>

<ProfilerFilterBar searchPlaceholder="Filter waits…">
  <Tabs
    size="sm"
    variant="pill"
    value={shown}
    ariaLabel="Waits view"
    items={[
      { id: 'waits', label: 'I/O and parks', title: 'Time blocked off the CPU: sockets, files, parking' },
      { id: 'locks', label: 'Lock contention', title: 'Monitors threads queued for' },
    ]}
    onSelect={(id) => (part = id as Part)}
  />
  <span class="legend"><ProfilerLegend {levels} format={fmtMs} label="Total wait" /></span>
</ProfilerFilterBar>

{#snippet meter(value: number)}
  {@const color = severityColor(severityOf(value, levels))}
  <span class="meter">
    <span class="bar"><ProgressBar pct={(value / maxTotal) * 100} height={6} {color} ariaLabel="Total wait" /></span>
    <span class="pct" style:color>{fmtMs(value)}</span>
  </span>
{/snippet}

{#if shown === 'waits'}
  <DataGrid
    columns={WAIT_COLS}
    rows={waitRows}
    sortable
    resizable
    ariaLabel="Time blocked off the CPU"
    emptyMessage={store.filterActive || store.query
      ? 'Nothing matches the filter.'
      : 'No waits were recorded (JFR reports socket, file and park waits of 10 ms or more; a recording made before this tab existed has none).'}
  >
    {#snippet cell({ value, column })}
      {#if column.id === 'total'}{@render meter(Number(value))}
      {:else if column.id === 'max'}{fmtMs(Number(value))}{:else}{value}{/if}
    {/snippet}
  </DataGrid>
{:else}
  <DataGrid
    columns={LOCK_COLS}
    rows={lockRows}
    sortable
    resizable
    ariaLabel="Lock contention"
    emptyMessage={store.filterActive || store.query ? 'Nothing matches the filter.' : 'No lock contention was recorded (JFR only reports waits over 20 ms).'}
  >
    {#snippet cell({ value, column })}
      {#if column.id === 'total'}{@render meter(Number(value))}
      {:else if column.id === 'max'}{fmtMs(Number(value))}{:else}{value}{/if}
    {/snippet}
  </DataGrid>
{/if}

<style>
  .legend { margin-left: auto; }
  .meter { display: flex; align-items: center; gap: 8px; width: 100%; }
  .bar { flex: 1; min-width: 30px; }
  .pct { min-width: 56px; text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; }
</style>

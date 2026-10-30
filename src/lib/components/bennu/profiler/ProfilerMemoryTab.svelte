<script lang="ts">
  /**
   * Memory — who allocates, what is allocated, and garbage-collection pauses.
   *
   * **From your code** answers the question the other two cannot: JFR reports an allocation at the code that
   * performed it (`Arrays.copyOf`), which says nothing about which of *your* methods to change. This view walks each
   * allocation's stack up to the nearest frame of the project and credits it there — `Service.load` allocated
   * 30 MB of `byte[]` — and a click opens the method. It needs a recording that kept allocation stacks.
   *
   * Allocation rows are rated by their share of everything the recording allocated; a collector is rated by the
   * share of the run it paused the program for. Both scales are the project's thresholds, so the colours mean the
   * same here as in the other tabs.
   */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import ProgressBar from '$lib/components/shared/ui/ProgressBar.svelte';
  import Tabs from '$lib/components/shared/ui/Tabs.svelte';
  import { bennuProfilerStore as store } from '$lib/stores/bennu/profiler.svelte';
  import ProfilerFilterBar from './ProfilerFilterBar.svelte';
  import ProfilerLegend from './ProfilerLegend.svelte';
  import { fmtBytes, fmtMs, fmtPct } from './profiler-format';
  import { classOfMember, elementClass, severityColor, severityOf } from './profiler-filter';

  type Part = 'code' | 'types' | 'sites' | 'gc';
  let part = $state<Part>('code');
  const view = $derived(store.view);
  const hasStacks = $derived(view?.has_alloc_stacks ?? false);
  /** "From your code" needs allocation stacks; a recording without them opens on the types instead. */
  const shown = $derived<Part>(part === 'code' && !hasStacks ? 'types' : part);

  $effect(() => {
    if (shown === 'code') void store.loadAllocByCode();
  });

  const CODE_COLS: DataGridColumn[] = [
    { id: 'name', label: 'Method', width: 380 },
    { id: 'share', label: 'Share of allocations', type: 'number', width: 190 },
    { id: 'bytes', label: 'Allocated', type: 'number', width: 110 },
    { id: 'types', label: 'Mostly', width: 360, title: 'The types the bytes went to' },
  ];
  const ALLOC_COLS: DataGridColumn[] = [
    { id: 'name', label: 'Name', width: 420 },
    { id: 'share', label: 'Share of allocations', type: 'number', width: 190 },
    { id: 'bytes', label: 'Allocated', type: 'number', width: 110 },
    { id: 'count', label: 'Samples', type: 'number', width: 90 },
  ];
  const GC_COLS: DataGridColumn[] = [
    { id: 'gc', label: 'Collector', width: 200 },
    { id: 'n', label: 'Collections', type: 'number', width: 100 },
    { id: 'share', label: 'Paused for', type: 'number', width: 150, title: 'Share of the recording the program was stopped by this collector' },
    { id: 'total', label: 'Total pause', type: 'number', width: 110 },
    { id: 'max', label: 'Longest pause', type: 'number', width: 120 },
    { id: 'cause', label: 'Most frequent cause', width: 240 },
  ];

  // ── from your code ──────────────────────────────────────────────────────────────────────────
  const codeAll = $derived(store.allocByCode);
  const simpleClass = (cls: string) => cls.slice(cls.lastIndexOf('.') + 1);
  const codeVisible = $derived.by(() => {
    const q = store.query.trim().toLowerCase();
    return (codeAll?.rows ?? []).filter((r) => store.keep(r.class) && (!q || `${r.class}.${r.method}`.toLowerCase().includes(q)));
  });
  const codeTotal = $derived(Math.max(1, codeAll?.total_bytes ?? 1));
  const codeRows = $derived(
    codeVisible.map((r) => [
      r.class ? `${r.class}.${r.method}` : r.method,
      Number(((r.bytes * 100) / codeTotal).toFixed(2)),
      r.bytes,
      r.types.slice(0, 3).map(([t, b]) => `${t} ${fmtBytes(b)}`).join('  ·  '),
    ]),
  );
  const codeMaxShare = $derived(Math.max(0.01, ...codeVisible.map((r) => (r.bytes * 100) / codeTotal)));

  // ── by type and by site ─────────────────────────────────────────────────────────────────────
  /** The class an allocation row is about, for the class filter. */
  const classOf = (name: string) => (shown === 'types' ? elementClass(name) : classOfMember(name));

  const source = $derived((shown === 'types' ? view?.alloc_types : view?.alloc_sites) ?? []);
  /** Shares are of everything allocated, not of what survives the filter: hiding the JDK must not inflate the rest. */
  const allocTotal = $derived(Math.max(1, source.reduce((s, a) => s + a.bytes, 0)));
  const visible = $derived.by(() => {
    const q = store.query.trim().toLowerCase();
    return source.filter((a) => store.keep(classOf(a.name)) && (!q || a.name.toLowerCase().includes(q)));
  });
  const allocRows = $derived(
    visible.map((a) => [a.name, Number(((a.bytes * 100) / allocTotal).toFixed(2)), a.bytes, a.count]),
  );
  const maxShare = $derived(Math.max(0.01, ...visible.map((a) => (a.bytes * 100) / allocTotal)));

  const runMs = $derived(Math.max(1, view?.meta.duration_ms ?? 1));
  const gcRows = $derived(
    (view?.gc ?? []).map((g) => [g.collector, g.count, Number(((g.total_pause_ms * 100) / runMs).toFixed(2)), g.total_pause_ms, g.max_pause_ms, g.top_cause]),
  );
  const hasAlloc = $derived((view?.alloc_types.length ?? 0) > 0);
  let selectedCode = $state<number | null>(null);
</script>

{#if shown !== 'gc'}
  <ProfilerFilterBar searchPlaceholder="Filter allocations…">
    <span class="legend"><ProfilerLegend levels={store.thresholds.alloc_share_pct} label="Share of allocations" /></span>
  </ProfilerFilterBar>
{/if}
<div class="mem-bar">
  <Tabs
    size="sm"
    variant="pill"
    value={shown}
    ariaLabel="Memory view"
    items={[
      { id: 'code', label: 'From your code', disabled: !hasStacks, title: hasStacks ? 'Which of your methods allocates' : 'This recording has no allocation stacks — record again' },
      { id: 'types', label: 'Allocated types' },
      { id: 'sites', label: 'Allocation sites', title: 'Where the JDK performed the allocation — see “From your code” for who asked' },
      { id: 'gc', label: 'GC pauses' },
    ]}
    onSelect={(id) => (part = id as Part)}
  />
  {#if shown === 'gc'}
    <ProfilerLegend levels={store.thresholds.gc_pause_pct} label="Paused for" />
  {:else if !hasAlloc}
    <span class="hint">No allocation samples — only JFR recordings carry them.</span>
  {:else if !hasStacks}
    <span class="hint">This recording predates allocation stacks: the types and sites are all it can say. Record again for “From your code”.</span>
  {/if}
</div>

{#if shown === 'gc'}
  <DataGrid columns={GC_COLS} rows={gcRows} sortable resizable ariaLabel="Garbage collection" emptyMessage="No garbage collections were recorded.">
    {#snippet cell({ value, column })}
      {#if column.id === 'share'}
        {@const color = severityColor(severityOf(Number(value), store.thresholds.gc_pause_pct))}
        <span class="meter">
          <span class="bar"><ProgressBar pct={(Number(value) / (store.thresholds.gc_pause_pct.red * 1.5)) * 100} height={6} {color} ariaLabel="Pause share" /></span>
          <span class="pct" style:color>{fmtPct(Number(value))}</span>
        </span>
      {:else if column.id === 'total' || column.id === 'max'}{fmtMs(Number(value))}{:else}{value}{/if}
    {/snippet}
  </DataGrid>
{:else if shown === 'code'}
  <DataGrid
    columns={CODE_COLS}
    rows={codeRows}
    bind:selectedRow={selectedCode}
    sortable
    resizable
    ariaLabel="Allocations by the project's methods"
    emptyMessage={codeAll === null ? 'Reading the allocation stacks…' : store.filterActive || store.query ? 'Nothing matches the filter.' : 'No allocations were sampled.'}
    onActivate={(i) => {
      const r = codeVisible[i];
      if (r?.class) void store.goTo({ class: r.class, method: r.method, arity: r.arity, file: null, decl_line: null });
    }}
  >
    {#snippet cell({ value, column, rowIndex })}
      {@const r = codeVisible[rowIndex]}
      {#if column.id === 'name' && r}
        {#if r.class}<span class="name">{r.method}</span><span class="cls">{simpleClass(r.class)}</span>
        {:else}<span class="outside">{r.method}</span>{/if}
      {:else if column.id === 'share'}
        {@const color = severityColor(severityOf(Number(value), store.thresholds.alloc_share_pct))}
        <span class="meter">
          <span class="bar"><ProgressBar pct={(Number(value) / codeMaxShare) * 100} height={6} {color} ariaLabel="Allocation share" /></span>
          <span class="pct" style:color>{fmtPct(Number(value))}</span>
        </span>
      {:else if column.id === 'bytes'}{fmtBytes(Number(value))}{:else}{value}{/if}
    {/snippet}
  </DataGrid>
{:else}
  <DataGrid
    columns={ALLOC_COLS}
    rows={allocRows}
    sortable
    resizable
    ariaLabel="Allocations"
    emptyMessage={store.filterActive || store.query ? 'Nothing matches the filter.' : 'No allocations were sampled.'}
  >
    {#snippet cell({ value, column })}
      {#if column.id === 'share'}
        {@const color = severityColor(severityOf(Number(value), store.thresholds.alloc_share_pct))}
        <span class="meter">
          <span class="bar"><ProgressBar pct={(Number(value) / maxShare) * 100} height={6} {color} ariaLabel="Allocation share" /></span>
          <span class="pct" style:color>{fmtPct(Number(value))}</span>
        </span>
      {:else if column.id === 'bytes'}{fmtBytes(Number(value))}{:else}{value}{/if}
    {/snippet}
  </DataGrid>
{/if}

<style>
  .mem-bar { display: flex; align-items: center; gap: 10px; padding: 4px 8px; }
  .hint { font-size: var(--font-size-xs); color: var(--text-muted); }
  .legend { margin-left: auto; }
  .name { font-weight: 600; }
  .cls { margin-left: 6px; color: var(--text-muted); }
  .outside { color: var(--text-muted); font-style: italic; }
  .meter { display: flex; align-items: center; gap: 8px; width: 100%; }
  .bar { flex: 1; min-width: 30px; }
  .pct { min-width: 44px; text-align: right; font-variant-numeric: tabular-nums; font-weight: 600; }
</style>

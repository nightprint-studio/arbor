<script lang="ts" module>
  export type DataTableCellKind = 'text' | 'code' | 'pill' | 'datetime' | 'age';

  export interface DataTableColumn {
    key:    string;
    label:  string;
    /** CSS width — `'120px'`, `'1fr'`, `'minmax(80px, 1fr)'`. Default `'1fr'`. */
    width?: string;
    align?: 'left' | 'center' | 'right';
    /** Per-column rendering. Default 'text'. */
    kind?:  DataTableCellKind;
    /** Pill background colour (CSS expression). Used when kind = 'pill'. */
    color?: string;
    sortable?: boolean;
  }
</script>

<script lang="ts" generics="R extends Record<string, any> = Record<string, any>">
  /**
   * Generic data table — sortable columns, optional row click, scrollable
   * body, sticky header. No virtualization in v1 (good up to ~1k rows).
   *
   * Cell rendering is column-driven via `kind`:
   *   · text  (default) — plain string/number, monospace if numeric
   *   · code            — `var(--font-code)`
   *   · pill            — wraps value in a coloured pill (column.color or
   *                       row[`_${col.key}_color`] override)
   *   · datetime        — formats ISO-8601 with the locale's short pattern
   *   · age             — formats a number of days as 1d / 7d / 3mo / 1.2y
   *
   * Domain coupling: zero. Plugins use this via the `data_table` form-node.
   */
  import { ChevronUp, ChevronDown, ChevronsUpDown } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';

  type SortDir = 'asc' | 'desc';

  interface Props {
    columns:      DataTableColumn[];
    rows:         R[];
    /** Field used as a stable id (Svelte `(key)` and `onRowClick` payload). Default `'id'`. */
    rowKey?:      string;
    /** Body height — when set, the table scrolls inside this height. */
    height?:      number;
    /** Initial sort. Sortable columns also drive this on click. */
    initialSort?: { key: string; dir: SortDir };
    /** Plain text shown when `rows` is empty. */
    empty?:       string;
    onRowClick?:  (row: R) => void;
  }

  let {
    columns,
    rows,
    rowKey      = 'id',
    height,
    initialSort,
    empty       = 'No data',
    onRowClick,
  }: Props = $props();

  // ── Sort state — owned here, but the *initial* values follow the prop
  // when it changes (e.g. plugin sends a new node tree).
  // svelte-ignore state_referenced_locally
  let sortKey = $state<string | null>(initialSort?.key ?? null);
  // svelte-ignore state_referenced_locally
  let sortDir = $state<SortDir>(initialSort?.dir ?? 'asc');

  $effect(() => {
    sortKey = initialSort?.key ?? null;
    sortDir = initialSort?.dir ?? 'asc';
  });

  function toggleSort(col: DataTableColumn) {
    if (!col.sortable) return;
    if (sortKey === col.key) {
      sortDir = sortDir === 'asc' ? 'desc' : 'asc';
    } else {
      sortKey = col.key;
      sortDir = 'asc';
    }
  }

  function compareValues(a: unknown, b: unknown): number {
    if (a == null && b == null) return 0;
    if (a == null) return -1;
    if (b == null) return 1;
    const an = typeof a === 'number';
    const bn = typeof b === 'number';
    if (an && bn) return (a as number) - (b as number);
    return String(a).localeCompare(String(b), undefined, { numeric: true });
  }

  const sortedRows = $derived.by(() => {
    if (!sortKey) return rows;
    const key = sortKey;
    const dir = sortDir === 'asc' ? 1 : -1;
    return [...rows].sort((a, b) => compareValues(a[key], b[key]) * dir);
  });

  const gridTemplate = $derived(columns.map(c => c.width ?? '1fr').join(' '));

  // ── Cell formatters ────────────────────────────────────────────────────────
  function formatDateTime(v: unknown): string {
    if (v == null) return '';
    const d = new Date(String(v));
    if (Number.isNaN(d.getTime())) return String(v);
    return d.toLocaleString();
  }
  function formatAge(v: unknown): string {
    const d = typeof v === 'number' ? v : Number(v);
    if (!Number.isFinite(d)) return '';
    if (d < 1)   return '<1d';
    if (d < 30)  return `${Math.round(d)}d`;
    if (d < 365) return `${Math.round(d / 30)}mo`;
    return `${(d / 365).toFixed(1)}y`;
  }
  function cellText(row: R, col: DataTableColumn): string {
    const v = row[col.key];
    switch (col.kind) {
      case 'datetime': return formatDateTime(v);
      case 'age':      return formatAge(v);
      default:         return v == null ? '' : String(v);
    }
  }
  function pillColor(row: R, col: DataTableColumn): string {
    const override = row[`_${col.key}_color`];
    return typeof override === 'string' ? override : (col.color ?? 'var(--accent)');
  }
  /** Tint colour for non-pill cells. Same lookup as pills (`_<key>_color`
   *  row override → `col.color`) but returns null when neither is set so
   *  the cell falls back to the default text color. Empty / zero-valued
   *  cells stay un-tinted on purpose: a "0 critical" reading shouldn't
   *  shout in red. */
  function textColor(row: R, col: DataTableColumn): string | null {
    if (col.kind === 'pill') return null;
    const override = row[`_${col.key}_color`];
    const c = typeof override === 'string' ? override : (col.color ?? null);
    if (!c) return null;
    const v = row[col.key];
    if (v == null || v === '' || v === 0) return null;
    return c;
  }
</script>

<div class="dt-wrap" style:--dt-cols={gridTemplate} style:--dt-height={height ? `${height}px` : 'auto'}>
  <div class="dt-head" role="row">
    {#each columns as col (col.key)}
      <button
        class="dt-th"
        class:sortable={col.sortable}
        class:active={sortKey === col.key}
        class:right={col.align === 'right'}
        class:center={col.align === 'center'}
        type="button"
        disabled={!col.sortable}
        onclick={() => toggleSort(col)}
      >
        <span class="dt-th-label">{col.label}</span>
        {#if col.sortable}
          {#if sortKey === col.key}
            {#if sortDir === 'asc'}
              <ChevronUp size={11} />
            {:else}
              <ChevronDown size={11} />
            {/if}
          {:else}
            <ChevronsUpDown size={11} class="dt-th-sort-idle" />
          {/if}
        {/if}
      </button>
    {/each}
  </div>

  <div class="dt-body" class:scroll={!!height}>
    {#if sortedRows.length === 0}
      <div class="dt-empty">{empty}</div>
    {:else}
      {#each sortedRows as row (row[rowKey] ?? row)}
        {#snippet rowCells()}
          {#each columns as col (col.key)}
            <div class="dt-cell {col.kind ?? 'text'}" class:right={col.align === 'right'} class:center={col.align === 'center'}>
              {#if col.kind === 'pill'}
                <span class="dt-pill" style:--dt-pill-color={pillColor(row, col)}>{cellText(row, col)}</span>
              {:else}
                {@const tint = textColor(row, col)}
                <span class="dt-cell-text"
                      class:tinted={tint !== null}
                      style:color={tint ?? undefined}
                      use:tooltip={cellText(row, col)}>{cellText(row, col)}</span>
              {/if}
            </div>
          {/each}
        {/snippet}
        {#if onRowClick}
          <div
            class="dt-row clickable"
            role="button"
            tabindex="0"
            onclick={() => onRowClick(row)}
            onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onRowClick(row); } }}
          >
            {@render rowCells()}
          </div>
        {:else}
          <div class="dt-row" role="row">
            {@render rowCells()}
          </div>
        {/if}
      {/each}
    {/if}
  </div>
</div>

<style>
  .dt-wrap {
    display: flex;
    flex-direction: column;
    width: 100%;
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    overflow: hidden;
    font-family: var(--font-ui-sans);
  }

  .dt-head {
    display: grid;
    grid-template-columns: var(--dt-cols);
    border-bottom: 1px solid var(--border-subtle);
    background: var(--bg-overlay);
    position: sticky;
    top: 0;
    z-index: 1;
  }

  .dt-th {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 8px 12px;
    background: transparent;
    border: none;
    color: var(--text-secondary);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    text-align: left;
    cursor: default;
  }
  .dt-th.sortable { cursor: pointer; }
  .dt-th.sortable:hover { color: var(--text-primary); }
  .dt-th.active { color: var(--accent); }
  .dt-th.right  { justify-content: flex-end; text-align: right; }
  .dt-th.center { justify-content: center; text-align: center; }
  .dt-th-label { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  :global(.dt-th-sort-idle) { opacity: 0.5; }

  .dt-body {
    display: flex;
    flex-direction: column;
    max-height: var(--dt-height);
  }
  .dt-body.scroll { overflow-y: auto; }

  .dt-row {
    display: grid;
    grid-template-columns: var(--dt-cols);
    align-items: center;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-primary);
    font-size: 12px;
    transition: background var(--transition-fast);
  }
  .dt-row:last-child { border-bottom: none; }
  .dt-row.clickable { cursor: pointer; }
  .dt-row.clickable:hover { background: var(--bg-hover); }
  .dt-row.clickable:focus-visible {
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }

  .dt-cell {
    padding: 8px 12px;
    min-width: 0;
    overflow: hidden;
  }
  .dt-cell.right  { text-align: right; }
  .dt-cell.center { text-align: center; }
  .dt-cell.code   { font-family: var(--font-code); font-size: 11px; }

  .dt-cell-text {
    display: inline-block;
    max-width: 100%;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    vertical-align: middle;
  }
  .dt-cell-text.tinted {
    font-weight: 600;
  }

  .dt-pill {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--dt-pill-color) 15%, transparent);
    color: var(--dt-pill-color);
    border: 1px solid color-mix(in srgb, var(--dt-pill-color) 35%, transparent);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.4px;
    line-height: 1.4;
  }

  .dt-empty {
    padding: 24px 16px;
    text-align: center;
    color: var(--text-muted);
    font-size: 12px;
  }
</style>

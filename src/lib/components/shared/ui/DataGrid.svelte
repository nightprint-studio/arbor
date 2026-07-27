<script lang="ts" module>
  import type { Snippet } from 'svelte';

  /** A grid column. `type` only drives alignment + sort comparison. */
  export interface DataGridColumn {
    id: string;
    label: string;
    /** Right-aligns and compares numerically. */
    type?: 'text' | 'number';
    /** Initial width in px (the user can drag it afterwards). */
    width?: number;
    /** Secondary label shown beside the header (e.g. a SQL type). */
    hint?: string;
  }

  /** One cell. `null` is a genuine null and renders differently from `''`. */
  export type DataGridValue = string | number | boolean | null | undefined;

  export type SortDirection = 'asc' | 'desc';
</script>

<script lang="ts">
  /**
   * DataGrid — virtualised, sortable, filterable tabular viewport.
   *
   * Rows are fixed-height boxes inside a translateY window over a full-height
   * spacer, so only the visible slice (± overscan) is ever in the DOM: a result
   * set of 100k rows scrolls as smoothly as one of ten.
   *
   * Two rendering rules it exists to get right once, everywhere:
   *  • **NULL is not the empty string.** A null cell reads `NULL` in muted
   *    italics; an empty string shows a thin placeholder box. Confusing the two
   *    is a real, expensive mistake when you are writing DML from what you see.
   *  • **Numbers line up.** Numeric columns are right-aligned with tabular
   *    figures, so magnitudes are comparable by eye down the column.
   *
   * Keyboard: ↑/↓ move the selected row, PageUp/PageDown jump a viewport,
   * Home/End go to the ends, Enter fires `onActivate`. Column headers are
   * buttons — sorting never needs the mouse. Per-column filters live in an
   * optional second header row enabled with `filterable`.
   *
   * NOTE (shared/ui contract): no Arbor concepts, no IPC/stores, no imports from
   * shared/internal — generic props + snippets only.
   */
  interface Props {
    columns: DataGridColumn[];
    /** Row-major values, aligned with `columns`. */
    rows: DataGridValue[][];
    rowHeight?: number;
    overscan?: number;
    /** Leading gutter with the 1-based row ordinal. */
    showRowNumbers?: boolean;
    /** Clickable headers that cycle asc → desc → unsorted. */
    sortable?: boolean;
    /** Show the per-column filter row. */
    filterable?: boolean;
    /** Drag handles between headers. */
    resizable?: boolean;
    /** Index of the selected row (bindable, indexes `rows` not the view). */
    selectedRow?: number | null;
    /** Enter / double-click on a row. */
    onActivate?: (rowIndex: number) => void;
    /** Double-click on a cell while `editable`. */
    onEditCell?: (rowIndex: number, columnIndex: number) => void;
    editable?: boolean;
    emptyMessage?: string;
    ariaLabel?: string;
    class?: string;
    /** Override a cell's rendering (the default handles null/empty/number). */
    cell?: Snippet<[{ value: DataGridValue; column: DataGridColumn; rowIndex: number; columnIndex: number }]>;
    /** Replaces the built-in empty state. */
    empty?: Snippet;
  }

  let {
    columns,
    rows,
    rowHeight = 24,
    overscan = 12,
    showRowNumbers = true,
    sortable = true,
    filterable = false,
    resizable = true,
    selectedRow = $bindable(null),
    onActivate,
    onEditCell,
    editable = false,
    emptyMessage = 'No rows.',
    ariaLabel = 'Data grid',
    class: klass = '',
    cell,
    empty,
  }: Props = $props();

  // ── Sorting ───────────────────────────────────────────────────────────────
  let sortColumn = $state<string | null>(null);
  let sortDir = $state<SortDirection>('asc');

  function cycleSort(id: string) {
    if (sortColumn !== id) { sortColumn = id; sortDir = 'asc'; return; }
    if (sortDir === 'asc') { sortDir = 'desc'; return; }
    sortColumn = null;
  }

  // ── Filtering ─────────────────────────────────────────────────────────────
  let filters = $state<Record<string, string>>({});

  // ── Column widths ─────────────────────────────────────────────────────────
  // Seeded from the column definitions; the user's drags win afterwards.
  let widths = $state<Record<string, number>>({});
  const widthFor = (c: DataGridColumn) => widths[c.id] ?? c.width ?? 160;

  // WebView2 drops native HTML5 drag events, so the resize handle runs on plain
  // pointer events (the same choice the shared tab strip makes).
  let dragging: { id: string; startX: number; startW: number } | null = null;

  function startResize(e: PointerEvent, c: DataGridColumn) {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = { id: c.id, startX: e.clientX, startW: widthFor(c) };
  }
  function moveResize(e: PointerEvent) {
    if (!dragging) return;
    const next = Math.max(56, dragging.startW + (e.clientX - dragging.startX));
    widths = { ...widths, [dragging.id]: next };
  }
  function endResize(e: PointerEvent) {
    if (!dragging) return;
    (e.currentTarget as HTMLElement).releasePointerCapture(e.pointerId);
    dragging = null;
  }

  // ── Derived view ──────────────────────────────────────────────────────────
  /** Rows carry their original index so selection survives sorting/filtering. */
  const indexed = $derived(rows.map((r, i) => ({ row: r, index: i })));

  const filtered = $derived.by(() => {
    const active = Object.entries(filters).filter(([, v]) => v.trim() !== '');
    if (!active.length) return indexed;
    return indexed.filter(({ row }) =>
      active.every(([colId, needle]) => {
        const ci = columns.findIndex((c) => c.id === colId);
        if (ci < 0) return true;
        return String(row[ci] ?? '').toLowerCase().includes(needle.trim().toLowerCase());
      }),
    );
  });

  const view = $derived.by(() => {
    if (!sortColumn) return filtered;
    const ci = columns.findIndex((c) => c.id === sortColumn);
    if (ci < 0) return filtered;
    const numeric = columns[ci]?.type === 'number';
    const factor = sortDir === 'asc' ? 1 : -1;
    return [...filtered].sort((a, b) => {
      const av = a.row[ci];
      const bv = b.row[ci];
      // Nulls sort last in both directions — they are "no value", not a value.
      if (av === null || av === undefined) return bv === null || bv === undefined ? 0 : 1;
      if (bv === null || bv === undefined) return -1;
      if (numeric) return (Number(av) - Number(bv)) * factor;
      return String(av).localeCompare(String(bv)) * factor;
    });
  });

  // ── Virtualisation ────────────────────────────────────────────────────────
  let scrollEl = $state<HTMLElement | null>(null);
  let scrollTop = $state(0);
  let viewH = $state(0);

  const totalH = $derived(view.length * rowHeight);
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const end = $derived(Math.min(view.length, Math.ceil((scrollTop + viewH) / rowHeight) + overscan));
  const slice = $derived(view.slice(start, end));
  const offsetY = $derived(start * rowHeight);

  function onScroll(e: Event) {
    scrollTop = (e.currentTarget as HTMLElement).scrollTop;
  }

  /** Keep the selected row inside the viewport after a keyboard move. */
  function revealRow(displayIndex: number) {
    if (!scrollEl) return;
    const top = displayIndex * rowHeight;
    const bottom = top + rowHeight;
    if (top < scrollEl.scrollTop) scrollEl.scrollTop = top;
    else if (bottom > scrollEl.scrollTop + scrollEl.clientHeight) {
      scrollEl.scrollTop = bottom - scrollEl.clientHeight;
    }
  }

  function moveSelection(delta: number) {
    if (!view.length) return;
    const current = view.findIndex((r) => r.index === selectedRow);
    const next = Math.max(0, Math.min(view.length - 1, (current < 0 ? -1 : current) + delta));
    selectedRow = view[next].index;
    revealRow(next);
  }

  function onKeyDown(e: KeyboardEvent) {
    const page = Math.max(1, Math.floor(viewH / rowHeight) - 1);
    switch (e.key) {
      case 'ArrowDown': moveSelection(1); break;
      case 'ArrowUp':   moveSelection(-1); break;
      case 'PageDown':  moveSelection(page); break;
      case 'PageUp':    moveSelection(-page); break;
      case 'Home':      if (view.length) { selectedRow = view[0].index; revealRow(0); } break;
      case 'End':       if (view.length) { selectedRow = view[view.length - 1].index; revealRow(view.length - 1); } break;
      case 'Enter':     if (selectedRow !== null) onActivate?.(selectedRow); break;
      default: return;
    }
    e.preventDefault();
  }

  const gridTemplate = $derived(
    (showRowNumbers ? '52px ' : '') + columns.map((c) => `${widthFor(c)}px`).join(' '),
  );
</script>

<div class="dg {klass}">
  <!-- Header: sticky, one button per column, optional filter row underneath. -->
  <div class="dg-head" style:grid-template-columns={gridTemplate}>
    {#if showRowNumbers}
      <div class="dg-th dg-gutter-th" aria-hidden="true"></div>
    {/if}
    {#each columns as col (col.id)}
      <div class="dg-th" class:dg-num={col.type === 'number'}>
        {#if sortable}
          <button type="button" class="dg-sort" onclick={() => cycleSort(col.id)} aria-label={`Sort by ${col.label}`}>
            <span class="dg-th-label">{col.label}</span>
            {#if sortColumn === col.id}
              <span class="dg-sort-mark" aria-hidden="true">{sortDir === 'asc' ? '▲' : '▼'}</span>
            {/if}
          </button>
        {:else}
          <span class="dg-th-label">{col.label}</span>
        {/if}
        {#if col.hint}<span class="dg-th-hint">{col.hint}</span>{/if}
        {#if resizable}
          <span
            class="dg-resize"
            role="separator"
            aria-orientation="vertical"
            aria-label={`Resize ${col.label}`}
            onpointerdown={(e) => startResize(e, col)}
            onpointermove={moveResize}
            onpointerup={endResize}
          ></span>
        {/if}
      </div>
    {/each}
  </div>

  {#if filterable}
    <div class="dg-filters" style:grid-template-columns={gridTemplate}>
      {#if showRowNumbers}<div class="dg-filter-cell" aria-hidden="true"></div>{/if}
      {#each columns as col (col.id)}
        <div class="dg-filter-cell">
          <input
            class="dg-filter"
            type="text"
            placeholder="filter"
            aria-label={`Filter ${col.label}`}
            value={filters[col.id] ?? ''}
            oninput={(e) => (filters = { ...filters, [col.id]: e.currentTarget.value })}
          />
        </div>
      {/each}
    </div>
  {/if}

  <div
    class="dg-body"
    bind:this={scrollEl}
    onscroll={onScroll}
    bind:clientHeight={viewH}
    onkeydown={onKeyDown}
    role="grid"
    aria-label={ariaLabel}
    aria-rowcount={view.length}
    tabindex="0"
  >
    {#if !view.length}
      <div class="dg-empty">
        {#if empty}{@render empty()}{:else}<span>{emptyMessage}</span>{/if}
      </div>
    {:else}
      <div class="dg-spacer" style:height={`${totalH}px`}>
        <div class="dg-window" style:transform={`translateY(${offsetY}px)`}>
          {#each slice as entry, i (entry.index)}
            <div
              class="dg-row"
              class:dg-selected={selectedRow === entry.index}
              style:grid-template-columns={gridTemplate}
              style:height={`${rowHeight}px`}
              role="row"
              aria-rowindex={start + i + 1}
              aria-selected={selectedRow === entry.index}
              tabindex="-1"
              onclick={() => (selectedRow = entry.index)}
              ondblclick={() => onActivate?.(entry.index)}
            >
              {#if showRowNumbers}
                <span class="dg-cell dg-gutter" role="gridcell">{entry.index + 1}</span>
              {/if}
              {#each columns as col, ci (col.id)}
                {@const value = entry.row[ci]}
                <span
                  class="dg-cell"
                  class:dg-num={col.type === 'number'}
                  role="gridcell"
                  ondblclick={editable ? () => onEditCell?.(entry.index, ci) : undefined}
                >
                  {#if cell}
                    {@render cell({ value, column: col, rowIndex: entry.index, columnIndex: ci })}
                  {:else if value === null || value === undefined}
                    <span class="dg-null">NULL</span>
                  {:else if value === ''}
                    <span class="dg-blank" title="empty string"></span>
                  {:else}
                    {value}
                  {/if}
                </span>
              {/each}
            </div>
          {/each}
        </div>
      </div>
    {/if}
  </div>
</div>

<style>
  .dg {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    flex: 1;
    overflow: hidden;
    font-family: var(--font-code);
    font-size: 11.5px;
  }

  /* ── Header ─────────────────────────────────────────────────────────────── */
  .dg-head,
  .dg-filters,
  .dg-row {
    display: grid;
    align-items: stretch;
    min-width: max-content;
  }

  .dg-head {
    position: sticky;
    top: 0;
    z-index: 2;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .dg-th {
    position: relative;
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    padding: 0 8px;
    height: 26px;
    border-right: 1px solid var(--border-subtle);
    font-family: var(--font-ui-sans);
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .dg-th.dg-num { justify-content: flex-end; }
  .dg-gutter-th { border-right-color: var(--border); }

  .dg-sort {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    padding: 0;
    background: none;
    border: none;
    color: inherit;
    font: inherit;
    letter-spacing: inherit;
    text-transform: inherit;
    cursor: pointer;
  }
  .dg-sort:hover { color: var(--text-primary); }
  .dg-th-label { overflow: hidden; text-overflow: ellipsis; }
  .dg-sort-mark { font-size: 8px; color: var(--accent); }
  .dg-th-hint {
    font-family: var(--font-code);
    font-size: 9.5px;
    font-weight: 400;
    letter-spacing: 0;
    text-transform: none;
    color: var(--text-disabled);
  }

  .dg-resize {
    position: absolute;
    top: 0;
    right: -3px;
    bottom: 0;
    width: 6px;
    cursor: col-resize;
    touch-action: none;
    z-index: 3;
  }
  .dg-resize:hover { background: var(--accent-subtle); }

  /* ── Filter row ─────────────────────────────────────────────────────────── */
  .dg-filters {
    position: sticky;
    top: 26px;
    z-index: 2;
    background: var(--bg-elevated);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .dg-filter-cell {
    display: flex;
    align-items: center;
    padding: 2px 4px;
    border-right: 1px solid var(--border-subtle);
    min-width: 0;
  }
  .dg-filter {
    width: 100%;
    min-width: 0;
    height: 20px;
    padding: 0 6px;
    background: var(--bg-input);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-sm);
    color: var(--text-primary);
    font-family: var(--font-code);
    font-size: 10.5px;
    outline: none;
  }
  .dg-filter:focus { border-color: var(--border-focus); }
  .dg-filter::placeholder { color: var(--text-disabled); font-style: italic; }

  /* ── Body ───────────────────────────────────────────────────────────────── */
  .dg-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
    outline: none;
    position: relative;
  }
  .dg-body:focus-visible { box-shadow: inset 0 0 0 1px var(--border-focus); }

  .dg-spacer { position: relative; width: 100%; }
  .dg-window { position: absolute; top: 0; left: 0; right: 0; will-change: transform; }

  .dg-row { cursor: default; }
  .dg-row:hover { background: var(--bg-hover); }
  .dg-row.dg-selected { background: var(--bg-selected); }

  .dg-cell {
    display: flex;
    align-items: center;
    min-width: 0;
    padding: 0 8px;
    border-right: 1px solid var(--border-subtle);
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Numbers right-aligned with tabular figures so magnitudes line up. */
  .dg-cell.dg-num {
    justify-content: flex-end;
    font-variant-numeric: tabular-nums;
    color: var(--warning);
  }
  .dg-gutter {
    justify-content: flex-end;
    background: var(--bg-elevated);
    border-right-color: var(--border);
    color: var(--text-disabled);
    font-variant-numeric: tabular-nums;
    position: sticky;
    left: 0;
    z-index: 1;
  }

  /* NULL and the empty string must never look alike. */
  .dg-null { color: var(--text-disabled); font-style: italic; }
  .dg-blank {
    width: 14px;
    height: 9px;
    border: 1px dashed var(--border);
    border-radius: 2px;
    opacity: 0.7;
  }

  .dg-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 80px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: 12px;
  }
</style>

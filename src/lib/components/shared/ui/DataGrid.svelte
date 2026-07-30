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

  /**
   * A result larger than what is in memory — a window onto something of known
   * length, rather than the whole of something short.
   *
   * The grid drives it: it scales the scrollbar to `total`, draws placeholders for
   * the rows it does not have, and calls `request` before the viewport reaches a
   * gap. What a range costs, how a late reply is matched to its request and when a
   * far-away range is forgotten are the source's business, not the grid's.
   *
   * Deliberately free of any transport: `request` is a plain callback, so this
   * stays a `shared/ui` shape a non-Arbor consumer could satisfy from a fetch, a
   * worker or an array.
   */
  export interface DataGridWindowSource {
    /** Rows the result is believed to have — what the scrollbar is scaled to. */
    total: number;
    /**
     * `total` is an estimate rather than a count. The grid never prints the total
     * itself, but it does refuse to claim completeness on an approximate length —
     * consumers showing the number must mark it (`~`).
     */
    approximate?: boolean;
    /**
     * Every row is loaded AND `total` is exact. Sorting and filtering are
     * meaningful again at that point, and the grid re-enables them.
     */
    complete?: boolean;
    /** The row at an absolute index; `undefined` while it has not arrived. */
    rowAt: (index: number) => DataGridValue[] | undefined;
    /**
     * Ask for `[start, start + count)`. Called whenever the band around the
     * viewport contains a row that is missing, so it WILL be called repeatedly for
     * a range still in flight — the source is responsible for ignoring a repeat.
     */
    request: (start: number, count: number) => void;
    /** Rows asked for in one go. */
    chunk?: number;
    /** How far beyond the viewport to look for a gap before asking. */
    margin?: number;
  }
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
   * ## Two ways to feed it
   *
   *  • **`rows`** — the whole thing, in memory. Everything sorts and filters.
   *  • **`source`** — a {@link DataGridWindowSource}: a total, the rows loaded so
   *    far, and a callback asking for a range. The scrollbar is scaled to the
   *    total from the start, rows not yet loaded draw as placeholders instead of
   *    collapsing it, and the next range is asked for before the viewport reaches
   *    the edge of what is loaded.
   *
   * The two are exclusive and `source` wins. Sorting and filtering over a window
   * would order and hide a fraction of the result while looking like they had
   * ordered and hidden all of it, so while a source is incomplete both controls
   * stay visible and disabled, carrying the reason. The moment the source reports
   * `complete` they come back and behave exactly as in the array case.
   *
   * Keyboard: ↑/↓ move the selected row, PageUp/PageDown jump a viewport,
   * Home/End go to the ends, Enter fires `onActivate`. Column headers are
   * buttons — sorting never needs the mouse. Per-column filters live in an
   * optional second header row enabled with `filterable`. End on a windowed
   * source jumps to the last row of the RESULT and pulls the tail in, so reaching
   * the far end never needs the scrollbar.
   *
   * NOTE (shared/ui contract): no Arbor concepts, no IPC/stores, no imports from
   * shared/internal — generic props + snippets only.
   */
  import { untrack } from 'svelte';
  import DataCellValue from './DataCellValue.svelte';

  interface Props {
    columns: DataGridColumn[];
    /** Row-major values, aligned with `columns`. Ignored when `source` is given. */
    rows?: DataGridValue[][];
    /** Drive the grid from a window onto a larger result instead of an array. */
    source?: DataGridWindowSource;
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
    /**
     * Index of the selected row (bindable). Indexes `rows` in the array case and
     * the RESULT in the windowed one — in both, the thing the data is addressed
     * by, never the on-screen position.
     */
    selectedRow?: number | null;
    /** Enter / double-click on a row. */
    onActivate?: (rowIndex: number) => void;
    /** Double-click on a cell while `editable`. */
    onEditCell?: (rowIndex: number, columnIndex: number) => void;
    editable?: boolean;
    emptyMessage?: string;
    /** Why sorting and filtering are inert while a window is still filling. */
    partialNotice?: string;
    ariaLabel?: string;
    class?: string;
    /** Override a cell's rendering (the default handles null/empty/number). */
    cell?: Snippet<[{ value: DataGridValue; column: DataGridColumn; rowIndex: number; columnIndex: number }]>;
    /** Replaces the built-in empty state. */
    empty?: Snippet;
  }

  let {
    columns,
    rows = [],
    source,
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
    partialNotice = 'Only part of this result is loaded — sorting and filtering come back once all of it is.',
    ariaLabel = 'Data grid',
    class: klass = '',
    cell,
    empty,
  }: Props = $props();

  /**
   * A source that has everything folds back into the array path: once every row
   * is in memory and the length is exact, "windowed" is a fact about how the rows
   * arrived, not about what can be done with them.
   */
  const windowed = $derived(!!source && !source.complete);

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

  // ── Derived view (array path) ─────────────────────────────────────────────
  /**
   * The rows the array pipeline works on. A complete source is materialised once
   * here rather than duplicated down the file — sorting, filtering, selection and
   * keyboard movement then have exactly one implementation to be correct in.
   */
  const baseRows = $derived.by<DataGridValue[][]>(() => {
    if (!source) return rows;
    if (!source.complete) return [];
    const out: DataGridValue[][] = [];
    for (let i = 0; i < source.total; i += 1) out.push(source.rowAt(i) ?? []);
    return out;
  });

  /** Rows carry their original index so selection survives sorting/filtering. */
  const indexed = $derived(baseRows.map((r, i) => ({ row: r, index: i })));

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
  /**
   * How far the body has scrolled sideways — which the header has to be told.
   *
   * The header and the filter row are **siblings** of the scrolling body, not
   * children of it, because the body is virtualised: its rows live in an
   * absolutely-positioned window whose offset is computed from `scrollTop`, and a
   * header inside that container would sit above the window and put every row
   * index out by the height of itself.
   *
   * The consequence is that `position: sticky` cannot hold the header still —
   * there is no shared scroll container for it to stick inside. So the body
   * reports its horizontal position and the header is moved by the same amount.
   * Without this the columns and their headings drift apart the moment a result is
   * wider than the panel, which is most results.
   */
  let scrollLeft = $state(0);
  let viewH = $state(0);

  /** Rows the scrollbar spans: the result's length, not what is in memory. */
  const viewLength = $derived(windowed ? (source?.total ?? 0) : view.length);

  const totalH = $derived(viewLength * rowHeight);
  const start = $derived(Math.max(0, Math.floor(scrollTop / rowHeight) - overscan));
  const end = $derived(Math.min(viewLength, Math.ceil((scrollTop + viewH) / rowHeight) + overscan));

  /** What is drawn. `row: undefined` is a row that has not arrived yet. */
  const slice = $derived.by<{ row: DataGridValue[] | undefined; index: number }[]>(() => {
    if (!windowed) return view.slice(start, end);
    const src = source;
    if (!src) return [];
    const out: { row: DataGridValue[] | undefined; index: number }[] = [];
    for (let i = start; i < end; i += 1) out.push({ row: src.rowAt(i), index: i });
    return out;
  });

  const offsetY = $derived(start * rowHeight);

  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLElement;
    scrollTop = el.scrollTop;
    scrollLeft = el.scrollLeft;
  }

  /**
   * Ask for the next range before the viewport needs it.
   *
   * The band scanned is the visible slice widened by `margin` on both sides, and
   * the FIRST missing row in it is what gets asked for. Deriving the ask from the
   * gap rather than from the scroll position is what makes the repeat harmless:
   * while a range is in flight its rows are still missing, so this recomputes the
   * same `(start, count)` pair every time it re-runs, and the source recognises it.
   *
   * Reads only positions and `rowAt`, so it re-runs when the viewport moves and
   * when a range lands (closing a gap, or revealing the next one).
   */
  $effect(() => {
    const src = source;
    if (!src || src.complete || !src.total) return;
    const chunk = Math.max(1, src.chunk ?? 200);
    const margin = Math.max(0, src.margin ?? Math.floor(chunk / 5));
    const from = Math.max(0, start - margin);
    const to = Math.min(src.total, end + margin);
    let missing = -1;
    for (let i = from; i < to; i += 1) {
      if (src.rowAt(i) === undefined) { missing = i; break; }
    }
    if (missing < 0) return;
    // The call mutates the source; untracked so its own reads never become deps
    // of this effect (which would re-enter it on every arrival).
    untrack(() => src.request(missing, chunk));
  });

  // ── Selection & keyboard ──────────────────────────────────────────────────
  //
  // Display position and row index are the same number in the windowed case and
  // differ in the sorted/filtered one; these two are the only place that knows.
  function displayOf(rowIndex: number | null): number {
    if (rowIndex === null) return -1;
    if (windowed) return rowIndex < viewLength ? rowIndex : -1;
    return view.findIndex((r) => r.index === rowIndex);
  }

  function indexAt(display: number): number | null {
    if (windowed) return display >= 0 && display < viewLength ? display : null;
    return view[display]?.index ?? null;
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

  function selectDisplay(display: number) {
    const clamped = Math.max(0, Math.min(viewLength - 1, display));
    selectedRow = indexAt(clamped);
    revealRow(clamped);
  }

  function moveSelection(delta: number) {
    if (!viewLength) return;
    const current = displayOf(selectedRow);
    selectDisplay((current < 0 ? -1 : current) + delta);
  }

  function onKeyDown(e: KeyboardEvent) {
    const page = Math.max(1, Math.floor(viewH / rowHeight) - 1);
    switch (e.key) {
      case 'ArrowDown': moveSelection(1); break;
      case 'ArrowUp':   moveSelection(-1); break;
      case 'PageDown':  moveSelection(page); break;
      case 'PageUp':    moveSelection(-page); break;
      case 'Home':      if (viewLength) selectDisplay(0); break;
      case 'End':       if (viewLength) selectDisplay(viewLength - 1); break;
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
  <!-- Header: sticky, one button per column, optional filter row underneath.
       While a window is filling, the sort buttons stay in place and go inert:
       removing them would read as "this grid does not sort", which is a different
       and untrue statement. -->
  <div
    class="dg-head"
    style:grid-template-columns={gridTemplate}
    style:transform={`translateX(${-scrollLeft}px)`}
  >
    {#if showRowNumbers}
      <!-- Pushed back by exactly what the header row was pulled by, so it holds
           still over the body's own pinned gutter. Without it the header slid
           away as one piece while the row numbers stayed: after any horizontal
           scroll the label sitting above "1, 2, 3" was some other column's. -->
      <div
        class="dg-th dg-gutter-th"
        aria-hidden="true"
        style:transform={`translateX(${scrollLeft}px)`}
      ></div>
    {/if}
    {#each columns as col (col.id)}
      <div class="dg-th" class:dg-num={col.type === 'number'}>
        {#if sortable}
          <button
            type="button"
            class="dg-sort"
            disabled={windowed}
            title={windowed ? partialNotice : undefined}
            onclick={() => cycleSort(col.id)}
            aria-label={`Sort by ${col.label}`}
          >
            <span class="dg-th-label">{col.label}</span>
            {#if sortColumn === col.id && !windowed}
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
    <div
      class="dg-filters"
      style:grid-template-columns={gridTemplate}
      style:transform={`translateX(${-scrollLeft}px)`}
    >
      <!-- Held still over the pinned gutter, like the header cell above it. -->
      {#if showRowNumbers}
        <div
          class="dg-filter-cell dg-gutter-filter"
          aria-hidden="true"
          style:transform={`translateX(${scrollLeft}px)`}
        ></div>
      {/if}
      {#each columns as col (col.id)}
        <div class="dg-filter-cell">
          <input
            class="dg-filter"
            type="text"
            placeholder={windowed ? 'partial' : 'filter'}
            aria-label={`Filter ${col.label}`}
            disabled={windowed}
            title={windowed ? partialNotice : undefined}
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
    aria-rowcount={viewLength}
    tabindex="0"
  >
    {#if !viewLength}
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
              class:dg-row-pending={entry.row === undefined}
              style:grid-template-columns={gridTemplate}
              style:height={`${rowHeight}px`}
              role="row"
              aria-rowindex={start + i + 1}
              aria-selected={selectedRow === entry.index}
              aria-busy={entry.row === undefined}
              tabindex="-1"
              onclick={() => (selectedRow = entry.index)}
              ondblclick={() => onActivate?.(entry.index)}
            >
              {#if showRowNumbers}
                <!-- The ordinal is known before the row is: that is the point of
                     scaling the scrollbar to the total. -->
                <span class="dg-cell dg-gutter" role="gridcell">{entry.index + 1}</span>
              {/if}
              {#each columns as col, ci (col.id)}
                {#if entry.row === undefined}
                  <!-- A row on its way. A quiet bar, never `NULL`: a value that is
                       absent and a value that has not arrived are different facts. -->
                  <span class="dg-cell" class:dg-num={col.type === 'number'} role="gridcell">
                    <DataCellValue value={null} loading />
                  </span>
                {:else}
                  {@const value = entry.row[ci]}
                  <span
                    class="dg-cell"
                    class:dg-num={col.type === 'number'}
                    role="gridcell"
                    ondblclick={editable ? () => onEditCell?.(entry.index, ci) : undefined}
                  >
                    {#if cell}
                      {@render cell({ value, column: col, rowIndex: entry.index, columnIndex: ci })}
                    {:else}
                      <DataCellValue {value} />
                    {/if}
                  </span>
                {/if}
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
    font-size: var(--font-size-xs);
  }

  /* ── Header ─────────────────────────────────────────────────────────────── */
  .dg-head,
  .dg-filters,
  .dg-row {
    display: grid;
    align-items: stretch;
    min-width: max-content;
  }

  /* `relative`, not `sticky`: the header is not inside the scrolling body (see
     `scrollLeft`), so there is nothing for it to stick to — it is moved by script
     instead. Saying `sticky` here only ever looked like it was doing the job. */
  /* On `--bg-base`, not elevated.
     The toolbar above the grid is already elevated, and so were the column
     header and the filter row — three grey bands stacked one under the other
     with the data starting below them, which is most of why a table read as
     "all chrome". Grey belongs to the toolbar; the grid is content. The border
     is what separates the header now, which is all it needed. */
  .dg-head {
    position: relative;
    z-index: 2;
    background: var(--bg-base);
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
    font-size: var(--font-size-2xs);
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .dg-th.dg-num { justify-content: flex-end; }
  /* Stacked above the scrolling labels, and opaque, so they pass underneath it
     rather than through it while it holds its place. */
  .dg-gutter-th {
    position: relative;
    z-index: 1;
    background: var(--bg-elevated);
    border-right-color: var(--border);
  }

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
  .dg-sort:hover:not(:disabled) { color: var(--text-primary); }
  /* Inert while the window fills — present, visibly unavailable, and it says why
     on hover and on focus. */
  .dg-sort:disabled { cursor: default; color: var(--text-disabled); }
  .dg-th-label { overflow: hidden; text-overflow: ellipsis; }
  .dg-sort-mark { font-size: var(--font-size-3xs); color: var(--accent); }
  .dg-th-hint {
    font-family: var(--font-code);
    font-size: var(--font-size-3xs);
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
    position: relative;
    z-index: 2;
    background: var(--bg-base);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
  }
  .dg-gutter-filter {
    position: relative;
    z-index: 1;
    background: var(--bg-elevated);
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
    font-size: var(--font-size-2xs);
    outline: none;
  }
  .dg-filter:focus { border-color: var(--border-focus); }
  .dg-filter::placeholder { color: var(--text-disabled); font-style: italic; }
  .dg-filter:disabled { opacity: 0.5; cursor: default; }

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
  /* No hover affordance on a row there is nothing to interact with yet. */
  .dg-row-pending { cursor: progress; }
  .dg-row-pending:hover { background: none; }

  /* Vertical separators only.
     A `border-bottom` on every cell as well draws the full 1990s spreadsheet grid:
     a mesh of lines with the data trapped in it, where the loudest thing on screen
     is the furniture. Rows are told apart by the hover and the selection — which is
     how IntelliJ's own data grid does it, and the layout target for this window. */
  .dg-cell {
    display: flex;
    align-items: center;
    min-width: 0;
    padding: 0 8px;
    border-right: 1px solid var(--border-subtle);
    color: var(--text-primary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* Numbers right-aligned with tabular figures so magnitudes line up.
     Not coloured. They were `--warning`, so every id, count and price in every
     result came up alarm-orange — which is the product's own rule broken in the
     most visible place it has: the vivid accents mean **state**, never decoration.
     Nothing is wrong with the number 42. */
  .dg-cell.dg-num {
    justify-content: flex-end;
    font-variant-numeric: tabular-nums;
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

  /* How a null, an empty string and a not-yet-arrived row look lives in
     `DataCellValue` — one answer for this grid's default and for every consumer
     that overrides the `cell` snippet. */

  .dg-empty {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 80px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
  }
</style>

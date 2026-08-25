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
    /**
     * A colour identifying what this column belongs to — drawn as a bar across the
     * bottom of its header. Any CSS colour, typically a `var(--…)`.
     *
     * **The header, never the body.** A tint running down the column would cut the
     * uninterrupted horizontal band that makes the alternating rows readable: the two
     * are antagonistic, and following a row across forty columns is the job that
     * matters more. A consumer wanting to group columns visually gets the header bar
     * and nothing else, on purpose.
     */
    accent?: string;
    /**
     * Draw the accent bar **dashed** rather than solid.
     *
     * For a consumer whose grouping is inferred rather than known. A dashed line
     * already reads as provisional everywhere a person has met one, which makes it
     * the only marker that survives nobody reading the caption — and a caption is
     * exactly what does not get read when two colourings look identical.
     */
    accentProvisional?: boolean;
    /**
     * Push this column's header into the background — for a grid highlighting a
     * subset of its columns. Nothing is hidden and nothing moves: dimming is the only
     * form of emphasis that cannot make a reader think a column is missing.
     */
    muted?: boolean;
    /** Tooltip for the header cell, when the label alone does not say enough. */
    title?: string;
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
    /**
     * Fetch the rest of the result, in one act, so `complete` becomes true and
     * sorting and filtering come back.
     *
     * Optional, and absent means **cannot** rather than "not implemented": a source
     * whose result is bigger than what it is willing to hold at once must not offer
     * this, because the button would fetch forever and never hand the controls back.
     * The grid offers it only while `complete` is false.
     */
    loadAll?: () => void;
    /** A `loadAll` is running — the grid shows it and offers to stop. */
    loadingAll?: boolean;
    /** Stop a running `loadAll`; whatever arrived is kept. */
    stopLoadAll?: () => void;
    /** Rows asked for in one go. */
    chunk?: number;
    /** How far beyond the viewport to look for a gap before asking. */
    margin?: number;
  }
</script>

<script lang="ts">
  import { tooltip } from '$lib/actions/tooltip';

  /**
   * DataGrid — virtualised, sortable, filterable tabular viewport.
   *
   * Virtualised in **both** axes, because a SQL result is large in both. Rows are
   * fixed-height boxes inside a translateY window over a full-height spacer, so only
   * the visible slice (± overscan) is ever in the DOM: 100k rows scroll as smoothly
   * as ten. Columns are the same idea with a prefix sum instead of arithmetic (each
   * has its own width) and two empty spacer tracks holding the width of what is not
   * drawn — a 250-column `SELECT *` across a join costs the fifteen columns you can
   * see, not ten thousand cells per paint.
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
   * optional second header row enabled with `filterable`: a text box, and a
   * picker listing the values the column actually holds with their row counts
   * (`DataGridFilterCell`), so narrowing a result never requires knowing in
   * advance what is in it. End on a windowed
   * source jumps to the last row of the RESULT and pulls the tail in, so reaching
   * the far end never needs the scrollbar.
   *
   * NOTE (shared/ui contract): no Arbor concepts, no IPC/stores, no imports from
   * shared/internal — generic props + snippets only.
   */
  import { untrack } from 'svelte';
  import { ArrowDownToLine, Square } from 'lucide-svelte';
  import DataCellValue from './DataCellValue.svelte';
  import DataGridFilterCell from './DataGridFilterCell.svelte';
  import {
    distinctValues, isActiveFilter, valuePasses,
    type ColumnFilter, type DistinctSet,
  } from './data-grid-filter';

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
    /**
     * Right-click on a cell.
     *
     * The grid raises the event and names the cell; what a menu contains is the
     * host's business, because it depends on things this widget has no notion of —
     * whether the value can be written, whether it is a large object, what the
     * column means. The default menu is suppressed only when a handler is given,
     * so a grid without one still behaves like ordinary text.
     */
    onContextMenuCell?: (rowIndex: number, columnIndex: number, event: MouseEvent) => void;
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
    onContextMenuCell,
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

  /**
   * `partialNotice`, plus the way out when there is one.
   *
   * The notice states a fact and used to stop there, which left the reader with
   * "these come back once all of it is loaded" and no way to load all of it short
   * of dragging the scrollbar to the end. Where the button exists it is named; where
   * it does not — a result too large to hold whole — the fact stands alone, which is
   * the honest version.
   */
  const filterNotice = $derived(
    source?.loadAll
      ? `${partialNotice} Load all of it with the button at the head of this row.`
      : partialNotice,
  );

  // ── Column identity ───────────────────────────────────────────────────────
  //
  // **A column is its position, never its `id`.** The grid is positional by
  // construction — `rows` is row-major and aligned with `columns` — and `id` is a
  // name a consumer chose, which for a SQL result is the column name and therefore
  // NOT unique: `SELECT *` across a join of two legacy tables returns `TIPLAV`
  // twice, and one real result did so at positions 150 and 246 out of 247.
  //
  // That was fatal rather than cosmetic. Keying the header's `{#each}` by `id`
  // threw `each_key_duplicate` mid-render, which takes the whole panel — and with
  // it the tab — down; the query had run and its rows were in memory, and what the
  // user saw was a studio that had stopped answering. The quieter half was already
  // wrong too: sorting or filtering the second `TIPLAV` resolved through
  // `findIndex` and silently acted on the first.
  //
  // So sort, filters and widths are all keyed by index below, and `id` goes back to
  // being what the type says it is: a label a consumer reads in the `cell` snippet.
  /**
   * The column set as one value.
   *
   * Sorting by column 3 of the previous result, or a width dragged for a column
   * that is no longer there, are not opinions to carry over — but re-running the
   * SAME query must not throw away the widths somebody just dragged. Comparing the
   * shape tells the two apart.
   */
  const columnSignature = $derived(
    JSON.stringify(columns.map((c) => [c.id, c.label])),
  );

  // ── Sorting ───────────────────────────────────────────────────────────────
  let sortColumn = $state<number | null>(null);
  let sortDir = $state<SortDirection>('asc');

  function cycleSort(index: number) {
    if (sortColumn !== index) { sortColumn = index; sortDir = 'asc'; return; }
    if (sortDir === 'asc') { sortDir = 'desc'; return; }
    sortColumn = null;
  }

  // ── Filtering ─────────────────────────────────────────────────────────────
  /** One filter per column, by index. Sparse: an unfiltered column has no entry. */
  let filters = $state<Record<number, ColumnFilter>>({});

  /** Set or clear one column's filter, leaving the others alone. */
  function setFilter(ci: number, next: ColumnFilter | undefined) {
    const copy = { ...filters };
    if (next) copy[ci] = next;
    else delete copy[ci];
    filters = copy;
  }

  // ── Column widths ─────────────────────────────────────────────────────────
  // Seeded from the column definitions; the user's drags win afterwards.
  let widths = $state<Record<number, number>>({});
  const widthFor = (c: DataGridColumn, index: number) => widths[index] ?? c.width ?? 160;

  // A different column set starts clean — see `columnSignature`.
  $effect(() => {
    void columnSignature;
    untrack(() => {
      sortColumn = null;
      filters = {};
      widths = {};
    });
  });

  // WebView2 drops native HTML5 drag events, so the resize handle runs on plain
  // pointer events (the same choice the shared tab strip makes).
  let dragging: { index: number; startX: number; startW: number } | null = null;

  function startResize(e: PointerEvent, c: DataGridColumn, index: number) {
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = { index, startX: e.clientX, startW: widthFor(c, index) };
  }
  function moveResize(e: PointerEvent) {
    if (!dragging) return;
    const next = Math.max(56, dragging.startW + (e.clientX - dragging.startX));
    widths = { ...widths, [dragging.index]: next };
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

  /** The filters that would actually exclude something, paired with their column. */
  const activeFilters = $derived(
    Object.entries(filters)
      .map(([index, f]) => [Number(index), f] as const)
      .filter(([ci, f]) => ci < columns.length && isActiveFilter(f)),
  );

  const filtered = $derived.by(() => {
    if (!activeFilters.length) return indexed;
    return indexed.filter(({ row }) =>
      activeFilters.every(([ci, f]) => valuePasses(row[ci], f)),
    );
  });

  /**
   * The values in one column, for its picker — over the rows that pass every
   * **other** column's filter.
   *
   * Excluding this column's own filter is what makes a second visit to the picker
   * useful: a list narrowed by what you already picked from it would only ever
   * show you your own selection back. Excluding the *others* is what makes the
   * first pick useful — after choosing a region, the list of provinces should be
   * that region's provinces and not the country's.
   *
   * Called on demand, when a picker opens, and never as part of a render.
   */
  function distinctFor(ci: number): DistinctSet {
    const others = activeFilters.filter(([i]) => i !== ci);
    const scope = others.length
      ? baseRows.filter((row) => others.every(([i, f]) => valuePasses(row[i], f)))
      : baseRows;
    return distinctValues(scope, ci);
  }

  const view = $derived.by(() => {
    // `0` is a column, so the null check is explicit rather than falsy.
    if (sortColumn === null) return filtered;
    const ci = sortColumn;
    if (ci < 0 || ci >= columns.length) return filtered;
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
  let viewW = $state(0);

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

  // ── Column virtualisation ─────────────────────────────────────────────────
  //
  // The same idea as the rows, in the other axis, and it became load-bearing for the
  // same reason: a legacy `SELECT *` across a join is 250 columns wide, and drawing
  // every one of them for every visible row is ~10 000 cells per paint — for a
  // viewport that shows about fifteen of them. Scrolling such a result was slow in a
  // way that had nothing to do with how much data had arrived.
  //
  // The mechanics differ from the vertical case in one respect: rows are a uniform
  // `rowHeight`, so their window is arithmetic, while columns each have their own
  // width. So the offsets are a prefix sum, and the window is found by walking it.
  //
  // What holds the layout together is that the header, the filter row and every body
  // row take the SAME template and the SAME slice — a lead track for everything
  // scrolled past, the visible columns, then a trailing track for the rest. The two
  // spacer tracks keep the total width (and therefore the horizontal scrollbar, and
  // therefore `min-width: max-content`) exactly what it was when every column was
  // drawn.

  /** The pinned row-number track, which is not one of `columns`. */
  const gutterW = $derived(showRowNumbers ? 52 : 0);

  /** Where each column starts, in column space (the gutter excluded); `[n]` is the total. */
  const colOffsets = $derived.by(() => {
    const out = new Array<number>(columns.length + 1);
    let x = 0;
    for (let i = 0; i < columns.length; i += 1) {
      out[i] = x;
      x += widthFor(columns[i], i);
    }
    out[columns.length] = x;
    return out;
  });

  /**
   * How much beyond each edge of the viewport is drawn anyway.
   *
   * In pixels rather than in columns because a column's width is the user's to drag:
   * "two columns" is 90px of overscan on one result and 900 on another. This is the
   * budget that keeps a fling from showing a blank band before the next paint.
   */
  const COLUMN_OVERSCAN_PX = 400;

  /** `[first, last)` — the columns worth drawing at this scroll position. */
  const colWindow = $derived.by(() => {
    const n = columns.length;
    if (!n) return { first: 0, last: 0 };
    const offsets = colOffsets;
    // Before the first layout there is no width to measure against, and rendering
    // nothing would flash an empty grid; a generous guess is corrected on the next
    // frame, which is what the row window does with `viewH` too.
    const usable = (viewW > 0 ? viewW : 1200) + COLUMN_OVERSCAN_PX * 2;
    const from = scrollLeft - gutterW - COLUMN_OVERSCAN_PX;
    const to = from + usable;
    let first = 0;
    while (first < n - 1 && offsets[first + 1] <= from) first += 1;
    let last = first + 1;
    while (last < n && offsets[last] < to) last += 1;
    return { first, last };
  });

  /** The columns actually rendered, paired with their real index in `columns`. */
  const visibleColumns = $derived(
    columns.slice(colWindow.first, colWindow.last)
      .map((col, i) => ({ col, index: colWindow.first + i })),
  );

  /** Width of everything scrolled past, and of everything still to come. */
  const leadW = $derived(colOffsets[colWindow.first] ?? 0);
  const trailW = $derived(
    Math.max(0, (colOffsets[columns.length] ?? 0) - (colOffsets[colWindow.last] ?? 0)),
  );

  const gridTemplate = $derived(
    (showRowNumbers ? '52px ' : '')
      + `${leadW}px `
      + visibleColumns.map(({ col, index }) => `${widthFor(col, index)}px`).join(' ')
      + ` ${trailW}px`,
  );

  /** 1-based, and the row-number gutter counts as a column when it is shown. */
  const ariaColCount = $derived(columns.length + (showRowNumbers ? 1 : 0));
  const ariaColIndex = (index: number) => index + 1 + (showRowNumbers ? 1 : 0);

  /**
   * Which rows carry the faint band — see `.dg-alt`.
   *
   * Takes the **display position** (`start + i`), never the row's own index. Two
   * reasons, and both of them are bugs if you get it wrong: with the rows
   * virtualised, the position within the rendered slice would make the banding
   * crawl as you scroll; and after a sort or a filter the original indices are no
   * longer consecutive, so striping by them produces runs of two and three rows of
   * the same tint — a pattern that looks like it means something and does not.
   */
  const striped = (displayIndex: number) => displayIndex % 2 === 1;
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
    <!-- The columns scrolled past, as one empty track. An item auto-places into the
         first free track, so the spacer has to exist as an element or the first real
         header would land in it. -->
    <div class="dg-pad" aria-hidden="true"></div>
    <!-- Unkeyed on purpose: a column IS its position here (see `columnSignature`),
         and `col.id` is a consumer's name for it — a SQL result can carry the same
         one twice, which as a key is a fatal `each_key_duplicate`. -->
    {#each visibleColumns as { col, index: ci }}
      <!-- No `role="columnheader"`: the header is a SIBLING of the scrolling body
           (see `scrollLeft`), so it is not inside the `role="grid"` and a header role
           out there would describe a table that does not exist. The body's cells
           carry `aria-colindex` instead, which is what virtualisation actually
           requires — only a slice of them is in the DOM. -->
      <div
        class="dg-th"
        class:dg-num={col.type === 'number'}
        class:dg-muted={col.muted}
        class:dg-accented={!!col.accent}
        class:dg-provisional={!!col.accent && col.accentProvisional}
        style:--dg-accent={col.accent}
        use:tooltip={col.title ?? (windowed ? partialNotice : undefined)}
      >
        {#if sortable}
          <!-- One tooltip, on the cell, covering the whole header including the
               label and the resize handle. The button used to carry a duplicate: a
               *native* `title` on a child shadows an ancestor's, so with `title=`
               the cell's never appeared over the label. An action does not shadow —
               it fires — so keeping both would now mean two tooltips racing over one
               header. And the cell is the better host anyway: it is still live when
               the button goes `disabled`, which is exactly when `partialNotice` has
               something to explain. -->
          <button
            type="button"
            class="dg-sort"
            disabled={windowed}
            onclick={() => cycleSort(ci)}
            aria-label={`Sort by ${col.label}`}
          >
            <span class="dg-th-label">{col.label}</span>
            {#if sortColumn === ci && !windowed}
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
            onpointerdown={(e) => startResize(e, col, ci)}
            onpointermove={moveResize}
            onpointerup={endResize}
          ></span>
        {/if}
      </div>
    {/each}
    <!-- …and the columns not reached yet, so the row keeps its full width and the
         horizontal scrollbar spans the whole result rather than the visible slice. -->
    <div class="dg-pad" aria-hidden="true"></div>
  </div>

  {#if filterable}
    <div
      class="dg-filters"
      style:grid-template-columns={gridTemplate}
      style:transform={`translateX(${-scrollLeft}px)`}
    >
      <!-- Held still over the pinned gutter, like the header cell above it.
           It is also where the way OUT of the partial state lives: the filter row
           is where a reader finds out the controls are unavailable, so it is where
           the button that makes them available belongs. Telling someone sorting
           comes back "once all of it is loaded" and leaving them to drag the
           scrollbar there is not an answer. -->
      {#if showRowNumbers}
        <div
          class="dg-filter-cell dg-gutter-filter"
          style:transform={`translateX(${scrollLeft}px)`}
        >
          {#if windowed && source?.loadAll}
            <button
              type="button"
              class="dg-loadall"
              class:dg-loading={!!source.loadingAll}
              aria-label={source.loadingAll ? 'Stop loading the rest' : 'Load every row'}
              use:tooltip={source.loadingAll
                ? 'Loading the rest of the result — click to stop. What has arrived is kept.'
                : 'Load the whole result, so sorting and filtering come back'}
              onclick={() => (source?.loadingAll ? source?.stopLoadAll?.() : source?.loadAll?.())}
            >
              {#if source.loadingAll}
                <Square size={9} />
              {:else}
                <ArrowDownToLine size={12} />
              {/if}
            </button>
          {/if}
        </div>
      {/if}
      <div class="dg-pad" aria-hidden="true"></div>
      {#each visibleColumns as { col, index: ci }}
        <!-- The reason sits on the CELL, not on the controls inside it, and that is
             the whole point of it being here: they are `disabled` in exactly the case
             the tooltip explains, and a disabled control fires no pointer events — so
             a tooltip attached to one would be silent precisely when it has something
             to say. The wrapper is always live. -->
        <div class="dg-filter-cell" use:tooltip={windowed ? filterNotice : undefined}>
          <DataGridFilterCell
            label={col.label}
            filter={filters[ci]}
            disabled={windowed}
            distinct={() => distinctFor(ci)}
            onChange={(next) => setFilter(ci, next)}
          />
        </div>
      {/each}
      <div class="dg-pad" aria-hidden="true"></div>
    </div>
  {/if}

  <div
    class="dg-body"
    bind:this={scrollEl}
    onscroll={onScroll}
    bind:clientHeight={viewH}
    bind:clientWidth={viewW}
    onkeydown={onKeyDown}
    role="grid"
    aria-label={ariaLabel}
    aria-rowcount={viewLength}
    aria-colcount={ariaColCount}
    tabindex="0"
  >
    {#if !viewLength}
      <div class="dg-empty">
        <!-- A grid narrowed to nothing is not an empty grid, and saying "No rows"
             for both is how a filter gets left on: the result looks like it came
             back empty, and the thing that emptied it is a box two rows up that
             nobody is looking at. It says which state this is, and undoes it. -->
        {#if activeFilters.length && baseRows.length}
          <span>
            No row matches the {activeFilters.length === 1
              ? 'filter'
              : `${activeFilters.length} filters`} on this result.
          </span>
          <button type="button" class="dg-unfilter" onclick={() => (filters = {})}>
            Clear {activeFilters.length === 1 ? 'it' : 'them'}
          </button>
        {:else if empty}{@render empty()}{:else}<span>{emptyMessage}</span>{/if}
      </div>
    {:else}
      <div class="dg-spacer" style:height={`${totalH}px`}>
        <div class="dg-window" style:transform={`translateY(${offsetY}px)`}>
          {#each slice as entry, i (entry.index)}
            <div
              class="dg-row"
              class:dg-alt={striped(start + i)}
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
                <span class="dg-cell dg-gutter" role="gridcell" aria-colindex={1}>{entry.index + 1}</span>
              {/if}
              <span class="dg-pad" role="presentation"></span>
              {#each visibleColumns as { col, index: ci }}
                {#if entry.row === undefined}
                  <!-- A row on its way. A quiet bar, never `NULL`: a value that is
                       absent and a value that has not arrived are different facts. -->
                  <span
                    class="dg-cell"
                    class:dg-num={col.type === 'number'}
                   
                    role="gridcell"
                    aria-colindex={ariaColIndex(ci)}
                  >
                    <DataCellValue value={null} loading />
                  </span>
                {:else}
                  {@const value = entry.row[ci]}
                  <span
                    class="dg-cell"
                    class:dg-num={col.type === 'number'}
                   
                    role="gridcell"
                    aria-colindex={ariaColIndex(ci)}
                    tabindex={-1}
                    ondblclick={editable ? () => onEditCell?.(entry.index, ci) : undefined}
                    oncontextmenu={onContextMenuCell
                      ? (e) => onContextMenuCell(entry.index, ci, e)
                      : undefined}
                  >
                    {#if cell}
                      {@render cell({ value, column: col, rowIndex: entry.index, columnIndex: ci })}
                    {:else}
                      <DataCellValue {value} />
                    {/if}
                  </span>
                {/if}
              {/each}
              <span class="dg-pad" role="presentation"></span>
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

  /* The two spacer tracks that stand in for the columns outside the viewport.
     Nothing to draw — no border, no background — so a scrolled grid looks exactly
     like one that renders every column. `min-width: 0` because a grid item's
     automatic minimum size would otherwise refuse to shrink a 0px track. */
  .dg-pad {
    min-width: 0;
    border: none;
    background: none;
    pointer-events: none;
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

  /* The identity bar. Along the bottom edge, inside the cell, so it reads as
     underlining the label rather than as a border between two rows of chrome — and
     it stops at the header, which is the whole point (see `DataGridColumn.accent`).
     `::after` rather than a `border-bottom`: the cell already has a border on its
     right, and a coloured bottom border would join it at the corner and look like a
     box being drawn around the column. */
  .dg-th.dg-accented::after {
    content: '';
    position: absolute;
    left: 0;
    right: 1px; /* clears the cell's own right border, so bars do not touch */
    bottom: 0;
    height: 2px;
    background: var(--dg-accent, transparent);
    transition: opacity var(--transition-fast);
  }
  /* Inferred rather than known. A repeating gradient rather than a `border-style`,
     because the bar is a painted box: same geometry, same colour, and the only thing
     that changes is that it is no longer continuous. */
  .dg-th.dg-provisional::after {
    background: repeating-linear-gradient(
      to right,
      var(--dg-accent, transparent) 0 5px,
      transparent 5px 9px
    );
  }

  /* Dimmed rather than hidden: a column that vanished from the eye while its data
     stayed would read as the grid having filtered itself. */
  .dg-th.dg-muted { color: var(--text-disabled); }
  .dg-th.dg-muted::after { opacity: 0.25; }
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
    justify-content: center;
  }
  /* Accent, not neutral: while a result is partial this is the only thing on the
     row that can be pressed, and everything beside it is deliberately greyed. */
  .dg-loadall {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 20px;
    padding: 0;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--accent);
    cursor: pointer;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .dg-loadall:hover { background: var(--accent-subtle); }
  .dg-loadall.dg-loading { color: var(--error); }
  .dg-loadall.dg-loading:hover { background: var(--error-subtle); }
  .dg-filter-cell {
    display: flex;
    align-items: center;
    padding: 2px 4px;
    border-right: 1px solid var(--border-subtle);
    min-width: 0;
  }
  /* The box, the value picker and the clear button live in `DataGridFilterCell`,
     which is where a column filter's whole behaviour is now defined. What stays
     here is the cell that positions it in the grid. */

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

  /* ── The row band ───────────────────────────────────────────────────────────
     Every other row carries a faint wash. On a result wider than the window this is
     the only thing tying a value 200 columns along back to the row number it
     belongs to: the eye rides an uninterrupted horizontal bar instead of counting
     cells.

     Three decisions behind one declaration:

     • **Rows, not columns, and not both.** Banding the columns as well makes a
       checkerboard — and worse than noisy, it is antagonistic: the row bar works
       precisely because it is uninterrupted, and a column band crossing every other
       cell chops it into segments. The horizontal axis is the one that needs help
       here; the vertical one is served by the header staying put.

     • **Mixed from `--text-primary`, not a hard-coded white.** Themes here are
       user-supplied JSON, so a fixed white overlay is invisible on a light theme
       and wrong on a tinted one. Mixing the foreground gives a wash that always
       contrasts with its own background, in any theme, and needs no new token.
       `--grid-row-stripe` is honoured first for a theme that wants to tune it — or
       set it to `transparent` and turn the banding off.

     • **It loses to everything.** The three row backgrounds are a strict order —
       stripe < hover < selection — and the rules below are written in it, so the
       later one wins wherever specificity ties. Keep them adjacent and in this
       order; the `:hover` twin on the selection rule is what stops the (more
       specific) hover rule from greying out a row you have selected. */
  .dg-row.dg-alt {
    background: var(--grid-row-stripe, color-mix(in srgb, var(--text-primary) 3%, transparent));
  }
  /* Hover outranks the stripe — and a row whose values have not arrived has nothing
     to hover, so it is excluded here rather than reset afterwards. */
  .dg-row:not(.dg-row-pending):hover { background: var(--bg-hover); }
  /* Selection outranks both, hovered or not. */
  .dg-row.dg-selected,
  .dg-row.dg-selected:hover { background: var(--bg-selected); }
  /* Nothing to interact with yet. */
  .dg-row-pending { cursor: progress; }

  /* Vertical separators only.
     A `border-bottom` on every cell as well draws the full 1990s spreadsheet grid:
     a mesh of lines with the data trapped in it, where the loudest thing on screen
     is the furniture. Rows are told apart by the band, the hover and the selection —
     which is how IntelliJ's own data grid does it, and the layout target for this
     window. */
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
    gap: 8px;
    height: 100%;
    min-height: 80px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
  }
  .dg-unfilter {
    padding: 0;
    background: none;
    border: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }
  .dg-unfilter:hover { text-decoration: underline; }
</style>

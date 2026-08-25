<script lang="ts">
  /**
   * The rows themselves — the grid, and everything that is about a *column*.
   *
   * Split out of `QueryResultPanel` because this is where all the column arithmetic
   * lives: which columns are visible, where each one came from, which are masked,
   * and how a grid position maps back to a result column. That arithmetic is
   * index-based and easy to get subtly wrong, so it is worth having in one file with
   * nothing else in it.
   *
   * ## It renders rows; it does not open files
   *
   * Revealing a large object, replacing one, loading text into a cell — all three
   * are reported upward rather than done here. They are a flow with its own modals
   * and its own idea of how a row is addressed (`ResultCellFileFlow`), and a grid
   * that also owned that would be back to being the file this was split out of.
   */
  import DataGrid, { type DataGridColumn } from '$lib/components/shared/ui/DataGrid.svelte';
  import ResultCell from './ResultCell.svelte';
  import ResultEditBar from './ResultEditBar.svelte';
  import ColumnOriginLegend from './ColumnOriginLegend.svelte';
  import { readOrigins, type Origins } from './column-origins';
  import { openResultContextMenu } from './result-context-menu';
  import { asCell } from './result-cells';
  import { resultEditStore, type Editability } from '$lib/stores/picus/result-edit.svelte';
  import type { PicusResult } from '$lib/stores/picus/result.svelte';

  interface Props {
    result: PicusResult;
    editable: Editability;
    /**
     * Colour the columns by a **traced** lineage instead of by what the server
     * reported.
     *
     * When given it replaces the reported origins wholesale — one colouring at a
     * time, never two competing ones — and the bars are drawn dashed so the reader
     * can see at a glance that this one is deduced. `null` means "use what the
     * server said", which is the state until somebody presses Trace.
     */
    traced?: Origins | null;
    /** Open a masked value. */
    onReveal: (rowIndex: number, column: string) => void;
    /** Put a file's bytes into a large object. */
    onReplaceLob: (rowIndex: number, column: string) => void;
    /** Put a file's text into an ordinary column, as a pending edit. */
    onLoadText: (rowIndex: number, column: string) => void;
  }

  let { result, editable, traced = null, onReveal, onReplaceLob, onLoadText }: Props = $props();

  /**
   * How many of `result.columns` the grid shows: all of them, minus the row key
   * Picus spliced in to make a masked cell addressable.
   *
   * Those are the **trailing** columns, so dropping them leaves every visible column
   * at the same index it has in `result.columns` — the cell snippet and the edit
   * callback index the full list by the grid's `columnIndex` and need no remapping.
   *
   * Counted, not matched by name. The injected key is a real column name — `ID`, or
   * the table's primary key — and a result is free to already contain one:
   * `SELECT *` across a join brings back whatever both tables call their columns, so
   * a name filter would hide a column the user actually asked for while leaving the
   * injected one on screen. Position is what the contract promises; position is what
   * this reads.
   */
  const visibleColumnCount = $derived(
    Math.max(0, result.columns.length - (result.hiddenColumns?.length ?? 0)),
  );

  // ── Which table each column came from ───────────────────────────────────────
  //
  // The reading of it is `column-origins.ts`; what is here is only the highlight,
  // which is view state and belongs to the view.

  /**
   * The colouring in force: the traced lineage when there is one, otherwise what the
   * server reported.
   *
   * One or the other, never both. Two colourings on one grid would need the reader
   * to hold which bar meant which, and the whole point of the distinction is that it
   * survives not being thought about.
   */
  const origins = $derived(traced ?? readOrigins(result.columnSources, visibleColumnCount));
  /** A traced colouring is a deduction, and its bars say so. */
  const provisional = $derived(traced !== null);

  /** The table whose columns are being picked out, if any. */
  let pickedTable = $state<string | null>(null);

  /**
   * The highlight, but only while it still refers to something on screen.
   *
   * Derived rather than reset by an effect: running one statement after another
   * would otherwise leave a table name selected that the new result has never heard
   * of, and every column would be dimmed with no way to see why. Reading validity
   * off the current groups makes that state unrepresentable instead of merely
   * corrected a tick later.
   */
  const highlight = $derived(
    pickedTable && origins.groups.some((g) => g.table === pickedTable) ? pickedTable : null,
  );

  const gridColumns = $derived<DataGridColumn[]>(
    result.columns.slice(0, visibleColumnCount).map((c, i) => ({
      id: c.name,
      label: c.name,
      hint: c.type,
      type: /NUMBER|INT|NUMERIC|DECIMAL/i.test(c.type) ? 'number' : 'text',
      width: 180,
      accent: origins.colorByColumn.get(i),
      accentProvisional: provisional,
      // With no table picked, nothing is dimmed. With one picked, everything that
      // is not it recedes — computed columns included, so there is exactly one
      // bright class of column to look for rather than two competing ones.
      muted: highlight !== null && origins.tableByColumn.get(i) !== highlight,
      title: origins.labelByColumn.get(i),
    })),
  );

  /** Columns whose value was not fetched — their cells hold a size. */
  const masked = $derived(new Set(result.maskedColumns ?? []));
</script>

<div class="rp">
  <ResultEditBar onStore={() => void resultEditStore.storeActive()} />
  <!-- Only for a result drawing on more than one table, which most are
       not — see `column-origins.ts`. -->
  {#if origins.groups.length}
    <ColumnOriginLegend
      groups={origins.groups}
      selected={highlight}
      {provisional}
      onSelect={(table) => (pickedTable = table)}
    />
  {/if}
  <!-- Inset to line up with the legend above it. The legend floats by its own
       margin (`FloatingBar`), so the inset cannot live on the shared parent — the
       two would add up and the strip would sit six pixels right of the rows it
       describes. A wrapper whose only job is that alignment says so plainly; the
       alternative was a `:global` reaching into the grid. -->
  <div class="rp-grid">
  <DataGrid
    columns={gridColumns}
    source={result}
    filterable
    editable={editable.ok}
    onEditCell={(rowIndex, columnIndex) => {
      const column = result.columns[columnIndex];
      if (column) resultEditStore.begin(rowIndex, column.name);
    }}
    onContextMenuCell={(rowIndex, columnIndex, event) => {
      openResultContextMenu(event, {
        rowIndex,
        columnIndex,
        columns: result.columns,
        row: result.rowAt(rowIndex)?.map(asCell),
        maskedColumns: result.maskedColumns,
        editable: editable.ok,
        onReveal,
        onReplaceLob,
        onLoadText,
      });
    }}
    ariaLabel="Query results"
  >
    {#snippet cell({ value, rowIndex, columnIndex })}
      {@const name = result.columns[columnIndex]?.name ?? ''}
      {@const cellValue = asCell(value)}
      <ResultCell
        value={cellValue}
        masked={masked.has(name)}
        onReveal={() => onReveal(rowIndex, name)}
        edited={resultEditStore.edited(rowIndex, name)}
        editing={resultEditStore.editing?.rowIndex === rowIndex
          && resultEditStore.editing?.column === name}
        onCommit={(next) => resultEditStore.change(rowIndex, name, cellValue, next)}
        onCancel={() => resultEditStore.cancel()}
      />
    {/snippet}
  </DataGrid>
  </div>
</div>

<style>
  .rp {
    display: flex;
    flex-direction: column;
    min-height: 0;
    min-width: 0;
    flex: 1;
  }

  .rp-grid {
    display: flex;
    min-height: 0;
    min-width: 0;
    flex: 1;
    /* Off the header too, so the pane floats whether or not a legend is above it.
       Without this the grid butted against the header exactly when there was no
       legend — and the header has stopped drawing a line on the strength of this
       pane floating, so "sometimes" would have meant "sometimes nothing separates
       them at all". */
    padding-top: 3px;
    /* The same 6px the strips above float by, so the row numbers sit inside the
       panel rather than butting against its edge — the layout the whole app
       follows, where the gap that shows the background *is* the border.
       Left only: the grid scrolls sideways, and an inset on the trailing edge
       would cut the last column short of where the scrollbar says it ends. */
    padding-left: 6px;
  }
</style>

/**
 * Which table each result column came from, turned into something readable.
 *
 * ## The question this answers
 *
 * `SELECT *` across three tables produces forty columns whose names give no hint of
 * where they came from, and two of which are called `id`. Reading such a result
 * means holding the join in your head and counting. The backend already knows the
 * answer for every column (`ColumnSource`); this decides how much of it is worth
 * showing and in what colour.
 *
 * ## It shows nothing on a single-table result
 *
 * Which is most results. One colour on every column, and a legend with one entry,
 * is a decoration that says nothing — worse, it trains the eye to ignore a marker
 * that *does* mean something on the next query. So {@link readOrigins} returns no
 * groups below two distinct tables, and the panel above it renders no legend.
 *
 * The per-column **tooltip** is not subject to that rule: `comunicazioni.data_invio`
 * on a column aliased to `quando` is worth having whether or not there is a second
 * table in the query.
 *
 * ## Colour is an index into the palette, not an identity
 *
 * Slots are handed out in order of first appearance **within one result**, so the
 * same table can be a different colour in the next query. That is deliberate: a
 * stable per-table colour would need somewhere to live, would drift as tables come
 * and go, and would eventually collide anyway. What the reader needs is to tell
 * *these* columns apart from *those* ones, right now, with the legend beside them.
 *
 * The palette is the connection palette, which already excludes the greens — green
 * means "connected" everywhere in Picus and must not turn up meaning anything else.
 */

import type { ColumnSource } from '$lib/ipc/picus/db';
import { CONNECTION_COLOR_SLOTS } from '$lib/stores/picus/connections.svelte';

/** Below this many distinct tables there is nothing a colour could distinguish. */
export const MIN_TABLES_TO_COLOUR = 2;

/** One table appearing in a result, and how it is drawn. */
export interface OriginGroup {
  /** The relation's name — what the legend chip says. */
  table: string;
  /** CSS variable holding its colour for this result. */
  color: string;
  /** Positions in the visible column list that belong to it, in order. */
  columns: number[];
}

/** What the grid and the legend need to know about a result's origins. */
export interface Origins {
  /** The tables, in order of first appearance. Empty when there is nothing to say. */
  groups: OriginGroup[];
  /** Visible column index → its group's colour. Empty when `groups` is. */
  colorByColumn: Map<number, string>;
  /**
   * Visible column index → the relation it is read from.
   *
   * Separate from {@link labelByColumn} on purpose: a caller asking *which table is
   * this* must not have to recover the answer by splitting a display string, which
   * is the kind of shortcut that works until a name contains the separator.
   */
  tableByColumn: Map<number, string>;
  /** Visible column index → `table.column`, for the header tooltip. */
  labelByColumn: Map<number, string>;
}

const NOTHING: Origins = {
  groups: [],
  colorByColumn: new Map(),
  tableByColumn: new Map(),
  labelByColumn: new Map(),
};

/**
 * Read a result's column sources into groups, colours and labels.
 *
 * `visibleCount` is how many columns the grid actually shows: a key Picus spliced
 * into the projection sits at the end and is hidden, and it must not appear in the
 * legend as a table the user never mentioned. Filtered by `index` rather than by
 * slicing, because a source list is sparse — see {@link ColumnSource}.
 */
export function readOrigins(
  sources: ColumnSource[] | undefined,
  visibleCount: number,
): Origins {
  if (!sources?.length || visibleCount <= 0) return NOTHING;

  const labelByColumn = new Map<number, string>();
  const tableByColumn = new Map<number, string>();
  const order: string[] = [];
  const columnsByTable = new Map<string, number[]>();

  for (const source of sources) {
    if (source.index < 0 || source.index >= visibleCount || !source.table) continue;

    tableByColumn.set(source.index, source.table);
    // `name` is empty for a row address (`ctid`), where naming the table is the whole
    // of what can honestly be said.
    labelByColumn.set(source.index, source.name ? `${source.table}.${source.name}` : source.table);

    let columns = columnsByTable.get(source.table);
    if (!columns) {
      columns = [];
      columnsByTable.set(source.table, columns);
      order.push(source.table);
    }
    columns.push(source.index);
  }

  if (order.length < MIN_TABLES_TO_COLOUR) {
    // The labels survive: a single-table result still benefits from being told which
    // real column an alias stands for.
    return { groups: [], colorByColumn: new Map(), tableByColumn, labelByColumn };
  }

  const groups: OriginGroup[] = order.map((table, i) => ({
    table,
    color: colorFor(i),
    columns: columnsByTable.get(table) ?? [],
  }));

  const colorByColumn = new Map<number, string>();
  for (const group of groups) {
    for (const index of group.columns) colorByColumn.set(index, group.color);
  }

  return { groups, colorByColumn, tableByColumn, labelByColumn };
}

/**
 * The colour for the n-th table in a result.
 *
 * Wraps rather than running out. A join wide enough to wrap has already exceeded
 * what colour can distinguish — around five is the honest limit — and at that point
 * the legend is what the reader is using anyway; repeating a hue there is a smaller
 * failure than leaving later tables unmarked, which reads as "these came from
 * nowhere".
 */
export function colorFor(position: number): string {
  const slot = CONNECTION_COLOR_SLOTS[position % CONNECTION_COLOR_SLOTS.length];
  return `var(--ws-color-${slot})`;
}

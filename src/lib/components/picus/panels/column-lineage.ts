/**
 * Colouring the grid by where a column **really** comes from.
 *
 * The sibling of `column-origins.ts`, and everything about it is the same except
 * the one thing that matters: those origins are the server's own statement and
 * cannot be wrong, and these are a deduction from the views' SQL and can be. So the
 * two must never be told apart only by reading a table name.
 *
 * They are told apart by the **bar**: reported origins draw a solid one, a lineage
 * draws a dashed one, and the legend says which it is showing. A dashed line already
 * means "provisional" everywhere else a person has ever seen one, which is why it is
 * the right carrier for a distinction nobody will read a caption about.
 *
 * The palette without its greens, and the legend's shape, are shared with
 * `column-origins.ts` on purpose: a reader should not have to learn a second visual
 * language for the same question asked twice. The **floor** is the one thing that
 * differs — one table is worth colouring here and not there — and the reason is
 * written where the check is.
 */

import type { Lineage } from '$lib/ipc/picus/lineage';
import { baseRelation } from '$lib/ipc/picus/lineage';
import { colorFor, type OriginGroup, type Origins } from './column-origins';

/**
 * Read a lineage into the same shape the reported origins produce.
 *
 * Deliberately the *same* shape, so `ResultRowsPane` and the legend take one kind of
 * input and the choice between reported and deduced is made in exactly one place.
 *
 * `visibleCount` is how many columns the grid shows — a key Picus spliced into the
 * projection sits at the end and must not appear in a legend as a table the user
 * never mentioned. The lineage is **positional** here, unlike a `ColumnSource`,
 * because it is produced from the statement's own projection in order.
 */
export function originsFromLineage(
  lineage: Lineage | null | undefined,
  visibleCount: number,
): Origins {
  const empty: Origins = {
    groups: [],
    colorByColumn: new Map(),
    tableByColumn: new Map(),
    labelByColumn: new Map(),
  };
  if (!lineage?.columns.length || visibleCount <= 0) return empty;

  const tableByColumn = new Map<number, string>();
  const labelByColumn = new Map<number, string>();
  const order: string[] = [];
  const columnsByTable = new Map<string, number[]>();

  lineage.columns.forEach((trace, index) => {
    if (index >= visibleCount) return;

    // A computed or unfollowed column gets a label but no table: it belongs to no
    // one relation, and a legend that claimed otherwise would be the exact failure
    // this whole feature is built to avoid.
    if (trace.verdict !== 'resolved') {
      // Each verdict says its own thing. `split` in particular carries no `stopped`
      // — nothing went wrong, the answer is genuinely several — so falling through
      // to the reason would end the tooltip on a bare dash.
      const why =
        trace.verdict === 'derived'
          ? 'computed, no single source'
          : trace.verdict === 'split'
            ? `one of ${trace.reads.map((r) => r.relation).join(' / ')}, depending on the row`
            : trace.stopped;
      labelByColumn.set(index, `${trace.output} — ${why}`);
      return;
    }

    const table = baseRelation(trace);
    if (!table) return;
    tableByColumn.set(index, table);
    // The whole chain in the tooltip: the endpoint is the answer, but the route is
    // what tells you which view renamed it.
    labelByColumn.set(
      index,
      [trace.output, ...trace.hops.map((hop) => `${hop.relation}.${hop.column}`)].join('  ←  '),
    );

    let columns = columnsByTable.get(table);
    if (!columns) {
      columns = [];
      columnsByTable.set(table, columns);
      order.push(table);
    }
    columns.push(index);
  });

  // ONE table is enough here, unlike the reported origins — and the difference is
  // the point rather than an inconsistency.
  //
  // Reported origins appear unasked on every query, so marking the ordinary
  // single-table case would train the eye to ignore a bar that means something on
  // the next one. A lineage exists *because somebody asked for it*: it is never
  // noise, and showing nothing after they pressed Trace reads as the feature having
  // failed. A view over one table resolving entirely to that table is also exactly
  // the answer people are hunting for, so it is worth stating.
  //
  // With one group the legend chip still earns its click: selecting it dims the
  // columns that did **not** resolve — the computed and the unfollowed ones — which
  // is the distinction that matters once the table is known.
  if (order.length === 0) {
    // Nothing resolved at all. The labels still carry each column's reason, which is
    // the part worth having when there is no table to name.
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

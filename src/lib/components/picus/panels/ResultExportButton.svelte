<script lang="ts">
  /**
   * Take a result out of Picus — four renditions, two destinations.
   *
   * Three of them are the ordinary ones every grid gets ({@link ExportButton}).
   * The fourth is the one that only makes sense here: **`INSERT` statements**. A
   * user looking at rows on a test database is very often looking at rows they want
   * installed somewhere else, and the alternative to this button is the generator's
   * grid and a lot of copy-pasting.
   *
   * ## The INSERTs are written by the backend
   *
   * Not out of ceremony: whether `007` keeps its quotes and `15` loses them is
   * decided by the column's declared type, and the declared type is something only
   * the connection's schema knows. Joining strings here would produce SQL that is
   * right until the first account code with a leading zero — and it would have to
   * re-decide, in TypeScript, a rule this product deliberately keeps in one place.
   *
   * ## What "the rows" means
   *
   * A result is a window onto a held cursor, so the honest answer is "the rows that
   * are loaded", and the subject line says so whenever that is fewer than all of
   * them. Silently exporting 500 of 40 000 rows under a name that says nothing is
   * the kind of thing somebody discovers after sending the file.
   */
  import { Database, FileJson, FileSpreadsheet, FileText } from 'lucide-svelte';
  import ExportButton, {
    type Rendition,
  } from '$lib/components/shared/internal/ExportButton.svelte';
  import {
    exportRows,
    EXPORT_EXTENSION,
    type ExportColumn,
    type ExportFormat,
  } from '$lib/utils/tabular-export';
  import { rowsToInsert } from '$lib/ipc/picus/db';
  import { formatRowTotal, type PicusResult } from '$lib/stores/picus/result.svelte';
  import type { CellValue, Dialect } from '$lib/types/picus';

  interface Props {
    result: PicusResult | null;
    dialect: Dialect;
    /**
     * The table the rows came from, when it is knowable — a relation tab knows it,
     * a query tab has it inferred from its SQL. Empty disables the `INSERT`
     * rendition rather than inventing a name.
     */
    table: string;
  }

  let { result, dialect, table }: Props = $props();

  /**
   * The loaded rows, in order.
   *
   * Ranges can be sparse — the reader may have jumped to row 900 000 and left a
   * hole behind — so this collects what is actually held rather than assuming
   * `[0, loaded)`. A gap is skipped, not exported as blanks: a blank row in a CSV
   * reads as data.
   */
  const rows = $derived.by<CellValue[][]>(() => {
    if (!result) return [];
    const out: CellValue[][] = [];
    for (let i = 0; i < result.total; i += 1) {
      const row = result.rowAt(i);
      if (row) out.push(row);
    }
    return out;
  });

  const names = $derived(result?.columns.map((c) => c.name) ?? []);

  const columns = $derived<ExportColumn<CellValue[]>[]>(
    names.map((name, i) => ({ key: name, value: (row) => row[i] })),
  );

  /** "1,200 rows", or "1,200 of ~40,000 rows" while the cursor is still filling. */
  const subject = $derived.by(() => {
    const count = rows.length.toLocaleString();
    if (!result || result.complete) return `${count} row${rows.length === 1 ? '' : 's'}`;
    return `${count} of ${formatRowTotal(result)} loaded rows`;
  });

  function tabular(
    format: ExportFormat,
    label: string,
    subtitle: string,
    icon: Rendition['icon'],
  ): Rendition {
    return {
      id: format,
      label,
      subtitle,
      icon,
      extension: EXPORT_EXTENSION[format],
      text: () => exportRows(rows, columns, format),
    };
  }

  const renditions = $derived<Rendition[]>([
    tabular('csv', 'As CSV', 'For a spreadsheet', FileSpreadsheet),
    tabular('json', 'As JSON', 'One object per row', FileJson),
    tabular('markdown', 'As a Markdown table', 'For a ticket or a message', FileText),
    ...(table && result
      ? [
          {
            id: 'insert',
            label: 'As INSERT statements',
            subtitle: `Into ${table}, quoted for ${dialect === 'oracle' ? 'Oracle' : 'PostgreSQL'}`,
            icon: Database,
            extension: 'sql',
            // `null` for a cell the driver reported as NULL — the backend leaves
            // those out of the row, which is how it is told to write `NULL`
            // rather than an empty string.
            text: () =>
              rowsToInsert(
                result.connectionId,
                table,
                names,
                rows.map((row) => row.map((cell) => (cell === null ? null : String(cell)))),
                dialect,
              ),
          } satisfies Rendition,
        ]
      : []),
  ]);
</script>

<ExportButton
  {renditions}
  fileName={table ? `${table.toLowerCase()}-rows` : 'picus-result'}
  {subject}
  empty={!result || !rows.length}
  emptyTooltip="Run a query first — there are no rows to export"
  tooltip={table
    ? 'Take these rows out — as a table, or as INSERT statements for another database'
    : 'Take these rows out. INSERT statements need a single source table, which this query has none of'}
/>

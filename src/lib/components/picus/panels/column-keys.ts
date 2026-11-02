/**
 * The key icons a column header wears: a key for a primary-key column, a link for a
 * foreign-key one. Shared by the result grid and the table's data tab so the two draw
 * the same thing for the same column.
 *
 * Names are compared case-insensitively — PostgreSQL folds, Oracle shouts, and a result
 * column's name comes from the server in whichever the statement happened to produce.
 */

import { KeyRound, Link2 } from 'lucide-svelte';
import type { DataGridMark } from '$lib/components/shared/ui/DataGrid.svelte';
import type { TableInfo } from '$lib/types/picus';

export function columnKeyMarks(table: TableInfo | null | undefined, columnName: string): DataGridMark[] {
  if (!table || !columnName) return [];
  const upper = columnName.toUpperCase();
  const marks: DataGridMark[] = [];

  if (table.columns.some((c) => c.primaryKey && c.name.toUpperCase() === upper)) {
    marks.push({ icon: KeyRound, label: 'Primary key', color: 'var(--warning)' });
  }

  const fk = (table.foreignKeys ?? []).find((f) => f.columns.some((c) => c.toUpperCase() === upper));
  if (fk) {
    const at = fk.columns.findIndex((c) => c.toUpperCase() === upper);
    const target = fk.referencedColumns[at];
    marks.push({
      icon: Link2,
      label: `Foreign key → ${fk.referencedTable}${target ? `.${target}` : ''}`,
      color: 'var(--info)',
    });
  }
  return marks;
}

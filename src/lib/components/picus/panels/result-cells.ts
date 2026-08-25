/**
 * The bridge between what the grid holds and what a SQL cell is.
 *
 * `DataGridValue` is deliberately wider than `CellValue`: the grid is a
 * `shared/ui` widget that knows nothing about databases, so it admits `undefined`
 * and `boolean`. A result does not. The gap is narrowed **here**, in one function,
 * rather than widened everywhere the two meet — which is what keeps `null` and
 * `undefined` from quietly becoming interchangeable in a product whose entire
 * purpose is writing correct DML.
 */

import type { DataGridValue } from '$lib/components/shared/ui/DataGrid.svelte';
import type { CellValue } from '$lib/types/picus';

/**
 * A grid value as a SQL cell.
 *
 * `undefined` becomes `null` because a row that is present has no absent cells —
 * `undefined` only ever means "the grid asked past the end". Booleans become their
 * text because that is how the driver reports them, and a cell that reads `true` in
 * the grid but is the boolean `true` in an export would be two different values
 * wearing one face.
 */
export function asCell(value: DataGridValue): CellValue {
  if (value === undefined) return null;
  return typeof value === 'boolean' ? String(value) : value;
}

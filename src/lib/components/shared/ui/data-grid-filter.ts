/**
 * What a `DataGrid` column is filtered by, and how a filter is answered.
 *
 * Split out of `DataGrid.svelte` because two components need it — the grid, which
 * applies filters, and `DataGridFilterCell`, which edits one — and a shape shared
 * by two files belongs to neither of them.
 */
import type { DataGridValue } from './DataGrid.svelte';

/**
 * A column filter, in one of two moods.
 *
 * They are alternatives rather than layers: a column is filtered by *text* or by a
 * *set of values*, never by both. Two filters on one column would have to be ANDed,
 * and an AND of "contains rom" and "is one of {ROMA, MILANO}" is a rule nobody
 * holds in their head while reading a grid.
 */
export type ColumnFilter =
  /** A case-insensitive substring of the value as rendered. */
  | { kind: 'text'; needle: string }
  /**
   * Membership in a chosen set, by {@link keyOf}. Exact, not substring — picking
   * `ROMA` from a list must not also bring back `ROMANO`, or the list would be
   * lying about what it selected.
   */
  | { kind: 'values'; picked: Set<string> };

/**
 * The key a value is counted and matched under.
 *
 * Every non-null value is prefixed, so no value can ever collide with the null
 * key however it renders — a column genuinely containing the string `NULL` is a
 * real thing in a legacy database, and it is not the same as a null.
 */
export const NULL_KEY = 'n';
export const keyOf = (v: DataGridValue): string =>
  v === null || v === undefined ? NULL_KEY : `v${String(v)}`;

/** One entry of a column's value list. */
export interface DistinctValue {
  key: string;
  /** What to print. Empty string for the empty string — the picker marks it. */
  label: string;
  isNull: boolean;
  count: number;
}

export interface DistinctSet {
  values: DistinctValue[];
  /**
   * The column has more than {@link MAX_DISTINCT} distinct values, so the list is
   * the first {@link MAX_DISTINCT} met and the counts do not add up to the number
   * of rows scanned. Said out loud in the picker rather than quietly truncated.
   */
  truncated: boolean;
  /** Rows looked at — the ones passing every *other* column's filter. */
  scanned: number;
}

/**
 * Ceiling on how many distinct values are collected.
 *
 * Not a rendering limit — the picker windows its own list — but a bound on the
 * work and the memory one click can cost. A key column of 400k rows has 400k
 * distinct values, and building that map is both pointless (nobody picks from it)
 * and slow enough to be felt.
 */
export const MAX_DISTINCT = 5000;

/** Whether a filter would actually exclude anything. */
export function isActiveFilter(f: ColumnFilter | undefined): f is ColumnFilter {
  if (!f) return false;
  return f.kind === 'text' ? f.needle.trim() !== '' : f.picked.size > 0;
}

/** Does one cell pass one filter? Assumes the filter is active. */
export function valuePasses(value: DataGridValue, f: ColumnFilter): boolean {
  if (f.kind === 'text') {
    return String(value ?? '').toLowerCase().includes(f.needle.trim().toLowerCase());
  }
  return f.picked.has(keyOf(value));
}

/**
 * The distinct values of column `ci`, with their counts.
 *
 * `rows` is expected to be pre-narrowed by the caller to what passes every *other*
 * column's filter — the spreadsheet behaviour, and the one that makes a second
 * pick useful: after choosing a region, the list of provinces should be that
 * region's provinces.
 *
 * Ordered the way the grid's own sort orders: nulls last, everything else
 * ascending and numerically aware, so `9` comes before `10`.
 */
export function distinctValues(rows: DataGridValue[][], ci: number): DistinctSet {
  const seen = new Map<string, DistinctValue>();
  let truncated = false;
  for (const row of rows) {
    const v = row[ci];
    const key = keyOf(v);
    const hit = seen.get(key);
    if (hit) { hit.count += 1; continue; }
    if (seen.size >= MAX_DISTINCT) { truncated = true; continue; }
    seen.set(key, {
      key,
      label: v === null || v === undefined ? 'NULL' : String(v),
      isNull: v === null || v === undefined,
      count: 1,
    });
  }

  const collator = new Intl.Collator(undefined, { numeric: true, sensitivity: 'base' });
  const values = [...seen.values()].sort((a, b) => {
    if (a.isNull) return b.isNull ? 0 : 1;
    if (b.isNull) return -1;
    return collator.compare(a.label, b.label);
  });
  return { values, truncated, scanned: rows.length };
}

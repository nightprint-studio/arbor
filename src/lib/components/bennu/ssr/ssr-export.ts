/**
 * The results of a structural search, taken out as CSV / JSON / Markdown.
 *
 * Two shapes because the answer has two: a grouped query produced a **table** and its rows are what you would
 * paste into a spreadsheet, an ungrouped one produced **places** and those are the rows. Exporting the places
 * under a grouped query would hand back the thing the query deliberately summarised.
 *
 * `undecided` is a column of its own rather than folded into the count: a total that silently included what the
 * classpath could not decide is a number that looks complete and is not.
 */

import { exportRows, type ExportFormat } from '$lib/utils/tabular-export';
import type { SsrHit, SsrReport } from '$lib/ipc/bennu/ssr';

export function exportResults(report: SsrReport | null, hits: readonly SsrHit[], format: ExportFormat): string {
  if (report?.groupedBy) {
    return exportRows(
      report.rows,
      [
        { key: report.groupedBy, value: (r) => r.key },
        { key: 'count', value: (r) => r.count },
        { key: 'files', value: (r) => r.files },
        { key: 'undecided', value: (r) => r.unresolved },
      ],
      format,
    );
  }
  return exportRows(
    hits,
    [
      { key: 'file', value: (h) => h.rel },
      { key: 'line', value: (h) => h.line },
      { key: 'match', value: (h) => h.preview },
      { key: 'enclosing', value: (h) => h.enclosing ?? '' },
      { key: 'undecided', value: (h) => (h.unresolved ? 'yes' : '') },
    ],
    format,
  );
}

/** "12 matches" / "3 rows" — what the export button says it will take out. */
export function exportSubject(report: SsrReport | null, hitCount: number): string {
  if (report?.groupedBy) return `${report.rows.length} row${report.rows.length === 1 ? '' : 's'}`;
  return `${hitCount} match${hitCount === 1 ? '' : 'es'}`;
}

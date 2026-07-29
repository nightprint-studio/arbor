/**
 * Turning a list of records into something you can paste somewhere else.
 *
 * App-agnostic on purpose: it names no Picus concept, so the structural-search
 * results, a query grid and Bennu's future find-usages list can all reach for it.
 * Three formats, and each exists for a different destination:
 *
 *  * **CSV** — a spreadsheet. What these results usually become: somebody wants to
 *    sort four hundred matched rows by one of the captures and hand the list to a
 *    colleague.
 *  * **JSON** — a script. Structured, so nothing has to be re-parsed out of a
 *    rendering.
 *  * **Markdown** — a ticket or a message, where a fenced table reads as a table.
 *
 * Nothing here touches the filesystem or the clipboard. It produces text; where
 * that text goes is the caller's decision, which keeps this testable and keeps the
 * "no automatic writing without confirmation" rule where it belongs.
 */

export type ExportFormat = 'csv' | 'json' | 'markdown';

export interface ExportColumn<T> {
  /** Header, and the JSON key. */
  key: string;
  value: (row: T) => string | number | null | undefined;
}

export const EXPORT_EXTENSION: Record<ExportFormat, string> = {
  csv: 'csv',
  json: 'json',
  markdown: 'md',
};

export function exportRows<T>(
  rows: T[],
  columns: ExportColumn<T>[],
  format: ExportFormat,
): string {
  switch (format) {
    case 'json':
      return toJson(rows, columns);
    case 'markdown':
      return toMarkdown(rows, columns);
    default:
      return toCsv(rows, columns);
  }
}

function cell<T>(row: T, column: ExportColumn<T>): string {
  const value = column.value(row);
  return value === null || value === undefined ? '' : String(value);
}

/**
 * RFC 4180, and the CRLF is not a typo: it is what the standard says and what
 * Excel expects. A field is quoted when it holds a comma, a quote or a line
 * break, and an embedded quote is doubled — the same rule SQL uses for its
 * literals, which is why it is the one nobody in this codebase gets wrong.
 */
function toCsv<T>(rows: T[], columns: ExportColumn<T>[]): string {
  const escape = (text: string): string =>
    /[",\r\n]/.test(text) ? `"${text.replace(/"/g, '""')}"` : text;
  const lines = [columns.map((c) => escape(c.key)).join(',')];
  for (const row of rows) {
    lines.push(columns.map((c) => escape(cell(row, c))).join(','));
  }
  return lines.join('\r\n') + '\r\n';
}

function toJson<T>(rows: T[], columns: ExportColumn<T>[]): string {
  const out = rows.map((row) => {
    const record: Record<string, string> = {};
    for (const column of columns) record[column.key] = cell(row, column);
    return record;
  });
  return JSON.stringify(out, null, 2) + '\n';
}

/**
 * A pipe table. Newlines inside a cell become spaces and pipes are escaped:
 * a Markdown table cannot hold either, and a table that silently breaks in half
 * is worse than one that flattens a value.
 */
function toMarkdown<T>(rows: T[], columns: ExportColumn<T>[]): string {
  const flatten = (text: string): string =>
    text.replace(/\r?\n/g, ' ').replace(/\|/g, '\\|').trim();
  const lines = [
    `| ${columns.map((c) => flatten(c.key)).join(' | ')} |`,
    `| ${columns.map(() => '---').join(' | ')} |`,
  ];
  for (const row of rows) {
    lines.push(`| ${columns.map((c) => flatten(cell(row, c))).join(' | ')} |`);
  }
  return lines.join('\n') + '\n';
}

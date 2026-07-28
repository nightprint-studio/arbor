/**
 * CSV reading for the DML generator.
 *
 * This is **not** SQL: sniffing a delimiter and matching a header to a column is
 * file handling, and it stays on the frontend for good. It used to sit in the
 * emitter stand-in only because that file happened to be where the generator's
 * inputs were written first; the emitter has since moved into `picus-emit` and
 * these came out with it rather than following it into the wrong crate.
 */

import type { Column } from '$lib/types/picus';

export interface CsvParseResult {
  /** Header names as they appear in the file. */
  headers: string[];
  /** One entry per data row, aligned with `headers`. */
  records: string[][];
  delimiter: string;
}

/** Sniff the delimiter from the header line and split the file into records. */
export function parseCsv(text: string): CsvParseResult {
  const lines = text.replace(/\r\n/g, '\n').split('\n').filter((l) => l.trim() !== '');
  if (!lines.length) return { headers: [], records: [], delimiter: ';' };

  const candidates = [';', ',', '\t', '|'];
  const delimiter = candidates
    .map((d) => ({ d, n: lines[0].split(d).length }))
    .sort((a, b) => b.n - a.n)[0].d;

  const headers = lines[0].split(delimiter).map((h) => h.trim());
  const records = lines.slice(1).map((l) => l.split(delimiter).map((c) => c.trim()));
  return { headers, records, delimiter };
}

/** Propose a CSV-header → table-column mapping by case-insensitive name match. */
export function proposeCsvMapping(headers: string[], columns: Column[]): Record<string, string> {
  const map: Record<string, string> = {};
  for (const h of headers) {
    const hit = columns.find((c) => c.name.toUpperCase() === h.toUpperCase());
    if (hit) map[h] = hit.name;
  }
  return map;
}

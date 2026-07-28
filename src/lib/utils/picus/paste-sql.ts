/**
 * Reading pasted INSERT statements back into rows — **a temporary stand-in.**
 *
 * Unlike the CSV reader next door, this one genuinely is SQL parsing, so it does
 * not belong on the frontend at all: it belongs to the `picus-parse` crate, and
 * this module goes away the moment a `picus_parse_inserts` handler exists on
 * `picus-be`. Until then the generator needs *something* to read a paste with, and
 * a regex that is honest about what it cannot read beats no paste source at all.
 *
 * The real implementation parses through Tree-sitter, never regex. Which is also
 * why this one is deliberately strict rather than clever: anything it cannot read
 * is reported, never guessed. A half-understood INSERT silently turned into three
 * files is worse than an error message.
 */

import type { Column, DmlRow } from '$lib/types/picus';

export interface PastedInserts {
  rows: DmlRow[];
  /** Table named by the statements, uppercased; `null` when none was read. */
  table: string | null;
  /** Everything that could not be read, in the user's terms. */
  errors: string[];
}

export function parsePastedInserts(text: string, columns: Column[]): PastedInserts {
  const rows: DmlRow[] = [];
  const errors: string[] = [];
  let table: string | null = null;

  const re = /INSERT\s+INTO\s+([A-Za-z_][\w$]*)\s*\(([^)]*)\)\s*VALUES\s*\(([\s\S]*?)\)\s*;/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    const [, tableName, colList, valueList] = m;
    table ??= tableName.toUpperCase();
    const names = colList.split(',').map((s) => s.trim().toUpperCase());
    const values = splitSqlValues(valueList);
    if (names.length !== values.length) {
      errors.push(`${tableName}: ${names.length} columns but ${values.length} values`);
      continue;
    }
    const row: DmlRow = {};
    names.forEach((n, i) => {
      const known = columns.find((c) => c.name.toUpperCase() === n);
      if (!known) {
        errors.push(`unknown column ${n}`);
        return;
      }
      row[known.name] = unquote(values[i]);
    });
    rows.push(row);
  }

  if (!rows.length && text.trim()) errors.push('no INSERT statement could be read');
  return { rows, table, errors };
}

/** Split a VALUES list on commas that are not inside a quoted literal. */
function splitSqlValues(list: string): string[] {
  const out: string[] = [];
  let current = '';
  let inString = false;
  for (let i = 0; i < list.length; i++) {
    const ch = list[i];
    if (ch === "'") {
      // '' inside a string is an escaped quote, not a terminator.
      if (inString && list[i + 1] === "'") { current += "''"; i++; continue; }
      inString = !inString;
      current += ch;
      continue;
    }
    if (ch === ',' && !inString) { out.push(current.trim()); current = ''; continue; }
    current += ch;
  }
  if (current.trim()) out.push(current.trim());
  return out;
}

/** Strip the surrounding quotes of a SQL literal and unescape doubled quotes. */
function unquote(value: string): string {
  const v = value.trim();
  if (v.length >= 2 && v.startsWith("'") && v.endsWith("'")) {
    return v.slice(1, -1).replace(/''/g, "'");
  }
  return v;
}

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
 *
 * ## The statements describe themselves
 *
 * A pasted `INSERT` names its table and lists its columns. Asking the user to pick
 * the table again — from a dropdown that is empty unless a live database happens
 * to be connected — was asking them to repeat what they had already said, and made
 * the whole source unusable on a machine with no database at hand. Which is most
 * of them: the product maintains *scripts*.
 *
 * So the table and the column set are read out of the text. Types are inferred
 * from the literals, and that inference is not a guess about the schema — it is a
 * record of how the value was **written**: a bare number was written bare and gets
 * re-emitted bare, a quoted value was quoted and gets re-quoted. Round-tripping a
 * statement through the model therefore cannot change what it means, which is the
 * only property that matters here. A live schema, when there is one, still wins:
 * its types are authoritative and carry the length limits validation needs.
 */

import type { Column, DmlRow } from '$lib/types/picus';

export interface PastedInserts {
  rows: DmlRow[];
  /** Table named by the statements, uppercased; `null` when none was read. */
  table: string | null;
  /**
   * The columns the statements name, in the order the first one listed them,
   * with a type inferred from how each value was written. Empty when nothing
   * could be read.
   */
  columns: Column[];
  /** Everything that could not be read, in the user's terms. */
  errors: string[];
}

/**
 * Read pasted INSERTs.
 *
 * `known` is the live table's column set when a database is connected. It is
 * consulted for *types* only — the columns that take part are always the ones the
 * statements actually name, so pasting three of a table's forty columns writes
 * three, not forty.
 */
export function parsePastedInserts(text: string, known: Column[] = []): PastedInserts {
  const rows: DmlRow[] = [];
  const errors: string[] = [];
  const columns: Column[] = [];
  const seen = new Map<string, number>();
  let table: string | null = null;

  const re = /INSERT\s+INTO\s+([A-Za-z_][\w$]*)\s*\(([^)]*)\)\s*VALUES\s*\(([\s\S]*?)\)\s*;/gi;
  let m: RegExpExecArray | null;
  while ((m = re.exec(text)) !== null) {
    const [, tableName, colList, valueList] = m;
    if (table === null) {
      table = tableName.toUpperCase();
    } else if (table !== tableName.toUpperCase()) {
      // One paste, one table. The model has a single `table`, so a second one
      // would silently have its rows written into the first — the kind of
      // mistake that is only found in production.
      errors.push(`${tableName}: a second table in the same paste (already reading ${table})`);
      continue;
    }
    const names = colList.split(',').map((s) => s.trim().replace(/^"|"$/g, '').toUpperCase());
    const values = splitSqlValues(valueList);
    if (names.length !== values.length) {
      errors.push(`${tableName}: ${names.length} columns but ${values.length} values`);
      continue;
    }
    const row: DmlRow = {};
    names.forEach((name, i) => {
      const column = remember(columns, seen, name, values[i], known);
      row[column.name] = readValue(values[i]);
    });
    rows.push(row);
  }

  if (!rows.length && text.trim() && !errors.length) {
    errors.push('no INSERT statement could be read');
  }
  return { rows, table, columns, errors };
}

/**
 * Add a column the first time a statement names it, and answer with it.
 *
 * A later statement that quotes a value the first one wrote bare widens the
 * column to textual — quoting is the safe direction, and a column that is numeric
 * in one row and not in the next is a paste worth emitting conservatively rather
 * than one worth refusing.
 */
function remember(
  columns: Column[],
  seen: Map<string, number>,
  name: string,
  value: string,
  known: Column[],
): Column {
  const at = seen.get(name);
  if (at !== undefined) {
    const existing = columns[at];
    if (!isKnown(existing, known) && existing.type === 'numeric' && !isBareNumber(value)) {
      existing.type = 'text';
    }
    return existing;
  }
  // The live schema's spelling and type win when it has this column: its types
  // carry the length limits and the NOT NULL flags validation reports on.
  const real = known.find((c) => c.name.toUpperCase() === name);
  const column: Column = real
    ? { ...real }
    : { name, type: isBareNumber(value) ? 'numeric' : 'text' };
  seen.set(name, columns.length);
  columns.push(column);
  return column;
}

function isKnown(column: Column, known: Column[]): boolean {
  return known.some((c) => c.name.toUpperCase() === column.name.toUpperCase());
}

/** Written as a bare number — so it must be re-emitted as one. */
function isBareNumber(value: string): boolean {
  return /^-?\d+(\.\d+)?$/.test(value.trim());
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

/**
 * A written SQL value, read into the model's own notation.
 *
 * Three cases, and the third is what keeps the round trip honest:
 *
 *  * a **quoted literal** loses its quotes and its doubling — it is a value;
 *  * a **bare number** stays as it is — also a value, re-emitted bare because the
 *    column's inferred type says it was written bare;
 *  * **anything else is SQL**: `SYSDATE`, `SEQ.nextval`, `(SELECT …)`, another
 *    column. It arrives with the `=` prefix, which is how the model says "this is
 *    an expression, do not quote it".
 *
 * Without that last line a pasted `SYSDATE` would come back as the string
 * `'SYSDATE'` — the statement would still be valid SQL and would install a
 * five-character description where a date was meant, which is the worst kind of
 * wrong. A value that genuinely begins with `=` is escaped as `==`.
 */
function readValue(value: string): string {
  const v = value.trim();
  if (v.length >= 2 && v.startsWith("'") && v.endsWith("'")) {
    const text = v.slice(1, -1).replace(/''/g, "'");
    return text.startsWith('=') ? `=${text}` : text;
  }
  if (/^-?\d+(\.\d+)?$/.test(v)) return v;
  return `=${v}`;
}

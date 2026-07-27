/**
 * Picus SQL emission — TEMPORARY frontend stand-in for the `picus-emit` crate.
 *
 * The product requirement is that generation is **deterministic**: structured
 * input → model → per-dialect emission, no language model anywhere in the flow.
 * This module honours that contract, but in TypeScript and in the wrong process:
 * it exists so the generator UI (live preview, per-target diff) can be built and
 * judged before the backend lands.
 *
 * DELETE THIS FILE when `picus-emit` is wired through `picus-be`. The real
 * emitter owns the golden tests; nothing here should grow a second home.
 *
 * What the dialects actually differ on — the list this module has to encode:
 * block delimiter, upsert syntax, current-date function, object-existence check,
 * transaction handling, identifier casing.
 */

import type {
  Column,
  Dialect,
  DmlOperation,
  DmlRow,
  Target,
  VersionTableConfig,
} from '$lib/types/picus';

/** The dialect-free description of what to write, whatever the source was. */
export interface DmlModel {
  table: string;
  operation: DmlOperation;
  /** Full column set of the table (drives types + ordering). */
  columns: Column[];
  /** Columns forming the comparison key: the WHERE of updates, the existence check. */
  keyColumns: Column[];
  rows: DmlRow[];
  /** Lowercase identifiers on PostgreSQL (a per-project option). */
  lowercasePostgres: boolean;
  /** Where the installed version lives — per project, never a constant. */
  versionTable: VersionTableConfig;
}

// ── Primitives ───────────────────────────────────────────────────────────────

/** Identifier casing is a dialect difference, not a formatting preference. */
function ident(name: string, dialect: Dialect, lowercase: boolean): string {
  return dialect === 'postgres' && lowercase ? name.toLowerCase() : name;
}

function isNumericType(type: string): boolean {
  return /NUMBER|INT|NUMERIC|DECIMAL|FLOAT|DOUBLE|REAL/i.test(type);
}

/** Values the user means as expressions, not as string literals. */
const EXPRESSION_RE = /^(SYSDATE|CURRENT_TIMESTAMP|CURRENT_DATE|NOW\(\)|NULL)$/i;

export function looksLikeExpression(value: string): boolean {
  return EXPRESSION_RE.test(value.trim());
}

/** The per-dialect "now" function. */
export function nowFunction(dialect: Dialect): string {
  return dialect === 'oracle' ? 'SYSDATE' : 'CURRENT_TIMESTAMP';
}

/**
 * Render one value as SQL. Empty → NULL; a recognised expression passes through
 * (translated per dialect); numbers stay bare when the column is numeric;
 * everything else is a quoted literal with doubled quotes.
 */
export function literal(value: string | undefined, column: Column, dialect: Dialect): string {
  const raw = String(value ?? '').trim();
  if (raw === '') return 'NULL';
  if (looksLikeExpression(raw)) {
    const upper = raw.toUpperCase();
    if (upper === 'NULL') return 'NULL';
    if (upper === 'SYSDATE' || upper === 'NOW()' || upper === 'CURRENT_TIMESTAMP') {
      return nowFunction(dialect);
    }
    return upper;
  }
  if (isNumericType(column.type) && /^-?\d+(\.\d+)?$/.test(raw)) return raw;
  return `'${raw.replace(/'/g, "''")}'`;
}

/** Does this value pass its column's type check? Drives the live validation. */
export function validateValue(value: string, column: Column): string | null {
  const raw = value.trim();
  if (raw === '') {
    return column.notNull && !column.primaryKey ? 'required (NOT NULL)' : null;
  }
  if (looksLikeExpression(raw)) return null;
  if (isNumericType(column.type) && !/^-?\d+(\.\d+)?$/.test(raw)) {
    return `not a number (${column.type})`;
  }
  const sized = /\((\d+)(?:,\s*\d+)?\)/.exec(column.type);
  if (sized && /CHAR|VARCHAR/i.test(column.type) && raw.length > Number(sized[1])) {
    return `longer than ${sized[1]} characters`;
  }
  return null;
}

/** Columns actually supplied in a row — an omitted column is left out entirely. */
function suppliedColumns(model: DmlModel, row: DmlRow): Column[] {
  return model.columns.filter((c) => String(row[c.name] ?? '').trim() !== '');
}

// ── Bare statements ──────────────────────────────────────────────────────────

/**
 * One statement with no procedural wrapper. `upsert` becomes the dialect's
 * native merge form: `MERGE … USING (… FROM DUAL)` on Oracle,
 * `INSERT … ON CONFLICT … DO UPDATE` on PostgreSQL.
 */
export function plainStatement(model: DmlModel, row: DmlRow, dialect: Dialect): string {
  const lc = model.lowercasePostgres;
  const table = ident(model.table, dialect, lc);
  const cols = suppliedColumns(model, row);
  const keys = model.keyColumns;
  const nonKey = cols.filter((c) => !keys.some((k) => k.name === c.name));
  const id = (c: Column) => ident(c.name, dialect, lc);
  const val = (c: Column) => literal(row[c.name], c, dialect);

  switch (model.operation) {
    case 'insert':
      return `INSERT INTO ${table} (${cols.map(id).join(', ')})\nVALUES (${cols.map(val).join(', ')});`;

    case 'update':
      return (
        `UPDATE ${table} SET ${nonKey.map((c) => `${id(c)} = ${val(c)}`).join(', ')}\n` +
        ` WHERE ${keys.map((c) => `${id(c)} = ${val(c)}`).join(' AND ')};`
      );

    case 'delete':
      return `DELETE FROM ${table}\n WHERE ${keys.map((c) => `${id(c)} = ${val(c)}`).join(' AND ')};`;

    case 'upsert':
      if (dialect === 'postgres') {
        return (
          `INSERT INTO ${table} (${cols.map(id).join(', ')})\n` +
          `VALUES (${cols.map(val).join(', ')})\n` +
          `ON CONFLICT (${keys.map(id).join(', ')}) DO UPDATE\n` +
          `   SET ${nonKey.map((c) => `${id(c)} = EXCLUDED.${id(c)}`).join(', ')};`
        );
      }
      return (
        `MERGE INTO ${table} d\n` +
        `USING (SELECT ${keys.map((c) => `${val(c)} AS ${id(c)}`).join(', ')} FROM DUAL) s\n` +
        `   ON (${keys.map((c) => `d.${id(c)} = s.${id(c)}`).join(' AND ')})\n` +
        `WHEN MATCHED THEN UPDATE SET ${nonKey.map((c) => `d.${id(c)} = ${val(c)}`).join(', ')}\n` +
        `WHEN NOT MATCHED THEN INSERT (${cols.map(id).join(', ')}) VALUES (${cols.map(val).join(', ')});`
      );
  }
}

// ── Procedural blocks ────────────────────────────────────────────────────────

const INDENT = '    ';

function indent(text: string): string {
  return text
    .split('\n')
    .map((l) => INDENT + l)
    .join('\n');
}

/** `WHERE …` for version tables holding one row per module; empty otherwise. */
function versionFilter(v: VersionTableConfig): string {
  return v.filter.trim() ? `\n   WHERE ${v.filter.trim()}` : '';
}

function oracleBlock(model: DmlModel, target: Target): string {
  const g = target.guards;
  const keys = model.keyColumns;
  const v = model.versionTable;
  let out = 'DECLARE\n';
  if (g.version) out += '  v_versione VARCHAR2(30);\n';
  if (g.skipIfPresent) out += '  v_presenti NUMBER;\n';
  if (g.requireObject) out += '  v_oggetto  NUMBER;\n';
  out += 'BEGIN\n';

  if (g.version) {
    out +=
      `  -- guard: only applies when starting from ${g.version.from}\n` +
      `  SELECT ${v.versionColumn} INTO v_versione FROM ${v.table}${versionFilter(v)};\n` +
      `  IF v_versione <> '${g.version.from}' THEN\n    RETURN;\n  END IF;\n\n`;
  }
  if (g.requireObject) {
    out +=
      `  SELECT COUNT(*) INTO v_oggetto FROM USER_TABLES WHERE TABLE_NAME = '${model.table}';\n` +
      `  IF v_oggetto = 0 THEN\n    RETURN;\n  END IF;\n\n`;
  }
  if (g.transactional) out += '  SAVEPOINT prima_delle_modifiche;\n\n';

  model.rows.forEach((row, i) => {
    const body = indent(plainStatement(model, row, 'oracle'));
    if (g.skipIfPresent && model.operation !== 'delete') {
      out +=
        `  SELECT COUNT(*) INTO v_presenti FROM ${model.table}\n` +
        `   WHERE ${keys.map((c) => `${c.name} = ${literal(row[c.name], c, 'oracle')}`).join(' AND ')};\n` +
        `  IF v_presenti = 0 THEN\n${body}\n  END IF;\n`;
    } else {
      out += `${body}\n`;
    }
    if (i < model.rows.length - 1) out += '\n';
  });

  if (g.version) {
    // The date column is stamped ONLY when the project has one — plenty of
    // version tables hold nothing but the version string, and inventing a
    // column would emit an UPDATE that fails on the first run.
    const sets = [`${v.versionColumn} = '${g.version.to}'`];
    if (v.dateColumn) sets.push(`${v.dateColumn} = SYSDATE`);
    out +=
      `\n  -- carry the database to ${g.version.to}\n` +
      `  UPDATE ${v.table} SET ${sets.join(', ')}${versionFilter(v)};\n`;
  }
  out += '  COMMIT;\n';
  if (g.transactional) {
    out += 'EXCEPTION\n  WHEN OTHERS THEN\n    ROLLBACK TO prima_delle_modifiche;\n    RAISE;\n';
  }
  out += 'END;\n/';
  return out;
}

function postgresBlock(model: DmlModel, target: Target): string {
  const g = target.guards;
  const lc = model.lowercasePostgres;
  const table = ident(model.table, 'postgres', lc);
  const keys = model.keyColumns;
  const v = model.versionTable;
  const vTable = ident(v.table, 'postgres', lc);
  const vColumn = ident(v.versionColumn, 'postgres', lc);

  let out = 'DO $$\n';
  if (g.version || g.skipIfPresent) {
    out += 'DECLARE\n';
    if (g.version) out += '  v_versione text;\n';
    if (g.skipIfPresent) out += '  v_presenti int;\n';
  }
  out += 'BEGIN\n';

  if (g.version) {
    out +=
      `  -- guard: only applies when starting from ${g.version.from}\n` +
      `  SELECT ${vColumn} INTO v_versione FROM ${vTable}${versionFilter(v)};\n` +
      `  IF v_versione <> '${g.version.from}' THEN\n    RETURN;\n  END IF;\n\n`;
  }
  if (g.requireObject) {
    out += `  IF to_regclass('${table}') IS NULL THEN\n    RETURN;\n  END IF;\n\n`;
  }

  model.rows.forEach((row, i) => {
    const body = indent(plainStatement(model, row, 'postgres'));
    if (g.skipIfPresent && model.operation !== 'delete') {
      out +=
        `  SELECT count(*) INTO v_presenti FROM ${table}\n` +
        `   WHERE ${keys.map((c) => `${ident(c.name, 'postgres', lc)} = ${literal(row[c.name], c, 'postgres')}`).join(' AND ')};\n` +
        `  IF v_presenti = 0 THEN\n${body}\n  END IF;\n`;
    } else {
      out += `${body}\n`;
    }
    if (i < model.rows.length - 1) out += '\n';
  });

  if (g.version) {
    const sets = [`${vColumn} = '${g.version.to}'`];
    if (v.dateColumn) sets.push(`${ident(v.dateColumn, 'postgres', lc)} = CURRENT_TIMESTAMP`);
    out +=
      `\n  -- carry the database to ${g.version.to}\n` +
      `  UPDATE ${vTable} SET ${sets.join(', ')}${versionFilter(v)};\n`;
  }
  out += 'END $$;';
  return out;
}

// ── Entry point ──────────────────────────────────────────────────────────────

/** The SQL one target receives, header comment included. */
export function emitForTarget(model: DmlModel, target: Target): string {
  if (!model.rows.length) {
    return '-- no rows yet: fill in the form, paste some INSERTs, or import a CSV';
  }
  const header = `-- ${model.table} · ${target.dialect === 'oracle' ? 'Oracle' : 'PostgreSQL'} · ${target.role}\n`;
  if (target.wrap === 'plain') {
    return header + model.rows.map((r) => plainStatement(model, r, target.dialect)).join('\n\n');
  }
  return header + (target.dialect === 'oracle' ? oracleBlock(model, target) : postgresBlock(model, target));
}

// ── Source readers (paste / CSV) ─────────────────────────────────────────────

/**
 * Read pasted INSERT statements back into rows.
 *
 * The real implementation parses through `picus-parse` (Tree-sitter), never
 * regex — this stand-in is regex precisely because it is a stand-in, and it is
 * deliberately strict: anything it can't read is reported, not guessed.
 */
export function parsePastedInserts(
  text: string,
  columns: Column[],
): { rows: DmlRow[]; table: string | null; errors: string[] } {
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

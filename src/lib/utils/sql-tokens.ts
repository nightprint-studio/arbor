/**
 * SQL in running text — finding it inside a log line, and cutting it into the pieces a renderer colours.
 *
 * Two consumers with different surfaces and one need: the Run console (a Hibernate `show_sql` line, a
 * MyBatis `Preparing:`) and the editor chrome that previews code outside the main text flow. Both want
 * "which part of this text is SQL, and what are its keywords / strings / numbers / placeholders" —
 * answered once here, rendered by each in its own way.
 *
 * Deliberately conservative about what counts as SQL: a log line is mostly English, and "Select a file
 * from the list" must not light up. A statement is recognised by its SHAPE (a column list or a
 * placeholder between `select` and `from`, `insert into … values`, `update … set x =`), not by one keyword.
 */

export type SqlTokenKind = 'keyword' | 'string' | 'number' | 'comment' | 'param' | 'text';

export interface SqlToken {
  text: string;
  kind: SqlTokenKind;
}

const SQL_KEYWORDS = new Set([
  'select', 'from', 'where', 'and', 'or', 'not', 'in', 'is', 'null', 'as', 'on', 'join', 'inner', 'left', 'right',
  'outer', 'full', 'cross', 'group', 'by', 'order', 'having', 'limit', 'offset', 'union', 'all', 'distinct',
  'insert', 'into', 'values', 'update', 'set', 'delete', 'with', 'case', 'when', 'then', 'else', 'end', 'like',
  'ilike', 'between', 'exists', 'asc', 'desc', 'count', 'sum', 'min', 'max', 'avg', 'coalesce', 'over',
  'partition', 'using', 'returning', 'fetch', 'first', 'rows', 'only', 'merge', 'matched', 'create', 'table',
]);

const TOKEN = /(--[^\n]*)|('(?:''|[^'])*')|(\b\d[\w.]*)|([A-Za-z_][\w$]*)|(:[A-Za-z_]\w*|\?\d*)/g;

/** SQL text cut into coloured pieces; the pieces concatenate back to `sql`. */
export function sqlTokens(sql: string): SqlToken[] {
  const out: SqlToken[] = [];
  let last = 0;
  TOKEN.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = TOKEN.exec(sql)) !== null) {
    if (m.index > last) out.push({ text: sql.slice(last, m.index), kind: 'text' });
    const [full, comment, str, num, word, param] = m;
    const kind: SqlTokenKind =
      comment !== undefined ? 'comment'
      : str !== undefined ? 'string'
      : num !== undefined ? 'number'
      : param !== undefined ? 'param'
      : word !== undefined && SQL_KEYWORDS.has(word.toLowerCase()) ? 'keyword'
      : 'text';
    out.push({ text: full, kind });
    last = m.index + full.length;
  }
  if (last < sql.length) out.push({ text: sql.slice(last), kind: 'text' });
  return out;
}

/** A statement shape at the start of `rest`, not just a keyword in a sentence. */
function startsStatement(rest: string): boolean {
  if (/^select\s+(?:distinct\s+)?[^;]*?\bfrom\s+\S/i.test(rest)) {
    const columns = /^select\s+(?:distinct\s+)?([^;]*?)\s+from\s/i.exec(rest)?.[1] ?? '';
    // "select a file from the list": no column punctuation, no placeholder — English, not SQL.
    return /[*,.(]|\bas\b/i.test(columns) || /\?|:\w/.test(rest) || /^\w+$/.test(columns) && /\bwhere\b|\bjoin\b/i.test(rest);
  }
  return (
    /^insert\s+into\s+\S+\s*(?:\(|values\b|select\b)/i.test(rest)
    || /^update\s+\S+\s+set\s+\S+\s*=/i.test(rest)
    || /^delete\s+from\s+\S+/i.test(rest)
    || /^merge\s+into\s+\S+/i.test(rest)
    || /^with\s+\w+\s+as\s*\(/i.test(rest)
  );
}

const START = /\b(?:select|insert\s+into|update|delete\s+from|merge\s+into|with)\b/gi;

/**
 * The SQL inside a line of text, as a `[start, end)` range of `line`, or `null`. It runs to the end of the
 * line (a log prints a statement last), less the `]` or quote a wrapper such as `Executing SQL query [...]`
 * closed it with.
 */
export function findSql(line: string): { start: number; end: number } | null {
  START.lastIndex = 0;
  let m: RegExpExecArray | null;
  while ((m = START.exec(line)) !== null) {
    if (!startsStatement(line.slice(m.index))) continue;
    let end = line.length;
    const opener = line[m.index - 1];
    if (opener === '[' || opener === '"' || opener === "'") {
      const close = line.lastIndexOf(opener === '[' ? ']' : opener);
      if (close > m.index) end = close;
    }
    return { start: m.index, end };
  }
  return null;
}

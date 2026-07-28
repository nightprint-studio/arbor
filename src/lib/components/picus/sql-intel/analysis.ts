/**
 * What one statement is talking about: which relations it names, what they were
 * aliased to, and what the caret is in the middle of.
 *
 * This is the layer alias resolution lives in, and it is worth being precise about
 * what it can and cannot do. It walks the token stream of **one statement** and
 * reads a table reference wherever SQL syntactically introduces one — after
 * `FROM`, after each `JOIN`, after `INSERT INTO`, after `UPDATE`, after
 * `MERGE INTO` / `MERGE … USING`. Each reference carries the name as written, the
 * schema qualifier if any, and the alias if one follows.
 *
 * Where it gives up — and says so, by marking the reference **opaque** rather than
 * guessing:
 *
 *  • a derived table — `FROM (SELECT …) d` — the columns of `d` are whatever that
 *    SELECT produced, which needs a real parse;
 *  • a CTE reference, for the same reason;
 *  • a table function — `FROM TABLE(f(x))`, `FROM generate_series(…) g`.
 *
 * An opaque relation contributes its alias (so `d.` is recognised as a qualifier
 * and produces no wrong candidates) but never a column list, and is never reported
 * as an unknown table. That is the whole trick: the scanner is allowed not to
 * know, as long as not-knowing is represented and never mistaken for knowing.
 *
 * Relations are collected at **paren depth 0 only**, so a subquery buried in a
 * `WHERE … IN (SELECT … FROM ALTRA)` does not leak `ALTRA` into the outer scope.
 */

import type { Dialect } from '$lib/types/picus';
import { ALIAS_STOP, BLOCK_STARTERS, WRITE_STARTERS } from './keywords';
import type { SqlStatement, SqlToken } from './tokens';

export type StatementKind = 'select' | 'insert' | 'update' | 'delete' | 'merge' | 'ddl' | 'other';

/** One table reference found in a statement. */
export interface RelationRef {
  /** Name as written, without the schema qualifier. */
  name: string;
  /** Schema qualifier as written, `''` when unqualified. */
  schema: string;
  /** Alias, `''` when the reference has none. */
  alias: string;
  /** Span of the name itself — where an "unknown table" marker goes. */
  from: number;
  to: number;
  /** Columns unknown: a derived table, a CTE reference or a table function. */
  opaque: boolean;
  /** A write target (`INSERT INTO t`) rather than a row source. It is not in scope
   *  for the `SELECT` half of an `INSERT … SELECT`, and treating it as if it were
   *  is exactly how a false "ambiguous column" is produced. */
  target: boolean;
}

export interface StatementInfo {
  kind: StatementKind;
  leading: SqlToken | null;
  isWrite: boolean;
  /** The statement is a block or a fragment of one — schema checks stand down. */
  procedural: boolean;
  relations: RelationRef[];
  /** CTE names defined by this statement's `WITH` — never "unknown tables". */
  cteNames: string[];
  /** Words introduced by `AS x` — output aliases, never column references. */
  declaredAliases: Set<string>;
  /** A `SELECT` nested in parentheses: scope is wider than we can see, so the
   *  unqualified-column analysis stands down for the whole statement. */
  hasSubquery: boolean;
  /** Token-index ranges of `WITH` bodies, analysed as statements of their own. */
  cteBodies: Array<{ start: number; end: number }>;
  /** Token indices sitting inside a `USING (…)` join column list — columns that are
   *  common to both sides **by construction**, so never ambiguous. */
  usingColumns: Set<number>;
  /** Token indices inside an `INSERT INTO t (…)` column list — unqualified by
   *  design and scoped to the target, so never ambiguous either. */
  insertListColumns: Set<number>;
}

/** The comparable identifier a token denotes: upper-cased for a bare word (SQL
 *  folds case), verbatim for a `"delimited"` one. */
export function identOf(t: SqlToken | undefined): string {
  if (!t) return '';
  return t.kind === 'word' || t.kind === 'quoted' ? t.value : '';
}

function isName(t: SqlToken | undefined): boolean {
  return !!t && (t.kind === 'word' || t.kind === 'quoted');
}

function isPunct(t: SqlToken | undefined, text: string): boolean {
  return !!t && t.kind === 'punct' && t.text === text;
}

/** Index just past the `)` matching the `(` at `open`. */
function skipParens(t: SqlToken[], open: number): number {
  let depth = 0;
  for (let i = open; i < t.length; i++) {
    if (isPunct(t[i], '(')) depth += 1;
    else if (isPunct(t[i], ')')) {
      depth -= 1;
      if (depth === 0) return i + 1;
    }
  }
  return t.length;
}

// ── Reading one table reference ───────────────────────────────────────────────

/** Read a table reference starting at `i`, append it to `out`, return the next index. */
function readRelation(
  t: SqlToken[], i: number, out: RelationRef[], opts: { target: boolean; columnListFollows: boolean },
): number {
  // Noise words that may precede the name without being it.
  while (t[i] && t[i].kind === 'word' && (t[i].value === 'ONLY' || t[i].value === 'LATERAL')) i += 1;

  let ref: RelationRef;

  if (isPunct(t[i], '(')) {
    // A derived table. Its columns are unknowable without a real parse — record the
    // alias so `d.` resolves to *something* known-unknown, and nothing else.
    const after = skipParens(t, i);
    ref = { name: '', schema: '', alias: '', from: t[i].from, to: t[i].to, opaque: true, target: opts.target };
    i = after;
  } else if (isName(t[i])) {
    let schema = '';
    let name = identOf(t[i]);
    const from = t[i].from;
    let to = t[i].to;
    i += 1;
    // `schema.table`, and `db.schema.table` — only the last two parts mean anything
    // to a catalogue read from one connection.
    while (isPunct(t[i], '.') && isName(t[i + 1])) {
      schema = name;
      name = identOf(t[i + 1]);
      to = t[i + 1].to;
      i += 2;
    }
    // `FROM generate_series(1, 10) g` — a set-returning function, not a table.
    // An `INSERT INTO t (…)` is the one place a following `(` is a column list.
    let opaque = false;
    if (!opts.columnListFollows && isPunct(t[i], '(')) {
      opaque = true;
      i = skipParens(t, i);
    }
    ref = { name, schema, alias: '', from, to, opaque, target: opts.target };
  } else {
    return i + 1;
  }

  // Optional alias: `AS x`, or a bare word that is not the next clause keyword.
  if (t[i] && t[i].kind === 'word' && t[i].value === 'AS') i += 1;
  if (isName(t[i]) && !(t[i].kind === 'word' && ALIAS_STOP.has(t[i].value)) && !isPunct(t[i + 1], '.')) {
    ref.alias = identOf(t[i]);
    i += 1;
    // A CTE-style column list on an alias — `… ) d (a, b)`. Skipped, not read: the
    // names are the derived table's, not a base table's.
    if (ref.opaque && isPunct(t[i], '(')) i = skipParens(t, i);
  }

  out.push(ref);
  return i;
}

/** Read a comma-separated list of table references (`FROM a, b c, d`). */
function readRelationList(t: SqlToken[], i: number, out: RelationRef[], target: boolean): number {
  for (;;) {
    const next = readRelation(t, i, out, { target, columnListFollows: false });
    if (next === i) return i + 1;
    i = next;
    if (!isPunct(t[i], ',')) return i;
    i += 1;
  }
}

// ── WITH ──────────────────────────────────────────────────────────────────────

/** Parse `WITH a AS (…), b AS (…)`, collecting the names and the body ranges. */
function readCtes(
  t: SqlToken[], i: number, names: string[], bodies: Array<{ start: number; end: number }>,
): number {
  i += 1; // past WITH
  if (t[i] && t[i].kind === 'word' && t[i].value === 'RECURSIVE') i += 1;
  for (;;) {
    if (!isName(t[i])) return i;
    names.push(identOf(t[i]));
    i += 1;
    if (isPunct(t[i], '(')) i = skipParens(t, i);          // explicit column list
    if (t[i] && t[i].kind === 'word' && t[i].value === 'AS') i += 1;
    while (t[i] && t[i].kind === 'word' && (t[i].value === 'NOT' || t[i].value === 'MATERIALIZED')) i += 1;
    if (isPunct(t[i], '(')) {
      const end = skipParens(t, i);
      bodies.push({ start: i + 1, end: end - 1 });
      i = end;
    }
    if (!isPunct(t[i], ',')) return i;
    i += 1;
  }
}

// ── The walk ──────────────────────────────────────────────────────────────────

function kindOf(t: SqlToken[]): StatementKind {
  const first = t[0]?.kind === 'word' ? t[0].value : '';
  const head = first === 'WITH' ? headAfterWith(t) : first;
  switch (head) {
    case 'SELECT': return 'select';
    case 'INSERT': return 'insert';
    case 'UPDATE': return 'update';
    case 'DELETE': return 'delete';
    case 'MERGE': return 'merge';
    case 'CREATE': case 'ALTER': case 'DROP': case 'TRUNCATE': case 'COMMENT': case 'GRANT':
    case 'REVOKE': case 'RENAME':
      return 'ddl';
    default: return 'other';
  }
}

/** The verb a `WITH …` statement actually runs — the first top-level word after the
 *  CTE list, which is the only thing that says whether it is a read or a write. */
function headAfterWith(t: SqlToken[]): string {
  let depth = 0;
  for (let i = 1; i < t.length; i++) {
    if (isPunct(t[i], '(')) depth += 1;
    else if (isPunct(t[i], ')')) depth -= 1;
    else if (depth === 0 && t[i].kind === 'word'
      && ['SELECT', 'INSERT', 'UPDATE', 'DELETE', 'MERGE'].includes(t[i].value)) return t[i].value;
  }
  return 'SELECT';
}

/** Analyse one statement. Pure and cheap — a linear pass over its tokens. */
export function analyzeStatement(stmt: SqlStatement, _dialect: Dialect): StatementInfo {
  const t = stmt.tokens;
  const relations: RelationRef[] = [];
  const cteNames: string[] = [];
  const cteBodies: Array<{ start: number; end: number }> = [];
  const declaredAliases = new Set<string>();
  const usingColumns = new Set<number>();
  const insertListColumns = new Set<number>();
  let hasSubquery = false;
  let depth = 0;

  const leading = t[0] ?? null;
  const leadValue = leading && leading.kind === 'word' ? leading.value : '';
  const kind = kindOf(t);
  const isWrite = WRITE_STARTERS.has(leadValue)
    || (leadValue === 'WITH' && ['insert', 'update', 'delete', 'merge'].includes(kind));
  // A routine definition is a block too — its body is procedural even though its
  // first word is `CREATE`.
  const routine = leadValue === 'CREATE'
    && t.slice(0, 8).some((x) => x.kind === 'word'
      && ['PROCEDURE', 'FUNCTION', 'PACKAGE', 'TRIGGER', 'TYPE'].includes(x.value));
  const procedural = BLOCK_STARTERS.has(leadValue) || routine;

  let i = 0;
  if (leadValue === 'WITH') i = readCtes(t, 0, cteNames, cteBodies);

  while (i < t.length) {
    const tok = t[i];

    if (tok.kind === 'punct') {
      if (tok.text === '(') depth += 1;
      else if (tok.text === ')') depth = Math.max(0, depth - 1);
      i += 1;
      continue;
    }

    if (tok.kind !== 'word') { i += 1; continue; }

    if (tok.value === 'SELECT' && depth > 0) hasSubquery = true;
    if (tok.value === 'AS' && isName(t[i + 1])) declaredAliases.add(identOf(t[i + 1]));

    // `JOIN b USING (ID)` — those columns exist on both sides on purpose.
    if (tok.value === 'USING' && isPunct(t[i + 1], '(')) {
      const end = skipParens(t, i + 1);
      for (let k = i + 2; k < end - 1; k++) usingColumns.add(k);
      i = end;
      continue;
    }

    if (depth === 0) {
      const prev = i > 0 && t[i - 1].kind === 'word' ? t[i - 1].value : '';
      if (tok.value === 'FROM') { i = readRelationList(t, i + 1, relations, false); continue; }
      if (tok.value === 'JOIN') { i = readRelation(t, i + 1, relations, { target: false, columnListFollows: false }); continue; }
      if (tok.value === 'INTO' && (prev === 'INSERT' || prev === 'MERGE')) {
        const before = relations.length;
        i = readRelation(t, i + 1, relations, { target: true, columnListFollows: true });
        // The parenthesised column list of an INSERT: unqualified by design.
        if (relations.length > before && isPunct(t[i], '(')) {
          const end = skipParens(t, i);
          for (let k = i + 1; k < end - 1; k++) insertListColumns.add(k);
          i = end;
        }
        continue;
      }
      if (tok.value === 'UPDATE' && prev !== 'FOR' && prev !== 'DO') {
        i = readRelationList(t, i + 1, relations, false);
        continue;
      }
      if (tok.value === 'USING' && kind === 'merge') {
        i = readRelation(t, i + 1, relations, { target: false, columnListFollows: false });
        continue;
      }
    }

    i += 1;
  }

  return {
    kind, leading, isWrite, procedural, relations, cteNames, declaredAliases,
    hasSubquery, cteBodies, usingColumns, insertListColumns,
  };
}

// ── Where the caret is ────────────────────────────────────────────────────────

export type Clause =
  | 'none' | 'select' | 'from' | 'on' | 'where' | 'set' | 'values'
  | 'insert-cols' | 'using-cols' | 'group' | 'order' | 'having' | 'returning';

export interface CaretContext {
  info: StatementInfo;
  clause: Clause;
  /** The qualifier the caret is completing against — `c` in `c.no|`. */
  qualifier: string;
  /** Index of the last token that ends at or before the caret, `-1` when none. */
  lastIndex: number;
  /** Names already present in an `INSERT INTO t (…)` list the caret is inside. */
  insertListSoFar: string[] | null;
}

/** Index of the `(` that opens an `INSERT INTO t (…)` column list, or `-1`. */
function insertColumnListParen(t: SqlToken[], into: number): number {
  if (t[into - 1]?.value !== 'INSERT') return -1;
  let i = into + 1;
  if (!isName(t[i])) return -1;
  i += 1;
  while (isPunct(t[i], '.') && isName(t[i + 1])) i += 2;
  return isPunct(t[i], '(') ? i : -1;
}

/** Which clause `offset` falls in — a small state machine over the tokens before it. */
function clauseAt(t: SqlToken[], offset: number): { clause: Clause; insertList: string[] | null } {
  let clause: Clause = 'none';
  let depth = 0;
  /** Paren depth a column-list clause opened at, so its `)` is what closes it. */
  let listDepth = -1;
  let insertList: string[] | null = null;
  /** Index of the `(` that will open the pending INSERT column list. */
  let pendingInsertParen = -1;

  for (let i = 0; i < t.length; i++) {
    const tok = t[i];
    if (tok.from >= offset) break;

    if (tok.kind === 'punct') {
      if (tok.text === '(') {
        depth += 1;
        if (i === pendingInsertParen) { clause = 'insert-cols'; listDepth = depth; insertList = []; }
        else if (t[i - 1]?.value === 'USING') { clause = 'using-cols'; listDepth = depth; }
      } else if (tok.text === ')') {
        depth -= 1;
        if (listDepth >= 0 && depth < listDepth) { clause = 'none'; listDepth = -1; insertList = null; }
      }
      continue;
    }
    if (tok.kind !== 'word') continue;
    // Inside a column list every word is one of the target's columns.
    if (clause === 'insert-cols') { insertList?.push(tok.value); continue; }

    switch (tok.value) {
      case 'SELECT': clause = 'select'; break;
      case 'FROM': case 'JOIN': clause = 'from'; break;
      case 'ON': clause = 'on'; break;
      case 'WHERE': clause = 'where'; break;
      case 'SET': clause = 'set'; break;
      case 'VALUES': clause = 'values'; break;
      case 'RETURNING': clause = 'returning'; break;
      case 'HAVING': clause = 'having'; break;
      case 'GROUP': clause = 'group'; break;
      case 'ORDER': clause = 'order'; break;
      case 'INTO': pendingInsertParen = insertColumnListParen(t, i); break;
      default: break;
    }
  }
  return { clause, insertList };
}

/** The qualifier the caret is completing against: `c` for `c.` and for `c.NO|`. */
function qualifierAt(t: SqlToken[], offset: number): string {
  let k = -1;
  for (let i = 0; i < t.length; i++) {
    if (t[i].to <= offset) k = i;
    else break;
  }
  if (k < 0) return '';
  if (isPunct(t[k], '.')) return identOf(t[k - 1]);
  if (isName(t[k]) && t[k].to === offset && isPunct(t[k - 1], '.')) return identOf(t[k - 2]);
  return '';
}

/** Everything the caret's position tells us, for one already-analysed statement. */
export function caretContext(stmt: SqlStatement, info: StatementInfo, offset: number): CaretContext {
  const { clause, insertList } = clauseAt(stmt.tokens, offset);
  let lastIndex = -1;
  for (let i = 0; i < stmt.tokens.length; i++) {
    if (stmt.tokens[i].to <= offset) lastIndex = i;
    else break;
  }
  return {
    info,
    clause,
    qualifier: qualifierAt(stmt.tokens, offset),
    lastIndex,
    insertListSoFar: insertList,
  };
}

/** Resolve a qualifier — an alias, or a table named in full — to its reference. */
export function resolveQualifier(info: StatementInfo, qualifier: string): RelationRef | null {
  if (!qualifier) return null;
  const q = qualifier.toUpperCase();
  return info.relations.find((r) => r.alias.toUpperCase() === q)
    ?? info.relations.find((r) => r.name.toUpperCase() === q)
    ?? null;
}

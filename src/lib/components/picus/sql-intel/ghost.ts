/**
 * Ghost text for SQL — the continuations that are **facts**, not predictions.
 *
 * This is the feature where the "no language model anywhere" constraint stops
 * being a restriction and becomes the design. After `INSERT INTO PARAMETRI (` the
 * column list is not something to guess at: the connection already told us what
 * `PARAMETRI` has, in what order. After `JOIN ORDINI o ON ` the predicate is
 * written in the foreign key. The closing `END;` of a block you opened is
 * arithmetic. Each of those is either exactly right or absent, and absent is
 * always the fallback.
 *
 * ## The rules that are implemented
 *
 * | Trigger | Proposal | Where it comes from |
 * |---|---|---|
 * | `INSERT INTO t ` | `(COL1, COL2, …)` | the catalogue |
 * | `INSERT INTO t (` | `COL1, COL2, …)` | the catalogue |
 * | `INSERT INTO t (A, B)` | `VALUES (…);` with matching arity | the list just typed |
 * | `JOIN t x ON ` | `x.COL = y.REF` | the foreign key between the two |
 * | a blank line inside an open block | `END;` / `END IF;` / `END LOOP;` / `END CASE;` | counting |
 *
 * ## The rules that were considered and rejected
 *
 * - **The column list after `SELECT `** — which columns you want is a choice, not
 *   a fact. `*` would be a guess dressed as help.
 * - **A `WHERE` predicate on the primary key after `DELETE FROM t `** — plausible,
 *   and plausible is exactly what this feature must not be.
 * - **Every column after `UPDATE t SET `** — nobody updates every column; the
 *   completion popup is the right tool for choosing.
 * - **`END` for an expression `CASE` in the middle of a line** — indistinguishable
 *   from a statement `CASE` without a real parse, and the guard that keeps
 *   proposals to blank lines makes it moot.
 * - **`$$ LANGUAGE plpgsql;` after a function body** — the language is not derivable
 *   from the text. A `DO $$` block does end `$$;`, so that one *is* offered.
 */

import type { EditorView } from '@codemirror/view';
import type { Dialect, TableInfo } from '$lib/types/picus';
import type { InlineCompletionSource } from '$lib/components/shared/ui/code-editor';
import { analyzeStatement, caretContext, type RelationRef, type StatementInfo } from './analysis';
import { ensureRelationDetail, schemaViewFor, type SchemaView } from './schema-view';
import { inLiteral, scanSql, statementAt, tokenize, type SqlToken, type TokenScan } from './tokens';

/** Everything on the caret's line, split at the caret. */
function lineAround(src: string, pos: number): { before: string; after: string } {
  let start = pos;
  while (start > 0 && src[start - 1] !== '\n') start -= 1;
  let end = pos;
  while (end < src.length && src[end] !== '\n') end += 1;
  return { before: src.slice(start, pos), after: src.slice(pos, end) };
}

/**
 * Nothing meaningful between the caret and the end of the line — the guard every
 * rule shares, so a proposal never lands in the middle of typed text.
 *
 * A lone `)` counts as nothing, because bracket auto-closing puts one there the
 * instant you type `INSERT INTO t (` — which is precisely the position the most
 * useful proposal fires at. Refusing to look past it would disable the rule for
 * everyone who has auto-close on, i.e. everyone.
 */
function atEndOfLine(src: string, pos: number): boolean {
  const rest = lineAround(src, pos).after.trim();
  return rest === '' || rest === ')' || rest === ');';
}

/** The next non-whitespace character after `pos`. */
function nextChar(src: string, pos: number): string {
  let i = pos;
  while (i < src.length && /\s/.test(src[i])) i += 1;
  return src[i] ?? '';
}

/** The relation an INSERT / MERGE writes into. */
function writeTarget(info: StatementInfo): RelationRef | null {
  return info.relations.find((r) => r.target) ?? null;
}

/** `:NOME, :VALORE` on Oracle, `$1, $2` on PostgreSQL.
 *
 *  The arity and the order are facts; the *form* of a placeholder is the one
 *  convention this module allows itself, and it is per-engine because writing
 *  Oracle binds into a PostgreSQL script would not even parse. */
function placeholders(columns: string[], dialect: Dialect): string {
  return dialect === 'oracle'
    ? columns.map((c) => `:${c}`).join(', ')
    : columns.map((_, i) => `$${i + 1}`).join(', ');
}

// ── The INSERT family ─────────────────────────────────────────────────────────

/** `INSERT INTO t (` → the column list, closing the parenthesis if nothing does. */
function insertColumns(rel: TableInfo, src: string, pos: number): string | null {
  if (rel.columns.length === 0) return null;
  const list = rel.columns.map((c) => c.name).join(', ');
  return nextChar(src, pos) === ')' ? list : `${list})`;
}

/** `INSERT INTO t ` → the whole parenthesised list. */
function insertColumnList(rel: TableInfo, src: string, pos: number): string | null {
  if (rel.columns.length === 0) return null;
  const lead = /\s$/.test(src.slice(0, pos)) ? '' : ' ';
  return `${lead}(${rel.columns.map((c) => c.name).join(', ')})`;
}

/** `INSERT INTO t (A, B)` → the matching `VALUES` line. */
function valuesSkeleton(
  stmt: { tokens: SqlToken[] }, info: StatementInfo, dialect: Dialect, src: string, pos: number,
): string | null {
  const names: string[] = [];
  for (const i of [...info.insertListColumns].sort((a, b) => a - b)) {
    const tok = stmt.tokens[i];
    if (tok && (tok.kind === 'word' || tok.kind === 'quoted')) names.push(tok.text);
  }
  if (names.length === 0) return null;
  const { before } = lineAround(src, pos);
  const indent = /^\s*/.exec(before)?.[0] ?? '';
  return `\n${indent}VALUES (${placeholders(names, dialect)});`;
}

// ── The FK-implied join predicate ─────────────────────────────────────────────

interface JoinPredicate { left: string; right: string; }

/** Every equality a foreign key between `a` and `b` implies, in either direction. */
function fkPredicates(a: TableInfo, aq: string, b: TableInfo, bq: string): JoinPredicate[][] {
  const out: JoinPredicate[][] = [];
  const collect = (owner: TableInfo, ownerQ: string, other: TableInfo, otherQ: string) => {
    for (const fk of owner.foreignKeys ?? []) {
      if (fk.referencedTable.toUpperCase() !== other.name.toUpperCase()) continue;
      out.push(fk.columns.map((c, i) => ({
        left: `${ownerQ}.${c}`,
        right: `${otherQ}.${fk.referencedColumns[i] ?? fk.referencedColumns[0] ?? c}`,
      })));
    }
  };
  collect(a, aq, b, bq);
  collect(b, bq, a, aq);
  return out;
}

/**
 * `JOIN ORDINI o ON ` → `o.CLIENTE_ID = c.ID`.
 *
 * Only when **exactly one** foreign key connects the joined table to the ones
 * already in the statement. Two candidates means the tool does not know which
 * relationship you meant, and picking one would be the guess this feature exists
 * to avoid.
 */
async function joinPredicate(
  info: StatementInfo, onToken: SqlToken, view: SchemaView, connectionId: string | undefined,
): Promise<string | null> {
  const joined = [...info.relations].reverse().find((r) => !r.opaque && r.name && r.to <= onToken.from);
  if (!joined) return null;
  const others = info.relations.filter((r) => r !== joined && !r.opaque && r.name && r.to < joined.from);
  if (others.length === 0) return null;

  const joinedRel = await ensureRelationDetail(connectionId, joined.name) ?? view.relation(joined.name);
  if (!joinedRel) return null;

  const candidates: JoinPredicate[][] = [];
  for (const other of others) {
    const otherRel = await ensureRelationDetail(connectionId, other.name) ?? view.relation(other.name);
    if (!otherRel) continue;
    candidates.push(...fkPredicates(
      joinedRel, joined.alias || joined.name,
      otherRel, other.alias || other.name,
    ));
  }
  if (candidates.length !== 1) return null;
  return candidates[0].map((p) => `${p.left} = ${p.right}`).join(' AND ');
}

// ── Block closers ─────────────────────────────────────────────────────────────

type BlockKind = 'BEGIN' | 'IF' | 'LOOP' | 'CASE';

const CLOSER: Record<BlockKind, string> = {
  BEGIN: 'END;',
  IF: 'END IF;',
  LOOP: 'END LOOP;',
  CASE: 'END CASE;',
};

/** Words after which an `IF` opens a statement rather than continuing an expression. */
const STATEMENT_START = new Set(['BEGIN', 'THEN', 'ELSE', 'LOOP', 'DECLARE', 'EXCEPTION']);

/**
 * The stack of blocks still open at `limit`.
 *
 * `BEGIN`, `CASE`, `LOOP` and a statement-position `IF` push; `END` pops, swallowing
 * the `IF` / `LOOP` / `CASE` that may follow it so `END LOOP` does not re-open one.
 * Counting `CASE` too is what keeps an expression `CASE … END` from popping the
 * enclosing `BEGIN`.
 */
function blockStack(tokens: SqlToken[], limit: number): BlockKind[] {
  const stack: BlockKind[] = [];
  let prev: SqlToken | undefined;
  for (let i = 0; i < tokens.length; i++) {
    const t = tokens[i];
    if (t.from >= limit) break;
    if (t.kind === 'comment') continue;
    if (t.kind === 'word') {
      if (t.value === 'END') {
        stack.pop();
        const next = tokens[i + 1];
        if (next?.kind === 'word' && ['IF', 'LOOP', 'CASE'].includes(next.value)) i += 1;
      } else if (t.value === 'BEGIN') stack.push('BEGIN');
      else if (t.value === 'CASE') stack.push('CASE');
      else if (t.value === 'LOOP') stack.push('LOOP');
      else if (t.value === 'IF' && (!prev || prev.text === ';'
        || (prev.kind === 'word' && STATEMENT_START.has(prev.value)))) stack.push('IF');
    }
    prev = t;
  }
  return stack;
}

/**
 * The closer for the innermost block still open at the caret.
 *
 * Two conditions, both necessary: the caret is on an otherwise-blank line (so the
 * proposal never lands in the middle of a statement), and the block is still open
 * when the **whole buffer** is counted — editing inside a complete block must not
 * offer to close it a second time.
 */
function blockCloser(src: string, pos: number, dialect: Dialect, scan: TokenScan): string | null {
  const { before } = lineAround(src, pos);
  if (before.trim() !== '') return null;

  let tokens = scan.tokens;
  let suffix = '';

  // PostgreSQL: an unterminated `$$` body swallows everything into one string
  // token, so the block words have to be read out of the body itself.
  if (scan.open?.kind === 'dollar') {
    const bodyFrom = scan.open.bodyFrom;
    const body = tokenize(src.slice(bodyFrom), dialect);
    tokens = body.tokens.map((t) => ({ ...t, from: t.from + bodyFrom, to: t.to + bodyFrom }));
    // A `DO $$ … $$;` block ends in exactly one way. A function body ends with a
    // `LANGUAGE` clause that is nowhere in the text, so it is left alone.
    const opener = scan.tokens.filter((t) => t.kind === 'word' && t.to <= bodyFrom).pop();
    if (opener?.value === 'DO') suffix = `\n${scan.open.tag};`;
  }

  if (blockStack(tokens, src.length + 1).length === 0) return null;
  const open = blockStack(tokens, pos);
  const innermost = open[open.length - 1];
  if (!innermost) return null;
  return `${CLOSER[innermost]}${open.length === 1 ? suffix : ''}`;
}

// ── The source ────────────────────────────────────────────────────────────────

/**
 * Build the ghost-text source for one dialect, bound to one connection.
 *
 * Async because a foreign key may not have been read yet — the schema snapshot
 * carries no constraints, so the first `JOIN … ON ` on a table pulls its detail in
 * and the proposal appears on the next keystroke. Everything else answers
 * synchronously.
 */
export function createSqlGhostText(dialect: Dialect, connectionId?: string): InlineCompletionSource {
  return async function sqlGhostText(view: EditorView, pos: number): Promise<string | null> {
    const src = view.state.doc.toString();
    if (!atEndOfLine(src, pos)) return null;

    const { scan, statements } = scanSql(src, dialect);

    // A block closer is the one rule that works with no database at all — and the
    // one that has to run even when the caret is inside an unterminated `$$` body,
    // which is a "literal" as far as the scanner is concerned.
    if (scan.open?.kind === 'dollar' || !inLiteral(scan, pos)) {
      const closer = blockCloser(src, pos, dialect, scan);
      if (closer) return closer;
    }
    if (inLiteral(scan, pos)) return null;

    const schema = schemaViewFor(connectionId);
    if (!schema.known) return null;

    const stmt = statementAt(statements, pos);
    if (!stmt) return null;
    const info = analyzeStatement(stmt, dialect);
    const cc = caretContext(stmt, info, pos);
    const last = cc.lastIndex >= 0 ? stmt.tokens[cc.lastIndex] : null;

    // `JOIN t x ON |`
    if (last?.kind === 'word' && last.value === 'ON' && cc.clause === 'on') {
      return joinPredicate(info, last, schema, connectionId);
    }

    if (info.kind !== 'insert') return null;
    const target = writeTarget(info);
    const rel = target ? schema.relation(target.name) : null;
    if (!rel) return null;

    // `INSERT INTO t (|`
    if (cc.clause === 'insert-cols' && last?.text === '(' && (cc.insertListSoFar?.length ?? 0) === 0) {
      return insertColumns(rel, src, pos);
    }

    // `INSERT INTO t |`
    if (last && target && last.to === target.to && cc.clause !== 'insert-cols') {
      return insertColumnList(rel, src, pos);
    }

    // `INSERT INTO t (A, B)|`
    if (last?.text === ')' && cc.lastIndex === stmt.tokens.length - 1
      && info.insertListColumns.has(cc.lastIndex - 1)) {
      return valuesSkeleton(stmt, info, dialect, src, pos);
    }

    return null;
  };
}

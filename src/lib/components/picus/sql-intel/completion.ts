/**
 * SQL completion — the candidates come from the catalogue, never from a guess.
 *
 * The interesting part is not the list, it is the **scope**. After
 * `FROM CLIENTI c JOIN ORDINI o ON … WHERE c.` the only right answer is `CLIENTI`'s
 * columns; offering every column in the database instead is the difference between
 * a tool that knows the schema and a tool that has merely read it. So the source
 * always resolves the caret's statement first, then asks a narrower question:
 *
 *  • after a qualifier — the columns of the relation that alias resolves to;
 *  • in `FROM` / `JOIN` — tables, views and the statement's own CTEs;
 *  • inside `INSERT INTO t (…)` — `t`'s columns, minus the ones already listed;
 *  • after `UPDATE t SET` — `t`'s columns;
 *  • anywhere else — the columns in scope first, then relations, sequences and the
 *    dialect's keywords.
 *
 * When the schema is not known (no connection, or its catalogue has not been read)
 * the source degrades to keywords plus the identifiers already in the buffer,
 * which is what makes a script file with no database open still worth typing in.
 */

import type { Completion, CompletionContext, CompletionResult, CompletionSource } from '@codemirror/autocomplete';
import type { Dialect, TableInfo } from '$lib/types/picus';
import { analyzeStatement, caretContext, resolveQualifier, type RelationRef, type StatementInfo } from './analysis';
import { keywordsFor, RESERVED } from './keywords';
import { schemaViewFor, type SchemaView } from './schema-view';
import { inLiteral, scanSql, statementAt } from './tokens';

/** Word under construction. Oracle allows `$`/`#`, and completing a name that
 *  contains one has to keep the whole name as the replaced range. */
const WORD_BEFORE = /[A-Za-z_][A-Za-z0-9_$#]*$/;
const VALID_FOR = /^[A-Za-z0-9_$#]*$/;

/** A hard ceiling on the popup. A schema with thousands of columns must not turn
 *  one keystroke into a list nobody can read (or a frame nobody sees). */
const MAX_OPTIONS = 500;

/** Boosts. Higher wins; the ordering is "what is in scope, then what exists, then
 *  what is merely grammatical". */
const BOOST = { scopedColumn: 3, alias: 2, relation: 1, sequence: 0, keyword: -1, bufferWord: -2 };

function relationCompletion(rel: TableInfo): Completion {
  return {
    label: rel.name,
    type: rel.kind === 'view' ? 'interface' : 'class',
    detail: rel.kind,
    info: rel.columns.length
      ? `${rel.columns.length} columns${rel.estimatedRows != null ? ` · ~${rel.estimatedRows} rows` : ''}`
      : undefined,
    boost: BOOST.relation,
  };
}

function columnCompletion(rel: TableInfo | null, name: string, type: string, boost: number): Completion {
  return {
    label: name,
    type: 'property',
    detail: type,
    info: rel ? rel.name : undefined,
    boost,
  };
}

/** Every column of every non-opaque relation the statement has in scope. */
function scopedColumns(info: StatementInfo, view: SchemaView, out: Completion[], seen: Set<string>) {
  for (const ref of info.relations) {
    if (ref.opaque || !ref.name) continue;
    const rel = view.relation(ref.name);
    if (!rel) continue;
    for (const col of rel.columns) {
      const key = `c:${col.name.toUpperCase()}`;
      if (seen.has(key)) continue;
      seen.add(key);
      out.push(columnCompletion(rel, col.name, col.type, BOOST.scopedColumn));
    }
  }
}

/** The aliases the statement declared — typing `c` and getting `c` back is not
 *  useless: it is the confirmation that the alias is in scope. */
function aliasCompletions(info: StatementInfo, out: Completion[]) {
  for (const ref of info.relations) {
    if (!ref.alias) continue;
    out.push({
      label: ref.alias,
      type: 'variable',
      detail: ref.name || 'derived',
      boost: BOOST.alias,
    });
  }
}

/** Identifiers already written in this buffer — the only source left when there is
 *  no catalogue, and harmless noise when there is one (so it is used only when
 *  there isn't). */
function bufferWords(src: string, out: Completion[], seen: Set<string>) {
  const re = /[A-Za-z_][A-Za-z0-9_$#]{2,}/g;
  let m: RegExpExecArray | null;
  let scanned = 0;
  while ((m = re.exec(src)) !== null) {
    if (++scanned > 5000 || out.length >= MAX_OPTIONS) break;
    const w = m[0];
    const key = `b:${w.toUpperCase()}`;
    if (seen.has(key) || RESERVED.has(w.toUpperCase())) continue;
    seen.add(key);
    out.push({ label: w, type: 'text', boost: BOOST.bufferWord });
  }
}

/** Columns of the relation a qualifier resolves to, plus `*`. Returns `null` when
 *  the qualifier resolves to nothing we know — no popup at all is the right answer
 *  there, because an unresolved `x.` is far more likely to be a package, a record
 *  or a schema than a typo we can help with. */
function qualifiedCandidates(
  qualifier: string, info: StatementInfo | null, view: SchemaView, dialect: Dialect,
): Completion[] | null {
  const ref = info ? resolveQualifier(info, qualifier) : null;
  if (ref?.opaque) return null;                       // derived table: columns unknowable
  const rel = view.relation(ref?.name || qualifier);
  if (rel) {
    const out: Completion[] = rel.columns.map((c) =>
      columnCompletion(rel, c.name, c.type, BOOST.scopedColumn));
    out.push({ label: '*', type: 'constant', detail: 'all columns', boost: BOOST.relation });
    return out;
  }
  // `MY_SEQ.` in Oracle — the only two things that can follow are facts.
  if (dialect === 'oracle' && view.sequence(qualifier)) {
    return [
      { label: 'NEXTVAL', type: 'property', detail: 'next sequence value' },
      { label: 'CURRVAL', type: 'property', detail: 'current sequence value' },
    ];
  }
  return null;
}

/** The relation an `INSERT INTO t (…)` or `UPDATE t SET` is writing to. */
function writeTarget(info: StatementInfo): RelationRef | null {
  if (info.kind === 'insert' || info.kind === 'merge') {
    return info.relations.find((r) => r.target) ?? null;
  }
  if (info.kind === 'update') return info.relations[0] ?? null;
  return null;
}

/**
 * Build the completion source for one dialect, bound to one connection.
 *
 * Both are fixed at descriptor creation, never read from a global: the dialect is
 * a property of the tab (a connection's engine, or the file's folder), and the
 * connection decides which catalogue — if any — this buffer may be checked against.
 */
export function createSqlCompletion(dialect: Dialect, connectionId?: string): CompletionSource {
  const keywords = keywordsFor(dialect);

  return function sqlCompletion(ctx: CompletionContext): CompletionResult | null {
    const src = ctx.state.doc.toString();
    const { scan, statements } = scanSql(src, dialect);
    // Never inside a comment or a string literal — there is nothing to complete
    // there and the popup is pure interruption.
    if (inLiteral(scan, ctx.pos)) return null;

    const word = ctx.matchBefore(WORD_BEFORE);
    const afterDot = ctx.matchBefore(/\.$/) != null;
    if (!ctx.explicit && !afterDot && (!word || word.from === word.to)) return null;

    const from = word ? word.from : ctx.pos;
    const view = schemaViewFor(connectionId);
    const stmt = statementAt(statements, ctx.pos);
    const info = stmt ? analyzeStatement(stmt, dialect) : null;
    const cc = stmt && info ? caretContext(stmt, info, ctx.pos) : null;

    // ── After a qualifier: one relation's columns, or nothing ─────────────────
    if (cc?.qualifier) {
      const options = qualifiedCandidates(cc.qualifier, info, view, dialect);
      return options && options.length ? { from, options, validFor: VALID_FOR } : null;
    }

    const options: Completion[] = [];
    const seen = new Set<string>();

    // ── Inside an INSERT column list: the target's columns, minus the typed ones ─
    if (cc?.clause === 'insert-cols' && info) {
      const target = writeTarget(info);
      const rel = target ? view.relation(target.name) : null;
      if (!rel) return null;
      const already = new Set((cc.insertListSoFar ?? []).map((n) => n.toUpperCase()));
      for (const col of rel.columns) {
        if (already.has(col.name.toUpperCase())) continue;
        options.push(columnCompletion(rel, col.name, col.type, BOOST.scopedColumn));
      }
      return options.length ? { from, options, validFor: VALID_FOR } : null;
    }

    // ── After UPDATE t SET: the target's columns first, then the operators ─────
    if (cc?.clause === 'set' && info) {
      const target = writeTarget(info);
      const rel = target ? view.relation(target.name) : null;
      if (rel) {
        for (const col of rel.columns) {
          seen.add(`c:${col.name.toUpperCase()}`);
          options.push(columnCompletion(rel, col.name, col.type, BOOST.scopedColumn));
        }
      }
    }

    // ── In FROM / JOIN: things you can select from ────────────────────────────
    const namingRelation = cc?.clause === 'from';
    if (namingRelation || cc?.clause === 'none' || cc?.clause === 'select' || !cc) {
      for (const name of info?.cteNames ?? []) {
        options.push({ label: name, type: 'namespace', detail: 'CTE', boost: BOOST.alias });
      }
      for (const rel of view.relations) {
        if (options.length >= MAX_OPTIONS) break;
        seen.add(`r:${rel.name.toUpperCase()}`);
        options.push(relationCompletion(rel));
      }
    }

    if (!namingRelation) {
      if (info) {
        scopedColumns(info, view, options, seen);
        aliasCompletions(info, options);
      }
      if (cc?.clause !== 'set') {
        for (const rel of view.relations) {
          if (options.length >= MAX_OPTIONS) break;
          if (seen.has(`r:${rel.name.toUpperCase()}`)) continue;
          seen.add(`r:${rel.name.toUpperCase()}`);
          options.push(relationCompletion(rel));
        }
      }
      for (const seq of view.sequences) {
        if (options.length >= MAX_OPTIONS) break;
        options.push({
          label: seq.name,
          type: 'enum',
          detail: 'sequence',
          info: `last value ${seq.lastValue}`,
          boost: BOOST.sequence,
        });
      }
    }

    for (const kw of keywords) options.push({ label: kw, type: 'keyword', boost: BOOST.keyword });

    // With no catalogue the buffer is the only evidence there is — and a script
    // file open with no database connected is a normal, frequent situation.
    if (!view.known) bufferWords(src, options, seen);

    if (options.length === 0) return null;
    return { from, options: options.slice(0, MAX_OPTIONS), validFor: VALID_FOR };
  };
}

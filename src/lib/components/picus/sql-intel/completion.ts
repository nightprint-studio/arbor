/**
 * SQL completion — the candidates come from the catalogue, never from a guess.
 *
 * The interesting part is not the list, it is the **scope**. After
 * `FROM CLIENTI c JOIN ORDINI o ON … WHERE c.` the only right answer is `CLIENTI`'s
 * columns; offering every column in the database instead is the difference between
 * a tool that knows the schema and a tool that has merely read it. So the source
 * always resolves the caret's statement first, then asks two narrower questions:
 * **what may go here at all** — that is {@link expectationAt}, in
 * `continuations.ts` — and then, only for the families that survive, which names
 * the catalogue actually has.
 *
 * That split is what fixed the two complaints this file was rewritten for:
 *
 *  • an empty statement was answered with every table in the database, when no
 *    statement in SQL begins with a table name;
 *  • keywords were appended last and then cut by the option ceiling, so on a real
 *    schema typing `SEL` never offered `SELECT`. Keywords are now built as their
 *    own list and the schema half is what gets truncated.
 *
 * Two things here are predictions in the honest sense — derived from facts,
 * absent when the fact is missing:
 *
 *  • a column whose name exists in **more than one** relation in scope is offered
 *    only in its qualified forms (`c.ID`, `o.ID`). Unqualified, it would not run;
 *    offering it is offering an error.
 *  • in `FROM` / `JOIN`, relations reachable by a **foreign key** from what is
 *    already in the statement are boosted and say which key. There is no guessing
 *    involved: either the constraint exists or the table ranks normally.
 *
 * When the schema is not known (no connection, or its catalogue has not been read)
 * the source degrades to keywords plus the identifiers already in the buffer,
 * which is what makes a script file with no database open still worth typing in.
 */

import type { Completion, CompletionContext, CompletionResult, CompletionSource } from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import type { Dialect, TableInfo } from '$lib/types/picus';
import {
  analyzeStatement, caretContext, resolveQualifier,
  type RelationRef, type StatementInfo,
} from './analysis';
import { constantsFor, expectationAt, functionsFor, type Expectation, type NameKind } from './continuations';
import { RESERVED } from './keywords';
import { ensureRelationDetail, schemaViewFor, type SchemaView } from './schema-view';
import { inLiteral, scanSql, statementAt, type SqlStatement, type SqlToken } from './tokens';

/** Word under construction. Oracle allows `$`/`#`, and completing a name that
 *  contains one has to keep the whole name as the replaced range. */
const WORD_BEFORE = /[A-Za-z_][A-Za-z0-9_$#]*$/;
const VALID_FOR = /^[A-Za-z0-9_$#]*$/;

/** A hard ceiling on the popup. A schema with thousands of columns must not turn
 *  one keystroke into a list nobody can read (or a frame nobody sees). Applied to
 *  the **schema** half only — the keyword half is short and always survives. */
const MAX_OPTIONS = 400;

/** How many in-scope relations are asked for their foreign keys. A statement that
 *  joins twelve tables does not need the thirteenth round trip to rank a popup. */
const FK_SCOPE_LIMIT = 4;

/**
 * Boosts. Higher wins.
 *
 * The ordering is "what the grammar expects, then what is in scope, then what
 * exists, then what is merely grammatical".
 */
const BOOST = {
  expectedKeyword: 6,
  column: 4,
  alias: 3,
  relatedRelation: 3,
  relation: 1,
  fn: 0,
  sequence: -1,
  keyword: -2,
  bufferWord: -3,
};

// ── Option builders ───────────────────────────────────────────────────────────

function relationCompletion(rel: TableInfo, related: string): Completion {
  return {
    label: rel.name,
    type: rel.kind === 'view' ? 'interface' : 'class',
    detail: related || rel.kind,
    info: rel.columns.length
      ? `${rel.columns.length} columns${rel.estimatedRows != null ? ` · ~${rel.estimatedRows} rows` : ''}`
      : undefined,
    boost: related ? BOOST.relatedRelation : BOOST.relation,
  };
}

function columnCompletion(label: string, type: string, owner: string): Completion {
  return { label, type: 'property', detail: type, info: owner || undefined, boost: BOOST.column };
}

function keywordCompletion(word: string, boost: number): Completion {
  return { label: word, type: 'keyword', boost };
}

/** A function, applied as `NAME()` with the caret **between** the parentheses —
 *  which is the only reason offering one is better than typing it. */
function functionCompletion(name: string): Completion {
  return {
    label: `${name}()`,
    type: 'function',
    boost: BOOST.fn,
    apply: (view: EditorView, _c: Completion, from: number, to: number) => {
      view.dispatch({
        changes: { from, to, insert: `${name}()` },
        selection: { anchor: from + name.length + 1 },
      });
    },
  };
}

/**
 * A sequence, applied in the form the engine actually accepts.
 *
 * `SEQ.NEXTVAL` on Oracle and `nextval('seq')` on PostgreSQL are not two spellings
 * of one thing — each is a syntax error on the other engine. The dialect is known
 * here, so the completion writes the right one instead of the name and a guess.
 */
function sequenceCompletion(name: string, lastValue: number | undefined, dialect: Dialect, callable: boolean): Completion {
  const inserted = dialect === 'oracle' ? `${name}.NEXTVAL` : `nextval('${name}')`;
  return {
    label: name,
    type: 'enum',
    detail: callable ? inserted : 'sequence',
    info: lastValue != null ? `last value ${lastValue}` : undefined,
    boost: BOOST.sequence,
    apply: callable ? inserted : undefined,
  };
}

// ── Names in scope ────────────────────────────────────────────────────────────

/**
 * Every column of every non-opaque relation the statement has in scope.
 *
 * A name carried by two of them is emitted **only** qualified: `ID` alone would
 * not run against `CLIENTI c JOIN ORDINI o`, and a completion that produces a
 * statement the server refuses is worse than no completion.
 */
function scopedColumns(info: StatementInfo, view: SchemaView, out: Completion[]) {
  /** column name → the relations that carry it, in statement order. */
  const owners = new Map<string, Array<{ rel: TableInfo; ref: RelationRef; type: string; name: string }>>();

  for (const ref of info.relations) {
    if (ref.opaque || !ref.name || ref.target) continue;
    const rel = view.relation(ref.name);
    if (!rel) continue;
    for (const col of rel.columns) {
      const key = col.name.toUpperCase();
      const list = owners.get(key) ?? [];
      list.push({ rel, ref, type: col.type, name: col.name });
      owners.set(key, list);
    }
  }

  for (const list of owners.values()) {
    if (list.length === 1) {
      const [only] = list;
      out.push(columnCompletion(only.name, only.type, only.rel.name));
      continue;
    }
    for (const owner of list) {
      const qualifier = owner.ref.alias || owner.ref.name;
      out.push(columnCompletion(`${qualifier}.${owner.name}`, owner.type, owner.rel.name));
    }
  }
}

/** The columns of the statement's write target — an `INSERT`'s or an `UPDATE`'s. */
function targetColumns(info: StatementInfo, view: SchemaView, out: Completion[], skip: Set<string>) {
  const target = writeTarget(info);
  const rel = target ? view.relation(target.name) : null;
  if (!rel) return;
  for (const col of rel.columns) {
    if (skip.has(col.name.toUpperCase())) continue;
    out.push(columnCompletion(col.name, col.type, rel.name));
  }
}

/** The aliases the statement declared — typing `c` and getting `c` back is not
 *  useless: it is the confirmation that the alias is in scope. */
function aliasCompletions(info: StatementInfo, out: Completion[]) {
  for (const ref of info.relations) {
    if (!ref.alias) continue;
    out.push({ label: ref.alias, type: 'variable', detail: ref.name || 'derived', boost: BOOST.alias });
  }
}

/** Identifiers already written in this buffer — the only source left when there is
 *  no catalogue, and harmless noise when there is one (so it is used only when
 *  there isn't). */
function bufferWords(src: string, out: Completion[]) {
  const re = /[A-Za-z_][A-Za-z0-9_$#]{2,}/g;
  const seen = new Set<string>();
  let m: RegExpExecArray | null;
  let scanned = 0;
  while ((m = re.exec(src)) !== null) {
    if (++scanned > 5000 || out.length >= MAX_OPTIONS) break;
    const w = m[0];
    const key = w.toUpperCase();
    if (seen.has(key) || RESERVED.has(key)) continue;
    seen.add(key);
    out.push({ label: w, type: 'text', boost: BOOST.bufferWord });
  }
}

/**
 * The relations a foreign key connects to what the statement already names.
 *
 * Both directions, as far as the catalogue can answer them: the keys **out of**
 * the tables in scope are pulled in on demand, and the keys **into** them are read
 * from whatever relations already carry their detail. Incoming links are therefore
 * partial by construction — reading every constraint in a database to rank a popup
 * would not be worth it — so a missing boost only ever costs ordering, never a
 * candidate.
 *
 * The value is the name of the key, so the popup can say *why* a table is near the
 * top rather than just putting it there.
 */
async function relatedRelations(
  info: StatementInfo, view: SchemaView, connectionId: string | undefined,
): Promise<Map<string, string>> {
  const out = new Map<string, string>();
  const scope = info.relations.filter((r) => !r.opaque && r.name).slice(0, FK_SCOPE_LIMIT);
  if (scope.length === 0) return out;

  const inScope = new Set<string>();
  for (const ref of scope) {
    const rel = await ensureRelationDetail(connectionId, ref.name) ?? view.relation(ref.name);
    if (!rel) continue;
    inScope.add(rel.name.toUpperCase());
    for (const fk of rel.foreignKeys ?? []) {
      out.set(fk.referencedTable.toUpperCase(), `→ ${rel.name}.${fk.columns[0] ?? ''}`);
    }
  }

  for (const rel of view.relations) {
    if (rel.foreignKeys === undefined) continue;              // detail never read
    if (inScope.has(rel.name.toUpperCase())) continue;
    for (const fk of rel.foreignKeys) {
      if (!inScope.has(fk.referencedTable.toUpperCase())) continue;
      out.set(rel.name.toUpperCase(), `${rel.name}.${fk.columns[0] ?? ''} →`);
    }
  }
  return out;
}

// ── Qualified completion (`c.` / `MY_SEQ.`) ───────────────────────────────────

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
    const out: Completion[] = rel.columns.map((c) => columnCompletion(c.name, c.type, rel.name));
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

// ── Reading the caret's neighbourhood ─────────────────────────────────────────

/** The last token that ends at or before `at`, comments skipped. */
function tokenBefore(tokens: SqlToken[], at: number): SqlToken | null {
  let found: SqlToken | null = null;
  for (const tok of tokens) {
    if (tok.to > at) break;
    if (tok.kind !== 'comment') found = tok;
  }
  return found;
}

/**
 * Is the caret sitting just past a completed table reference?
 *
 * True for `FROM ORDINI |` and `FROM ORDINI o |`, false for `FROM |` and for
 * `FROM ORDINI WHERE |` — the last one only because a reserved word is never a
 * table's alias, which is the cheap test that keeps this honest without a parse.
 */
function afterRelationReference(tokens: SqlToken[], info: StatementInfo, at: number): boolean {
  const rel = [...info.relations].reverse().find((r) => r.to <= at);
  if (!rel) return false;
  const last = tokenBefore(tokens, at);
  if (!last || (last.kind !== 'word' && last.kind !== 'quoted')) return false;
  if (last.kind === 'word' && RESERVED.has(last.value)) return false;
  return last.to >= rel.to;
}

// ── The source ────────────────────────────────────────────────────────────────

/** Names for one {@link NameKind}, in the order they should rank. */
function namesFor(
  kind: NameKind,
  facts: { info: StatementInfo | null; view: SchemaView; dialect: Dialect; insertListSoFar: string[] | null; related: Map<string, string> },
): Completion[] {
  const { info, view, dialect, related } = facts;
  const out: Completion[] = [];

  const relations = () => {
    for (const name of info?.cteNames ?? []) {
      out.push({ label: name, type: 'namespace', detail: 'CTE', boost: BOOST.alias });
    }
    for (const rel of view.relations) {
      if (out.length >= MAX_OPTIONS) break;
      out.push(relationCompletion(rel, related.get(rel.name.toUpperCase()) ?? ''));
    }
  };
  const sequences = (callable: boolean) => {
    for (const seq of view.sequences) {
      if (out.length >= MAX_OPTIONS) break;
      out.push(sequenceCompletion(seq.name, seq.lastValue, dialect, callable));
    }
  };

  switch (kind) {
    case 'none':
      break;
    case 'relations':
      relations();
      break;
    case 'columns':
      if (info) { scopedColumns(info, view, out); aliasCompletions(info, out); }
      break;
    case 'target-columns':
      if (info) {
        const already = new Set((facts.insertListSoFar ?? []).map((n) => n.toUpperCase()));
        targetColumns(info, view, out, already);
      }
      break;
    case 'values':
      sequences(true);
      break;
    case 'any':
      if (info) { scopedColumns(info, view, out); aliasCompletions(info, out); }
      relations();
      sequences(false);
      break;
  }
  return out;
}

/**
 * Build the completion source for one dialect, bound to one connection.
 *
 * Both are fixed at descriptor creation, never read from a global: the dialect is
 * a property of the tab (a connection's engine, or the file's folder), and the
 * connection decides which catalogue — if any — this buffer may be checked against.
 *
 * Async only because a foreign key may not have been read yet. Everything else is
 * answered from what is already in memory, and the first pass over a statement
 * simply ranks without the boost.
 */
export function createSqlCompletion(dialect: Dialect, connectionId?: string): CompletionSource {
  return async function sqlCompletion(ctx: CompletionContext): Promise<CompletionResult | null> {
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
    const stmt: SqlStatement | null = statementAt(statements, ctx.pos);
    const info = stmt ? analyzeStatement(stmt, dialect) : null;
    const cc = stmt && info ? caretContext(stmt, info, ctx.pos) : null;

    // ── After a qualifier: one relation's columns, or nothing ─────────────────
    if (cc?.qualifier) {
      const options = qualifiedCandidates(cc.qualifier, info, view, dialect);
      return options && options.length ? { from, options, validFor: VALID_FOR } : null;
    }

    // ── What may go here at all ───────────────────────────────────────────────
    const previous = stmt ? tokenBefore(stmt.tokens, from) : null;
    const expectation: Expectation = expectationAt(
      {
        clause: cc?.clause ?? 'none',
        previousWord: previous?.kind === 'word' ? previous.value : '',
        // A terminator counts as the start of the next one: the splitter may hand
        // back the statement that just ended, and offering its clause keywords
        // after the `;` would be answering a question nobody asked.
        atStatementStart: !stmt || previous === null
          || (previous.kind === 'punct' && previous.text === ';'),
        afterRelationRef: !!stmt && !!info && afterRelationReference(stmt.tokens, info, from),
        hasPrefix: !!word && word.to > word.from,
        info,
      },
      dialect,
    );

    // Keywords are built first and never truncated: the schema is what a ceiling
    // is for. This is the whole of the "typing SEL offers no SELECT" fix.
    const keywordBoost = expectation.keywordsFirst ? BOOST.expectedKeyword : BOOST.keyword;
    const options: Completion[] = expectation.keywords.map((k) => keywordCompletion(k, keywordBoost));

    if (expectation.exclusive) {
      return options.length ? { from, options, validFor: VALID_FOR } : null;
    }

    if (expectation.functions) {
      for (const fn of functionsFor(dialect)) options.push(functionCompletion(fn));
      for (const c of constantsFor(dialect)) options.push(keywordCompletion(c, BOOST.fn));
    }

    const names = namesFor(expectation.names, {
      info,
      view,
      dialect,
      insertListSoFar: cc?.insertListSoFar ?? null,
      // Only worth asking in the clause where relations are the candidates.
      related: expectation.names === 'relations' && info
        ? await relatedRelations(info, view, connectionId)
        : new Map<string, string>(),
    });
    options.push(...names.slice(0, Math.max(0, MAX_OPTIONS - options.length)));

    // With no catalogue the buffer is the only evidence there is — and a script
    // file open with no database connected is a normal, frequent situation.
    if (!view.known) bufferWords(src, options);

    if (options.length === 0) return null;
    return { from, options, validFor: VALID_FOR };
  };
}

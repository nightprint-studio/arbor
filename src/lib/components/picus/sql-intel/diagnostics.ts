/**
 * Live SQL diagnostics — what is wrong with this statement *before* you run it.
 *
 * Four rules, and they were chosen because each one is a fact rather than a
 * judgement:
 *
 *  1. **Unknown table or view** — the name is not in the catalogue this connection
 *     reported, and is not created earlier in the same buffer.
 *  2. **Unknown column** — a *qualified* reference (`c.NOEM`) whose alias resolves
 *     to a known table that has no such column.
 *  3. **Ambiguous unqualified column** — the bare name exists in two of the
 *     relations the statement joins, so the server will refuse to choose.
 *  4. **A write on a read-only connection** — the server will refuse it; saying so
 *     now saves the round trip and the confusing error.
 *
 * ## What is deliberately *not* checked
 *
 * The whole value of this feature is that it is quiet when it does not know, so
 * the exclusions are as load-bearing as the rules:
 *
 * - **No catalogue, no object diagnostics.** `SchemaView.known` is false whenever
 *   the buffer is not bound to a connection whose schema has actually been read —
 *   and then nothing at all is reported. An unread schema must never look like an
 *   empty one; see `schema-view.ts`.
 * - **A different schema is not an unknown schema.** `ALTRO.CLIENTI` on a session
 *   pinned to `PUBLIC` is skipped: we have no catalogue for `ALTRO`.
 * - **DDL is never checked against the live schema.** A script whose job is to
 *   create an object would otherwise be one long list of "unknown table", which is
 *   exactly backwards. For the same reason, anything the buffer creates earlier
 *   counts as existing.
 * - **Blocks are not checked as blocks.** A statement whose first word is
 *   procedural stands down entirely.
 * - **Unqualified names are never reported as unknown**, only as ambiguous. A bare
 *   word can be an output alias, a function, a PL/SQL variable or a literal; the
 *   scanner cannot tell, so it does not try.
 * - **A statement containing a subquery skips the ambiguity check**, because the
 *   inner scope is wider than what is collected at the top level.
 *
 * Offsets come out in **UTF-8 bytes**, which is the wire coordinate
 * `EditorDiagnostic` is defined in; the editor core maps them back.
 */

import type { EditorDiagnostic } from '$lib/components/shared/ui/code-editor';
import { makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import type { Dialect, TableInfo } from '$lib/types/picus';
import {
  analyzeStatement, identOf, resolveQualifier,
  type RelationRef, type StatementInfo,
} from './analysis';
import { RESERVED } from './keywords';
import { inReadableSchema, schemaViewFor, type SchemaView } from './schema-view';
import { scanSql, type SqlStatement, type SqlToken } from './tokens';

/** Above this the analysis is skipped entirely. A 200 KB script is a generated
 *  dump, not something anyone is typing into, and a linear scan per keystroke on
 *  one is exactly the kind of thing that makes an editor feel heavy. */
const MAX_ANALYSED_CHARS = 200_000;

/** However wrong a buffer is, a hundred markers is already more than anyone reads. */
const MAX_DIAGNOSTICS = 100;

interface Marker {
  from: number;   // UTF-16 while collecting; converted to bytes at the end
  to: number;
  severity: EditorDiagnostic['severity'];
  message: string;
}

// ── Objects the buffer itself creates ─────────────────────────────────────────

const CREATABLE = new Set(['TABLE', 'VIEW', 'SEQUENCE']);

/**
 * Names created by the buffer's own DDL.
 *
 * An initialisation script's whole purpose is to bring objects into existence, so
 * a table it creates on line 10 is not "unknown" on line 40 — it is simply not in
 * the database *yet*. Without this rule the feature would be useless on precisely
 * the files Picus exists to maintain.
 */
function createdInBuffer(statements: SqlStatement[]): Set<string> {
  const names = new Set<string>();
  for (const stmt of statements) {
    const t = stmt.tokens;
    if (t[0]?.kind !== 'word' || t[0].value !== 'CREATE') continue;
    for (let i = 1; i < Math.min(t.length, 10); i++) {
      if (t[i].kind !== 'word' || !CREATABLE.has(t[i].value)) continue;
      let j = i + 1;
      // `CREATE TABLE IF NOT EXISTS x`
      while (t[j]?.kind === 'word' && ['IF', 'NOT', 'EXISTS'].includes(t[j].value)) j += 1;
      let name = identOf(t[j]);
      if (t[j + 1]?.text === '.' && (t[j + 2]?.kind === 'word' || t[j + 2]?.kind === 'quoted')) {
        name = identOf(t[j + 2]);
      }
      if (name) names.add(name.toUpperCase());
      break;
    }
  }
  return names;
}

// ── Rules ─────────────────────────────────────────────────────────────────────

function checkUnknownRelations(
  info: StatementInfo, view: SchemaView, created: Set<string>, out: Marker[],
) {
  const ctes = new Set(info.cteNames.map((n) => n.toUpperCase()));
  for (const ref of info.relations) {
    if (ref.opaque || !ref.name) continue;
    if (!inReadableSchema(view, ref.schema)) continue;
    const upper = ref.name.toUpperCase();
    if (ctes.has(upper) || created.has(upper)) continue;
    if (view.relation(ref.name)) continue;
    out.push({
      from: ref.from,
      to: ref.to,
      severity: 'warning',
      message: `"${ref.name}" is not a table or view in ${view.schemaName || 'this schema'}.`,
    });
  }
}

/** `alias.column` where the alias resolves to a table that has no such column. */
function checkQualifiedColumns(stmt: SqlStatement, info: StatementInfo, view: SchemaView, out: Marker[]) {
  const t = stmt.tokens;
  for (let i = 1; i < t.length - 1; i++) {
    if (t[i].kind !== 'punct' || t[i].text !== '.') continue;
    const left = t[i - 1];
    const right = t[i + 1];
    if (!isName(left) || !isName(right)) continue;                 // `t.*`, `x.1`
    const ref = resolveQualifier(info, identOf(left));
    if (!ref || ref.opaque || !ref.name) continue;                 // a schema, a package, a record
    const rel = view.relation(ref.name);
    if (!rel) continue;                                            // already reported as unknown
    const name = identOf(right);
    if (rel.columns.some((c) => c.name.toUpperCase() === name.toUpperCase())) continue;
    out.push({
      from: right.from,
      to: right.to,
      severity: 'warning',
      message: `${rel.name} has no column "${name}".`,
    });
  }
}

/** The same bare column name in two of the joined relations. */
function checkAmbiguousColumns(stmt: SqlStatement, info: StatementInfo, view: SchemaView, out: Marker[]) {
  if (info.hasSubquery) return;

  // A write target is not a row source: in `INSERT INTO A (…) SELECT … FROM B`
  // the columns of A are not in the SELECT's scope, and counting them would
  // manufacture an ambiguity that does not exist.
  const scope: Array<{ ref: RelationRef; rel: TableInfo }> = [];
  for (const ref of info.relations) {
    if (ref.opaque || !ref.name || ref.target) continue;
    const rel = view.relation(ref.name);
    if (rel) scope.push({ ref, rel });
  }
  if (scope.length < 2) return;

  const aliases = new Set(info.relations.flatMap((r) => [r.alias, r.name])
    .filter(Boolean).map((s) => s.toUpperCase()));
  const declared = new Set([...info.declaredAliases].map((s) => s.toUpperCase()));

  const t = stmt.tokens;
  let depth = 0;
  let inSet = false;

  for (let i = 0; i < t.length; i++) {
    const tok = t[i];
    if (tok.kind === 'punct') {
      if (tok.text === '(') depth += 1;
      else if (tok.text === ')') depth = Math.max(0, depth - 1);
      continue;
    }
    if (tok.kind !== 'word') continue;

    // `UPDATE t SET x = …` — the left-hand names are scoped to the target by the
    // grammar, so they can never be ambiguous.
    if (depth === 0) {
      if (tok.value === 'SET') { inSet = true; continue; }
      if (['WHERE', 'FROM', 'RETURNING', 'VALUES'].includes(tok.value)) inSet = false;
    }
    if (inSet) continue;

    if (RESERVED.has(tok.value)) continue;
    if (info.usingColumns.has(i) || info.insertListColumns.has(i)) continue;
    if (isPunct(t[i - 1], '.') || isPunct(t[i + 1], '.')) continue;   // qualified either way
    if (isPunct(t[i + 1], '(')) continue;                             // a function call
    if (t[i - 1]?.kind === 'word' && t[i - 1].value === 'AS') continue;
    if (aliases.has(tok.value) || declared.has(tok.value)) continue;

    const owners = scope.filter((s) => s.rel.columns.some((c) => c.name.toUpperCase() === tok.value));
    if (owners.length < 2) continue;

    const shown = owners.slice(0, 2).map((o) => o.ref.alias || o.rel.name);
    out.push({
      from: tok.from,
      to: tok.to,
      severity: 'warning',
      message: `"${tok.text}" is a column of both ${shown[0]} and ${shown[1]} — qualify it.`,
    });
  }
}

function isName(t: SqlToken | undefined): boolean {
  return !!t && (t.kind === 'word' || t.kind === 'quoted');
}

function isPunct(t: SqlToken | undefined, text: string): boolean {
  return !!t && t.kind === 'punct' && t.text === text;
}

// ── Entry point ───────────────────────────────────────────────────────────────

/** Analyse one statement (and, recursively, the bodies of its CTEs). */
function checkStatement(
  stmt: SqlStatement, dialect: Dialect, view: SchemaView, created: Set<string>, out: Marker[],
) {
  const info = analyzeStatement(stmt, dialect);

  if (view.readOnly && info.isWrite && info.leading) {
    out.push({
      from: info.leading.from,
      to: info.leading.to,
      severity: 'error',
      message: `This connection is read-only — the server will refuse ${identOf(info.leading)}.`,
    });
  }

  if (!view.known || info.procedural || info.kind === 'ddl') return;

  checkUnknownRelations(info, view, created, out);
  checkQualifiedColumns(stmt, info, view, out);
  checkAmbiguousColumns(stmt, info, view, out);

  // A `WITH x AS ( … )` body is a statement in its own right; analysing it inside
  // the outer walk would mix the two scopes, so it gets its own pass.
  for (const body of info.cteBodies) {
    const tokens = stmt.tokens.slice(body.start, body.end);
    if (tokens.length === 0) continue;
    checkStatement(
      { tokens, from: tokens[0].from, to: tokens[tokens.length - 1].to },
      dialect, view, created, out,
    );
  }
}

/**
 * Diagnostics for a buffer, in UTF-8 byte offsets.
 *
 * Pure with respect to the editor: a Svelte `$derived` calls it and hands the
 * result to `CodeEditor`'s `diagnostics` prop, so it re-runs when the text, the
 * connection or the schema changes and never at any other time.
 */
export function sqlDiagnostics(
  text: string, dialect: Dialect, connectionId?: string,
): EditorDiagnostic[] {
  if (!text || text.length > MAX_ANALYSED_CHARS) return [];

  const view = schemaViewFor(connectionId);
  // Nothing to say at all: no catalogue and no connection to refuse a write.
  if (!view.known && !view.readOnly) return [];

  const { statements } = scanSql(text, dialect);
  const created = createdInBuffer(statements);
  const markers: Marker[] = [];

  for (const stmt of statements) {
    if (markers.length >= MAX_DIAGNOSTICS) break;
    checkStatement(stmt, dialect, view, created, markers);
  }

  const u2b = makeU16ToByte(text);
  return markers.slice(0, MAX_DIAGNOSTICS).map((m) => ({
    from: u2b(m.from),
    to: u2b(m.to),
    severity: m.severity,
    message: m.message,
  }));
}

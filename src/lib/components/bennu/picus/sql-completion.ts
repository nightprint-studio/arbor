/**
 * SQL completion from the connected database — tables after `FROM`/`JOIN`, columns after `alias.`
 * and, elsewhere, the columns of the tables the statement already names.
 *
 * Two hosts, one core: a `.sql` file (the whole buffer is SQL) and a Java string that is a native
 * query (`createNativeQuery("…")`, `@NamedNativeQuery`, `@Query(…, nativeQuery = true)`). The core
 * ({@link sqlSuggestions}) is pure text-and-schema; the CodeMirror sources below only find the SQL
 * and hand it over.
 *
 * It answers only when a Picus connection is chosen and open — there is nothing to complete from
 * otherwise, and a source that returns `null` costs the editor nothing. Names are offered as the
 * schema spells them; whether a project's casing is quoted is not guessed at.
 */

import type { Completion, CompletionContext, CompletionResult, CompletionSource } from '@codemirror/autocomplete';
import type { InlineCompletionSource } from '$lib/components/shared/ui/code-editor';
// Picus's abbreviation language (`s#orders(id)[id=1]`) — the expander and its editor glue live there,
// and are reused rather than written again: the schema- and type-aware part is the point of it.
import { createAbbrevIntel } from '$lib/components/picus/sql-intel/abbrev';
import { bennuPicusStore } from '$lib/stores/bennu/picus.svelte';
import { projectStore } from '$lib/stores/bennu/project.svelte';
import type { TableInfo } from '$lib/types/picus';

/** What the statement says about its tables: alias (or bare name) → table, in the order written. */
export function referencedTables(sql: string, tables: TableInfo[]): Map<string, TableInfo> {
  const byName = new Map(tables.map((t) => [bare(t.name), t]));
  const out = new Map<string, TableInfo>();
  const re = /\b(?:from|join|update|into)\s+((?:"?[\w$]+"?\.)?"?[\w$]+"?)(?:\s+(?:as\s+)?("?[\w$]+"?))?/gi;
  for (const m of sql.matchAll(re)) {
    const table = byName.get(bare(m[1]));
    if (!table) continue;
    out.set(bare(m[1]), table);
    const alias = m[2] ? bare(m[2]) : '';
    // A keyword after the table is not an alias (`FROM orders WHERE …`).
    if (alias && !KEYWORDS_AFTER_TABLE.has(alias)) out.set(alias, table);
  }
  return out;
}

const KEYWORDS_AFTER_TABLE = new Set([
  'where', 'on', 'join', 'inner', 'left', 'right', 'full', 'cross', 'natural', 'group', 'order', 'having',
  'limit', 'offset', 'union', 'set', 'values', 'using', 'fetch', 'for', 'select',
]);

function bare(name: string): string {
  return name.replace(/^.*\./, '').replace(/"/g, '').toLowerCase();
}

function columnOptions(table: TableInfo): Completion[] {
  return table.columns.map((c, i) => ({
    label: c.name,
    type: 'property',
    detail: c.type,
    info: c.primaryKey ? `${table.name} — primary key` : table.name,
    // Declaration order is how the table reads; keep it unless the user types.
    boost: -i / 1000,
  }));
}

function tableOptions(tables: TableInfo[]): Completion[] {
  return tables.map((t) => ({ label: t.name, type: t.kind === 'view' ? 'interface' : 'class', detail: t.kind }));
}

export interface SqlSuggestion {
  /** Characters before the caret to replace. */
  replace: number;
  options: Completion[];
}

/**
 * The completions for a caret after `lineBefore` (the current line up to the caret), inside the
 * statement `statement` (all of it, for alias resolution), over `tables`. `null` when the position
 * is not one SQL has a name for.
 */
export function sqlSuggestions(
  lineBefore: string,
  statement: string,
  tables: TableInfo[],
  explicit: boolean,
): SqlSuggestion | null {
  if (tables.length === 0) return null;

  // `alias.` — the columns of the table the alias stands for.
  const dotted = /("?[\w$]+"?)\.("?[\w$]*)$/.exec(lineBefore);
  if (dotted) {
    const table = referencedTables(statement, tables).get(bare(dotted[1]))
      ?? tables.find((t) => bare(t.name) === bare(dotted[1]));
    return table ? { replace: dotted[2].length, options: columnOptions(table) } : null;
  }

  const word = /[\w$]*$/.exec(lineBefore)![0];

  // A table is expected right after the keyword that introduces one.
  if (/\b(?:from|join|into|update|table)\s+[\w$]*$/i.test(lineBefore)) {
    return { replace: word.length, options: tableOptions(tables) };
  }

  // Anywhere else a name is only worth offering once the user has started one (or asked).
  if (!word && !explicit) return null;
  const referenced = [...new Set(referencedTables(statement, tables).values())];
  const options = referenced.flatMap(columnOptions);
  return options.length ? { replace: word.length, options } : null;
}

/** The schema to complete from, loading it the first time it is needed. `[]` when there is none. */
async function schemaTables(): Promise<TableInfo[]> {
  if (!bennuPicusStore.connected) return [];
  const root = projectStore.project?.root;
  if (!root) return [];
  await bennuPicusStore.ensureSchema();
  return bennuPicusStore.tables;
}

function resultFor(ctx: CompletionContext, s: SqlSuggestion | null): CompletionResult | null {
  if (!s) return null;
  return { from: ctx.pos - s.replace, options: s.options, validFor: /^[\w$"]*$/ };
}

/** Picus's abbreviation completion and preview for the connection chosen in the Database window,
 *  or `null` without an open one. Built once per connection: the editor asks on every keystroke. */
const abbrevByConnection = new Map<string, ReturnType<typeof createAbbrevIntel>>();
function abbreviations(): ReturnType<typeof createAbbrevIntel> {
  const c = bennuPicusStore.selected;
  if (!c || !bennuPicusStore.connected) return null;
  const key = `${c.id}|${c.engine}`;
  let intel = abbrevByConnection.get(key);
  if (intel === undefined) {
    intel = createAbbrevIntel(c.engine, c.id);
    abbrevByConnection.set(key, intel);
  }
  return intel;
}

/** Ghost text for an abbreviation: the SQL it expands to, Tab to accept. */
export function sqlAbbreviationPreview(): InlineCompletionSource {
  return (view, pos) => abbreviations()?.inlineCompletion?.(view, pos) ?? null;
}

/** For a `.sql` file: the whole buffer is the statement. */
export function sqlFileCompletion(): CompletionSource {
  return async (ctx) => {
    const tables = await schemaTables();
    if (!tables.length) return null;
    // An abbreviation on this line is Picus's to complete; anything else falls through to names.
    const abbreviation = abbreviations()?.completion;
    if (abbreviation) {
      const answer = await abbreviation(ctx);
      if (answer) return answer;
    }
    const line = ctx.state.doc.lineAt(ctx.pos);
    // The statement is the run between semicolons around the caret.
    const text = ctx.state.doc.toString();
    const start = text.lastIndexOf(';', ctx.pos - 1) + 1;
    const end = text.indexOf(';', ctx.pos);
    const statement = text.slice(start, end < 0 ? text.length : end);
    return resultFor(ctx, sqlSuggestions(line.text.slice(0, ctx.pos - line.from), statement, tables, ctx.explicit));
  };
}

const NATIVE_CALL = /createNativeQuery\s*\(|createSQLQuery\s*\(|@NamedNativeQuery\b/;

/** IntelliJ's own way of saying a string is SQL: `// language=sql`, `/* language=PostgreSQL *\/`, `@Language("SQL")`. */
const SQL_LANGUAGE_HINT = /(?:\/\/|\/\*)\s*language\s*=\s*"?\w*(?:sql|oracle)\b|@Language\s*\(\s*(?:value\s*=\s*)?"\w*sql"/i;

/** The SQL of the native query the caret is inside, or `null` when it is in no such string. */
export function nativeQueryAt(text: string, pos: number): string | null {
  const before = text.slice(Math.max(0, pos - 1500), pos);
  const after = text.slice(pos, pos + 800);

  // The caret has to be inside a string literal: an odd number of unescaped quotes on its line.
  const lineBefore = before.slice(before.lastIndexOf('\n') + 1);
  if ((lineBefore.replace(/\\./g, '').match(/"/g)?.length ?? 0) % 2 === 0) return null;

  // The call or annotation the literal belongs to starts after the last statement boundary.
  const boundary = Math.max(before.lastIndexOf(';'), before.lastIndexOf('}'));
  const head = before.slice(boundary + 1);
  const tail = after.slice(0, Math.max(0, after.search(/[;{]/) < 0 ? after.length : after.search(/[;{]/)));
  const isNative =
    NATIVE_CALL.test(head) || SQL_LANGUAGE_HINT.test(head) ||
    (/@Query\s*\(/.test(head) && /nativeQuery\s*=\s*true/.test(head + tail));
  if (!isNative) return null;

  return [...(head + tail).matchAll(/"((?:[^"\\]|\\.)*)"/g)].map((m) => m[1]).join(' ');
}

/** For Java: SQL completion inside a native query's string, `inner` (the Java source) everywhere else. */
export function nativeQueryCompletion(inner: CompletionSource): CompletionSource {
  return async (ctx) => {
    const statement = nativeQueryAt(ctx.state.doc.toString(), ctx.pos);
    if (statement === null) return inner(ctx);
    const tables = await schemaTables();
    if (!tables.length) return null;
    const line = ctx.state.doc.lineAt(ctx.pos);
    // Only the text of the current literal counts as "before the caret" for the keyword test.
    const lineBefore = line.text.slice(0, ctx.pos - line.from);
    const literalBefore = lineBefore.slice(lineBefore.lastIndexOf('"') + 1);
    return resultFor(ctx, sqlSuggestions(literalBefore, statement, tables, ctx.explicit));
  };
}

/** A statement element whose text the caret is in. */
const STATEMENT_OPEN = /<(select|insert|update|delete|sql)\b[^>]*>/gi;

/**
 * The SQL of the MyBatis statement the caret is in the *text* of, or `null` — in a tag, in an
 * attribute, inside a `#{…}` / `${…}` binding, or outside any statement. Those places belong to the
 * XML and MyBatis completion; a table name offered inside `test="…"` would be noise.
 *
 * Dynamic tags (`<if>`, `<where>`, …) are looked through: what they wrap is still SQL, and the
 * statement the caret is in is the nearest enclosing select / insert / update / delete / sql.
 */
export function mapperSqlAt(text: string, pos: number): { statement: string; lineBefore: string } | null {
  if (!text.includes('<mapper')) return null;
  let open: RegExpMatchArray | null = null;
  for (const m of text.slice(0, pos).matchAll(STATEMENT_OPEN)) open = m;
  if (!open || open.index === undefined) return null;
  const bodyStart = open.index + open[0].length;
  if (bodyStart > pos) return null;

  const kind = open[1].toLowerCase();
  const close = text.toLowerCase().indexOf(`</${kind}`, bodyStart);
  if (close !== -1 && close < pos) return null;

  const before = text.slice(bodyStart, pos);
  if (before.lastIndexOf('<') > before.lastIndexOf('>')) return null;
  const line = before.slice(before.lastIndexOf('\n') + 1);
  if (/[#$]\{[^}]*$/.test(line)) return null;

  const body = text
    .slice(bodyStart, close === -1 ? text.length : close)
    .replace(/<!\[CDATA\[|\]\]>/g, ' ')
    .replace(/<[^>]*>/g, ' ')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&');
  // Whatever follows the last tag on the line is what is being typed.
  const lineBefore = line.slice(line.lastIndexOf('>') + 1);
  return { statement: body, lineBefore };
}

/** For a MyBatis mapper (`.xml`): SQL completion in a statement's text, `inner` (the XML source) elsewhere. */
export function mapperCompletion(inner: CompletionSource): CompletionSource {
  return async (ctx) => {
    const hit = mapperSqlAt(ctx.state.doc.toString(), ctx.pos);
    if (hit === null) return inner(ctx);
    const tables = await schemaTables();
    if (!tables.length) return inner(ctx);
    return resultFor(ctx, sqlSuggestions(hit.lineBefore, hit.statement, tables, ctx.explicit)) ?? inner(ctx);
  };
}

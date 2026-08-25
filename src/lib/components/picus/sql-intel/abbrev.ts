/**
 * Emmet for SQL — a shorthand line that expands into the statement it stands for.
 *
 * ```
 * s#localstrings(keycode,value)[keycode='ita']
 *   -> SELECT KEYCODE, VALUE FROM LOCALSTRINGS WHERE KEYCODE = 'ita'
 * ```
 *
 * This module belongs to the same rule as the rest of the folder — **every
 * proposal is a fact, never a prediction** — and it is arguably the clearest case
 * of it. The expansion is not a template with the words filled in: the column's
 * *type* decides whether its value is quoted, and a `>` join reads its `ON` out of
 * the **foreign key**, refusing outright when two of them would fit. A snippet
 * engine can do neither, which is the entire reason this exists rather than a
 * snippet file.
 *
 * ## Nothing here decides what an abbreviation is
 *
 * That rule lives in the Rust parser (`arbor-sql-abbrev`), and a copy of it in
 * TypeScript would drift the first time the grammar grew. So `picus_expand_sql` is
 * asked, and it answers for ordinary SQL too — `isAbbreviation: false` and nothing
 * else. Everything this module does, including telling the SQL linter to keep
 * quiet, is downstream of that one answer.
 *
 * The same call returns the expansion **and** the cursor context, which is why
 * completion cannot end up offering the columns of a table the preview does not
 * think is there: there is only ever one parse behind both.
 *
 * ## What is shown, and what Tab does
 *
 * The preview is ghost text — an arrow, then the SQL — and Tab **replaces** the
 * abbreviation with it. The arrow is there because the two are not a continuation:
 * without it the line would read as `s#loc... SELECT ...` and look like an append.
 * `insert` carries the bare SQL, so what is written is never the rendering.
 *
 * ## What is offered
 *
 * Tables after `#` and after `>`, columns inside `(...)` and `[...]`, the join
 * column after `>t:`, the operators, the verbs. **Never a value** — what to compare
 * a column to is the user's data, and a list of guesses at it is exactly the popup
 * people switch off.
 */

import type {
  Completion,
  CompletionContext,
  CompletionResult,
  CompletionSource,
} from '@codemirror/autocomplete';
import type { EditorView } from '@codemirror/view';
import { SvelteMap } from 'svelte/reactivity';
import type {
  CodeEditorIntel,
  InlineCompletion,
  InlineCompletionSource,
} from '$lib/components/shared/ui/code-editor';
import { makeU16ToByte } from '$lib/components/shared/ui/code-editor';
import { expandSql, type CursorContext, type Expansion } from '$lib/ipc/picus/abbrev';
import type { Dialect, TableInfo } from '$lib/types/picus';
import { ensureRelationDetail, schemaViewFor, type SchemaView } from './schema-view';

/** Word under construction, as `completion.ts` defines it — Oracle allows `$` / `#`. */
const VALID_FOR = /^[A-Za-z0-9_$#]*$/;

/**
 * How long a request waits before it is sent, so a burst of keystrokes costs one
 * round trip rather than one per character. A request superseded inside the window
 * is never sent at all.
 */
const DEBOUNCE_MS = 60;

/** Bounds on both caches. Small on purpose: they exist to spare round trips over a
 *  handful of lines somebody is editing, not to remember a session. */
const MAX_ANSWERS = 120;
const MAX_VERDICTS = 200;

/**
 * The one place a caret offset changes coordinate system.
 *
 * CodeMirror counts in **UTF-16 code units**; `picus_expand_sql` takes a **UTF-8
 * byte** offset. The two agree only while the text is ASCII, and these are Italian
 * install scripts — one accented character in a value and every offset after it is
 * wrong, which shows up as completion offering the wrong thing near the end of a
 * line and nowhere else. Converted in one named function so there is one place to
 * be right.
 */
function byteOffsetIn(line: string, u16: number): number {
  return makeU16ToByte(line)(u16);
}

// ── The caches ────────────────────────────────────────────────────────────────

/** What the backend said about a line it called an abbreviation. */
interface LineVerdict {
  /** The refusal, when it is an abbreviation that will not expand. */
  error?: string;
}

/**
 * The lines the backend has called abbreviations, keyed by **connection and exact
 * line text**.
 *
 * Reactive (a `SvelteMap`) because `sqlDiagnostics` is a pure function called from
 * a Svelte `$derived`: the answer arrives after the derived has already run, and
 * without a reactive dependency the linter's squiggles would sit on an abbreviation
 * until the next keystroke happened to re-run it.
 *
 * **Only abbreviations go in.** A buffer of ordinary SQL then never writes here at
 * all, and so never invalidates the diagnostics either — the alternative is a full
 * re-lint of the file every time the caret settles on a line that turned out to be
 * nothing special.
 *
 * Keyed by the line text and nothing else, so two identical lines share one answer:
 * the answer depends on the text and the schema, and on nothing about where the
 * line sits.
 */
const verdicts = new SvelteMap<string, LineVerdict>();

/** Full answers, keyed by connection, caret and line — `context` depends on all three. */
const answers = new Map<string, Expansion>();

/** Requests already on the wire, so two callers asking the same thing ask once. */
const inflight = new Map<string, Promise<Expansion | null>>();

/** The most recent question. Anything else still inside the debounce window is
 *  stale by definition and is dropped rather than sent. */
let latestKey = '';

/** Key separator. A newline, because a *line* cannot contain one and neither can a
 *  connection id — so a key splits back apart exactly the way it went together. */
const SEP = '\n';

function verdictKey(connectionId: string, line: string): string {
  return `${connectionId}${SEP}${line}`;
}

/** Insert, evicting the oldest first. `Map` iterates in insertion order, so the
 *  first key is the least recently *added* — good enough for a working set. */
function put<K, V>(map: Map<K, V>, key: K, value: V, max: number) {
  map.set(key, value);
  while (map.size > max) {
    const oldest = map.keys().next();
    if (oldest.done) break;
    map.delete(oldest.value);
  }
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Remember an answer.
 *
 * The reactive half is written only when it **changes**, and only for lines that
 * are abbreviations: every write there re-runs the buffer's diagnostics, so an
 * unconditional one would re-lint the file on every settled caret.
 */
function remember(connectionId: string, line: string, answer: Expansion) {
  if (!answer.isAbbreviation) return;
  const key = verdictKey(connectionId, line);
  const previous = verdicts.get(key);
  if (previous && previous.error === answer.error) return;
  put(verdicts, key, { error: answer.error }, MAX_VERDICTS);
}

/**
 * Ask what this line expands to, and what is under the caret in it.
 *
 * Debounced, deduplicated and race-guarded: a slow answer can only ever land in
 * the cache entry it was asked for, and a request the user has already typed past
 * is dropped before it is sent. Callers get `null` when their question was
 * superseded — which is not an error, it is "ask again when the caret settles".
 */
async function requestExpansion(
  connectionId: string,
  dialect: Dialect,
  line: string,
  caretU16: number,
): Promise<Expansion | null> {
  const key = `${connectionId}${SEP}${caretU16}${SEP}${line}`;
  const cached = answers.get(key);
  if (cached) return cached;
  const running = inflight.get(key);
  if (running) return running;

  latestKey = key;
  const call = (async (): Promise<Expansion | null> => {
    try {
      await sleep(DEBOUNCE_MS);
      if (latestKey !== key) return null;
      const answer = await expandSql(connectionId, line, byteOffsetIn(line, caretU16), dialect);
      put(answers, key, answer, MAX_ANSWERS);
      remember(connectionId, line, answer);
      return answer;
    } catch {
      // `picus-be` down, or the connection closed under us. No answer is a valid
      // answer here: no preview, and the SQL linter keeps whatever opinion it had.
      return null;
    } finally {
      inflight.delete(key);
    }
  })();
  inflight.set(key, call);
  return call;
}

// ── What the linter needs to know ─────────────────────────────────────────────

/** One line of the buffer the backend has said is an abbreviation. */
export interface AbbreviationLine {
  /** UTF-16 offsets of the abbreviation itself — the line minus its indentation. */
  from: number;
  to: number;
  /** The refusal, when there is one. */
  error?: string;
}

/**
 * The lines of `text` this connection's backend has called abbreviations.
 *
 * Driven from the **cache**, not from a shape test. The cache is the only place the
 * Rust parser's answer lives on this side, and re-deriving "does this look like an
 * abbreviation?" in TypeScript is the one thing that would guarantee the editor and
 * the expander eventually disagree about the same line.
 *
 * So this walks the handful of remembered lines and finds where they occur, rather
 * than asking about every line in the buffer — which also keeps a 5 000-line script
 * from taking 5 000 reactive reads per keystroke. A line the caret has never
 * visited is a line nobody is typing an abbreviation on; and a line whose answer has
 * not arrived yet keeps the SQL linter's squiggles for a moment and then loses them,
 * which is self-correcting and much better than a rule that can rot.
 */
export function abbreviationLines(
  connectionId: string | undefined,
  text: string,
): AbbreviationLine[] {
  if (!connectionId || !text) return [];

  const prefix = `${connectionId}${SEP}`;
  const known = new Map<string, LineVerdict>();
  for (const [key, verdict] of verdicts) {
    if (key.startsWith(prefix)) known.set(key.slice(prefix.length), verdict);
  }
  if (known.size === 0) return [];

  const out: AbbreviationLine[] = [];
  let at = 0;
  for (const line of text.split('\n')) {
    const verdict = known.get(line);
    if (verdict) {
      const indent = line.length - line.trimStart().length;
      out.push({ from: at + indent, to: at + line.trimEnd().length, error: verdict.error });
    }
    at += line.length + 1;
  }
  return out;
}

// ── Candidates ────────────────────────────────────────────────────────────────

/**
 * The comparison operators, as the grammar spells them.
 *
 * The one vocabulary duplicated from `arbor_sql_abbrev` (`Operator::SYMBOLS`),
 * because it does not cross the wire — `CursorContext` says *that* an operator goes
 * here, never which ones exist. Seven fixed symbols with no reason to change, and
 * `~` in particular is worth offering: nobody guesses that `LIKE` is spelled that
 * way.
 */
const OPERATORS: Completion[] = [
  { label: '=', type: 'operator', detail: 'equals' },
  { label: '<>', type: 'operator', detail: 'not equal' },
  { label: '<', type: 'operator', detail: 'less than' },
  { label: '<=', type: 'operator', detail: 'less than or equal' },
  { label: '>', type: 'operator', detail: 'greater than' },
  { label: '>=', type: 'operator', detail: 'greater than or equal' },
  { label: '~', type: 'operator', detail: 'LIKE' },
];

/**
 * The verbs, spelled the short way — the point of the whole language.
 *
 * `info` rather than a bare letter list: `m`, `a` and `fc` are the three nobody
 * would guess at, and a completion popup is the only place they are ever going to
 * be discovered. The example is what makes the entry teach something.
 */
const VERBS: Completion[] = [
  { label: 's', type: 'keyword', detail: 'SELECT', info: 's#ordini(codice)[id=7]' },
  { label: 'i', type: 'keyword', detail: 'INSERT', info: "i#ordini(id,codice)*3{$, 'COD_$'}" },
  { label: 'u', type: 'keyword', detail: 'UPDATE', info: 'u#ordini(evaso=true)[id=7]' },
  { label: 'd', type: 'keyword', detail: 'DELETE', info: 'd#ordini[id=7]' },
  { label: 'm', type: 'keyword', detail: 'MERGE (upsert)', info: 'm#ordini[id]' },
  { label: 'a', type: 'keyword', detail: 'ALTER TABLE', info: 'a#ordini+nota:varchar(200)' },
  { label: 'fc', type: 'keyword', detail: 'FOR loop over a cursor', info: "fc#ordini[stato='EV']" },
];

function ci(a: string, b: string): boolean {
  return a.toUpperCase() === b.toUpperCase();
}

function relationOption(rel: TableInfo, boost = 0): Completion {
  return {
    label: rel.name,
    type: rel.kind === 'view' ? 'interface' : 'class',
    detail: rel.kind,
    info: rel.columns.length ? `${rel.columns.length} columns` : undefined,
    boost,
  };
}

/** The columns of every table named in the context, deduplicated by name. */
function columnOptions(view: SchemaView, tables: string[]): Completion[] {
  const out: Completion[] = [];
  const seen = new Set<string>();
  for (const name of tables) {
    const rel = view.relation(name);
    if (!rel) continue;
    for (const col of rel.columns) {
      const key = col.name.toUpperCase();
      if (seen.has(key)) continue;
      seen.add(key);
      out.push({ label: col.name, type: 'property', detail: col.type, info: rel.name });
    }
  }
  return out;
}

/**
 * Tables to join to, with the ones actually related to `from` first.
 *
 * Everything is still offered and the ordering is only a hint: the *decision* is
 * the backend's, which refuses a `>` between two tables no foreign key connects.
 * The reverse direction — tables that point **at** `from` — is best-effort, because
 * a relation whose detail has not been read carries no constraints, and reading
 * every one of them to sort a list would be an expensive way to be tidy.
 */
async function joinTableOptions(
  connectionId: string,
  view: SchemaView,
  from: string,
): Promise<Completion[]> {
  const source = (await ensureRelationDetail(connectionId, from)) ?? view.relation(from);
  const related = new Set<string>();
  if (source) {
    for (const fk of source.foreignKeys ?? []) related.add(fk.referencedTable.toUpperCase());
    for (const rel of view.relations) {
      if ((rel.foreignKeys ?? []).some((fk) => ci(fk.referencedTable, source.name))) {
        related.add(rel.name.toUpperCase());
      }
    }
  }
  return view.relations.map((rel) =>
    relationOption(rel, related.has(rel.name.toUpperCase()) ? 1 : 0),
  );
}

/**
 * After `>table:` — the column that tells two foreign keys apart.
 *
 * The label is the **child** column, which is what the language reads a `:` as, and
 * the detail spells the whole key out, so picking one is a decision made with the
 * constraint in front of you rather than from memory.
 */
async function joinColumnOptions(
  connectionId: string,
  view: SchemaView,
  from: string,
  to: string,
): Promise<Completion[]> {
  const left = (await ensureRelationDetail(connectionId, from)) ?? view.relation(from);
  const right = (await ensureRelationDetail(connectionId, to)) ?? view.relation(to);
  if (!left || !right) return [];

  const out: Completion[] = [];
  const collect = (child: TableInfo, parent: TableInfo) => {
    for (const fk of child.foreignKeys ?? []) {
      if (!ci(fk.referencedTable, parent.name) || fk.columns.length === 0) continue;
      out.push({
        label: fk.columns[0],
        type: 'property',
        detail: `${child.name}.${fk.columns.join(', ')} = ${parent.name}.${fk.referencedColumns.join(', ')}`,
      });
    }
  };
  collect(left, right);
  if (!ci(left.name, right.name)) collect(right, left);
  return out;
}

/**
 * What is worth offering where the caret is.
 *
 * `null` for the three value contexts and for `none`. A value is the user's data:
 * offering a list there would be the tool guessing at what they meant to compare
 * against, which is the failure this whole folder is shaped to avoid.
 */
async function candidatesFor(
  context: CursorContext,
  connectionId: string,
): Promise<Completion[] | null> {
  const view = schemaViewFor(connectionId);
  switch (context.at) {
    case 'verb':
      return VERBS;
    case 'table':
      return view.relations.map((rel) => relationOption(rel));
    case 'joinTable':
      return joinTableOptions(connectionId, view, context.from);
    case 'joinColumn':
      return joinColumnOptions(connectionId, view, context.from, context.to);
    case 'column':
    case 'predicateColumn':
      return columnOptions(view, context.tables);
    case 'predicateOperator':
      return OPERATORS;
    default:
      return null;
  }
}

/** How much of the identifier under the caret a completion replaces. */
function prefixOf(context: CursorContext): string {
  return 'prefix' in context ? context.prefix : '';
}

/**
 * Is there anything left for the popup to offer?
 *
 * `false` once what has been typed **is** one of the options and nothing longer
 * starts the same way. The popup then has one entry, identical to the text under
 * it, and accepting it would write what is already there.
 *
 * That is not harmless, and it is the reason this exists. An open popup **owns
 * Tab** — which is the key that expands the abbreviation — and while it is open the
 * ghost preview stands down entirely. So a fully typed `s#v_ws_elenchi` showed no
 * expansion at all: the first Tab accepted a completion that changed nothing, and
 * only the edit *that* dispatched let the preview through. It read as the feature
 * being broken and then spontaneously working.
 *
 * `ORDINI` alongside `ORDINI_DETTAGLIO` still offers: the name is complete but the
 * list is not, and closing there would take the longer one away.
 */
function nothingLeftToOffer(options: Completion[], typed: string): boolean {
  if (!typed) return false;
  const upper = typed.toUpperCase();
  let exact = false;
  for (const option of options) {
    const label = option.label.toUpperCase();
    if (label === upper) exact = true;
    // Something longer to reach — keep offering, whatever else matched.
    else if (label.startsWith(upper)) return false;
  }
  return exact;
}

// ── The two sources ───────────────────────────────────────────────────────────

/**
 * Nothing but closing brackets and whitespace between the caret and the end of its
 * line.
 *
 * The preview replaces the **whole line**, so it may only be offered from the end
 * of one: shown from the middle it would overwrite text the caret is sitting in
 * front of, which is the one thing an accept must never surprise anybody with.
 *
 * Closers count as nothing for the same reason `ghost.ts` allows them — bracket
 * auto-closing puts a `)` there the instant you type `(`, which is exactly where
 * the first useful preview appears. They are part of the line the backend was asked
 * about, so they are part of what the accept replaces.
 */
function atEndOfLine(view: EditorView, pos: number): boolean {
  const line = view.state.doc.lineAt(pos);
  return /^[)\]\s]*$/.test(line.text.slice(pos - line.from));
}

function createAbbrevPreview(dialect: Dialect, connectionId: string): InlineCompletionSource {
  return async function abbrevPreview(
    view: EditorView,
    pos: number,
  ): Promise<InlineCompletion | null> {
    if (!atEndOfLine(view, pos)) return null;
    const line = view.state.doc.lineAt(pos);
    const answer = await requestExpansion(connectionId, dialect, line.text, pos - line.from);
    if (!answer?.isAbbreviation || !answer.sql) return null;

    // Indentation survives: an abbreviation inside an indented block expands in
    // place, not against the left margin.
    const indent = line.text.length - line.text.trimStart().length;
    return {
      text: ` → ${answer.sql}`,
      insert: answer.sql,
      replace: { from: line.from + indent, to: line.to },
    };
  };
}

function createAbbrevCompletion(dialect: Dialect, connectionId: string): CompletionSource {
  return async function abbrevCompletion(ctx: CompletionContext): Promise<CompletionResult | null> {
    const line = ctx.state.doc.lineAt(ctx.pos);
    const answer = await requestExpansion(connectionId, dialect, line.text, ctx.pos - line.from);
    // Not an abbreviation — or the question was superseded, in which case the SQL
    // completion behind this one is still a reasonable answer for the line.
    if (!answer?.isAbbreviation) return null;

    const options = await candidatesFor(answer.context, connectionId);
    if (!options || options.length === 0) return null;
    const prefix = prefixOf(answer.context);
    if (nothingLeftToOffer(options, prefix)) return null;
    return {
      from: ctx.pos - prefix.length,
      options,
      // A predicate rather than the bare pattern, and the difference is load-bearing.
      // `validFor` is what stops CodeMirror re-asking the source while a word is
      // being typed — which also stopped it ever re-deciding whether the popup
      // should still be there. With the pattern alone, typing the last character of
      // a table name left the popup open over a complete name with nothing to add,
      // holding Tab hostage. Now that keystroke invalidates the result, the source
      // runs once more, and the refusal above closes it.
      validFor: (text: string) => VALID_FOR.test(text) && !nothingLeftToOffer(options, text),
    };
  };
}

/**
 * The abbreviation half of the editor's intelligence, or `null`.
 *
 * `null` without a connection, and that single gate is the whole rule: no
 * connection means no schema, and with no schema there is no type to decide a quote
 * and no foreign key to decide a join — which leaves a snippet expander with none
 * of the reasons this one exists.
 */
export function createAbbrevIntel(
  dialect: Dialect,
  connectionId?: string,
): Pick<CodeEditorIntel, 'completion' | 'inlineCompletion'> | null {
  if (!connectionId) return null;
  return {
    completion: createAbbrevCompletion(dialect, connectionId),
    inlineCompletion: createAbbrevPreview(dialect, connectionId),
  };
}

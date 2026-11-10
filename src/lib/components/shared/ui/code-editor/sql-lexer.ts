/**
 * The one SQL tokenizer — a CodeMirror {@link StreamParser} with no Arbor concept in it.
 *
 * Three consumers colour SQL and used to do it three ways: Picus (a whole `.sql` buffer), Bennu's
 * `.sql` files, and Bennu's SQL / JPQL / HQL *inside Java strings*. They now share this lexer, so a
 * rule fixed here is fixed everywhere:
 *
 * - the editor installs it through `StreamLanguage.define` (see `sql-modes.ts`);
 * - a fragment (a query in a string) is driven by a plain loop over the same parser (see
 *   `sqlSpans` in `sql-spans.ts`).
 *
 * Token names are the legacy-mode vocabulary the rest of the editor already understands
 * (`keyword`, `type`, `string`, …) plus `callee` (a function call) and `property` (a name after a
 * dot), mapped to Lezer tags through {@link TOKEN_TABLE} so the same theme colours them.
 *
 * ## What it knows that the stock `@codemirror/legacy-modes` SQL mode does not
 *
 * - **Position decides keyword-ness.** A word after `from` / `join` / `update` / `into` / `table` is
 *   the project's own vocabulary unless it is a word that can never name anything (`select`, `where`
 *   …). So a table called `order`, `group` or `user` is not coloured as the keyword it collides with.
 * - **Calls and types are different things from keywords**: `count(…)`, `varchar(20)`, `::jsonb`.
 * - **A name after a dot is a property** (`o.total`), whatever the word is.
 * - **Parameters**: `:name`, `:1`, `$1`, `?`, `?1`, and Oracle's `&var` / `&&var`.
 * - **Strings the way each engine writes them**: `''` doubling, PostgreSQL's `E'…\n…'` and `$tag$…$tag$`,
 *   Oracle's `q'[…]'`, and no backslash escape anywhere else.
 * - **Comments**: `--`, C-style block comments (nested on PostgreSQL), and the optimizer hint that
 *   opens with `/*+`, which is code, not commentary.
 *
 * Dollar-quoted bodies are *not* swallowed as a string: a `DO $$ … $$` or function body is code, and
 * colouring it as one string made every PL/pgSQL script read as a single green block.
 */

import { StringStream, type StreamParser } from '@codemirror/language';
import { tags as t } from '@lezer/highlight';
import {
  AMBIGUOUS_TYPES, ATOMS, CALLABLE_KEYWORDS, NAME_MODIFIERS, RESERVED, vocabularyOf, type SqlVocabularyDialect,
} from './sql-vocabulary';

/** Words after which the next word is a name the project chose. */
const NAME_INTRODUCERS = new Set(['from', 'join', 'update', 'into', 'table', 'view', 'references']);
/** After `new`, JPQL names a class (`select new com.acme.Dto(...)`). */
const TYPE_INTRODUCERS = new Set(['new']);

const WORD = /^[\p{L}_][\p{L}\p{N}_$#]*/u;
const NUMBER = /^(?:0[xX][0-9a-fA-F]+|(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?[fFdDlL]?)/;
const NAMED_PARAM = /^:[\p{L}_][\p{L}\p{N}_]*/u;
const NUMBERED_PARAM = /^[:$?]\d+/;
const SUBSTITUTION = /^&&?(?:[\p{L}_][\p{L}\p{N}_]*|\d+)/u;
const DOLLAR_TAG = /^\$(?:[\p{L}_][\p{L}\p{N}_]*)?\$/u;
const Q_QUOTE = /^[nN]?[qQ]'(.)/;
const STRING_PREFIX = /^(?:[eE]|[nN]|[bB]|[xX]|[uU]&)'/;
const QUOTE_CLOSER: Record<string, string> = { '[': ']', '(': ')', '{': '}', '<': '>' };

export interface SqlState {
  /** Block-comment nesting depth (0 = outside). */
  comment: number;
  /** The open block comment is an optimizer hint (`/*+`), which is code. */
  hint: boolean;
  /** The quote character of a literal / quoted identifier still open at the end of the line. */
  quote: '' | "'" | '"' | '`';
  /** The open literal honours `\` escapes (`E'…'` on PostgreSQL). */
  escapes: boolean;
  /** An Oracle `q'…'` literal is open, with this closing character. */
  qClose: string;
  /** The `$tag$` of the dollar-quoted body the cursor is inside, or ''. */
  dollar: string;
  /** The next word is a name (table, entity) rather than part of the language. */
  nameMode: boolean;
  /** …and the name is a type (`new com.acme.Dto`). */
  nameIsType: boolean;
  /** The previous token was `.`. */
  afterDot: boolean;
  /** The previous token was `::`. */
  afterCast: boolean;
  /** The previous word, lower-cased, when the previous token was one (`for` before `update`). */
  prev: string;
  /** The previous token was a word — tells `name char(10)` (a type) from `char(65)` (a call). */
  afterWord: boolean;
}

/** Lezer tags for the token names the theme has no default for. */
const TOKEN_TABLE = {
  callee: t.function(t.variableName),
  property: t.propertyName,
  punctuation: t.punctuation,
  bracket: t.bracket,
  type: t.typeName,
  atom: t.atom,
} as const;

/** What a word is, given where it stands. Pure — the one place the rules above are decided. */
export function classifyWord(
  word: string,
  at: { afterDot: boolean; afterCast: boolean; afterWord: boolean; nameMode: boolean; nameIsType: boolean; call: boolean },
  dialect: SqlVocabularyDialect,
): 'keyword' | 'type' | 'callee' | 'atom' | 'property' | 'variable' {
  const v = vocabularyOf(dialect);
  const lower = word.toLowerCase();
  if (at.afterDot) {
    if (at.nameMode) return at.nameIsType ? 'type' : 'variable';
    return at.call ? 'callee' : 'property';
  }
  if (at.afterCast) return 'type';
  if (at.nameMode && !RESERVED.has(lower)) return at.nameIsType ? 'type' : 'variable';
  if (ATOMS.has(lower) && !at.call) return 'atom';
  if (at.call) {
    if (v.functions.has(lower) || CALLABLE_KEYWORDS.has(lower)) return 'callee';
    if (v.types.has(lower)) return AMBIGUOUS_TYPES.has(lower) && !at.afterWord ? 'callee' : 'type';
    return v.keywords.has(lower) ? 'keyword' : 'callee';
  }
  if (v.types.has(lower)) return 'type';
  if (v.keywords.has(lower)) return 'keyword';
  return 'variable';
}

function startState(): SqlState {
  return {
    comment: 0, hint: false, quote: '', escapes: false, qClose: '', dollar: '',
    nameMode: false, nameIsType: false, afterDot: false, afterCast: false, afterWord: false, prev: '',
  };
}

/** Consume a block comment (or hint) up to its end or the end of the line. */
function blockComment(stream: StringStream, s: SqlState, nested: boolean): string {
  const token = s.hint ? 'meta' : 'comment';
  while (!stream.eol()) {
    if (stream.match('*/')) {
      s.comment -= 1;
      if (s.comment === 0) { s.hint = false; break; }
    } else if (nested && stream.match('/*')) {
      s.comment += 1;
    } else {
      stream.next();
    }
  }
  return token;
}

/** Consume the rest of an open quoted run (literal or quoted identifier). */
function quotedRun(stream: StringStream, s: SqlState): void {
  const quote = s.quote;
  while (!stream.eol()) {
    const ch = stream.next();
    if (s.escapes && ch === '\\') { stream.next(); continue; }
    if (ch === quote) {
      if (stream.peek() === quote) { stream.next(); continue; } // doubled: a quote inside the literal
      s.quote = '';
      s.escapes = false;
      return;
    }
  }
}

/** Consume the rest of an Oracle `q'[…]'` literal. */
function qQuotedRun(stream: StringStream, s: SqlState): void {
  const close = s.qClose + "'";
  while (!stream.eol()) {
    if (stream.match(close)) { s.qClose = ''; return; }
    stream.next();
  }
}

/** Token for an opened quoted run: `string` for a literal, `property`/`variable-2` for an identifier. */
function quotedToken(s: SqlState, quote: string, wasDot: boolean): string {
  if (quote === "'") return 'string';
  return wasDot ? 'property' : 'variable-2';
}

const CALL_AHEAD = /^\s*\(/;

/** Build the stream parser for a dialect. */
export function createSqlStream(dialect: SqlVocabularyDialect): StreamParser<SqlState> {
  const oracleLike = dialect === 'oracle' || dialect === 'portable';
  const postgresLike = dialect === 'postgres' || dialect === 'portable';
  const nestedComments = dialect === 'postgres' || dialect === 'portable';
  const jpql = dialect === 'jpql';

  /** Remember the flags a token leaves behind for the next one. */
  const settle = (s: SqlState, flags: Partial<Pick<SqlState, 'afterDot' | 'afterCast' | 'afterWord' | 'prev'>> = {}) => {
    s.prev = flags.prev ?? '';
    s.afterDot = flags.afterDot ?? false;
    s.afterCast = flags.afterCast ?? false;
    s.afterWord = flags.afterWord ?? false;
  };

  return {
    name: 'sql',
    startState,
    tokenTable: TOKEN_TABLE,
    languageData: { commentTokens: { line: '--', block: { open: '/*', close: '*/' } } },

    token(stream, s) {
      // ── Constructs that can span lines ────────────────────────────────────────────────────
      if (s.comment > 0) return blockComment(stream, s, nestedComments);
      if (s.qClose) { qQuotedRun(stream, s); return 'string'; }
      if (s.quote) {
        const quote = s.quote;
        quotedRun(stream, s);
        return quotedToken(s, quote, false);
      }
      if (stream.eatSpace()) return null;

      const wasDot = s.afterDot;
      const wasCast = s.afterCast;
      const wasWord = s.afterWord;
      const wasPrev = s.prev;
      settle(s);

      // ── Comments ──────────────────────────────────────────────────────────────────────────
      if (stream.match('--')) { stream.skipToEnd(); return 'comment'; }
      if (stream.match('/*')) {
        s.comment = 1;
        s.hint = stream.eat('+') !== undefined && !jpql;
        return blockComment(stream, s, nestedComments);
      }

      // ── Literals and quoted identifiers ───────────────────────────────────────────────────
      if (oracleLike) {
        const q = stream.match(Q_QUOTE, false) as RegExpMatchArray | null;
        if (q) {
          stream.match(Q_QUOTE);
          s.qClose = QUOTE_CLOSER[q[1]] ?? q[1];
          qQuotedRun(stream, s);
          return 'string';
        }
      }
      const prefix = stream.match(STRING_PREFIX, false) as RegExpMatchArray | null;
      if (prefix) {
        stream.match(STRING_PREFIX);
        s.quote = "'";
        s.escapes = postgresLike && /^[eE]/.test(prefix[0]);
        quotedRun(stream, s);
        return 'string';
      }
      const ch = stream.peek() ?? '';
      if (ch === "'" || ch === '"' || ch === '`') {
        stream.next();
        s.quote = ch;
        s.escapes = false;
        quotedRun(stream, s);
        if (ch !== "'") {
          if (stream.peek() !== '.') { s.nameMode = false; s.nameIsType = false; }
          settle(s, { afterWord: true });
        }
        return quotedToken(s, ch, wasDot);
      }

      // ── Numbers ───────────────────────────────────────────────────────────────────────────
      if (/[0-9]/.test(ch) || (ch === '.' && /[0-9]/.test(stream.string.charAt(stream.pos + 1)))) {
        stream.match(NUMBER);
        return 'number';
      }

      // ── Parameters, casts and dollar quotes ───────────────────────────────────────────────
      if (stream.match('::')) { settle(s, { afterCast: true }); return 'operator'; }
      if (ch === ':' && (stream.match(NAMED_PARAM) || stream.match(NUMBERED_PARAM))) return 'meta';
      if (ch === '$') {
        if (stream.match(NUMBERED_PARAM)) return 'meta';
        const tag = stream.match(DOLLAR_TAG, true) as RegExpMatchArray | null;
        if (tag && postgresLike) {
          // The matching delimiter closes the body; any other `$tag$` seen inside it is just text.
          s.dollar = s.dollar === tag[0] ? '' : s.dollar || tag[0];
          return 'meta';
        }
        if (tag) return 'meta';
        stream.next();
        return 'operator';
      }
      if (ch === '?') {
        const after = stream.string.charAt(stream.pos + 1);
        if (postgresLike && (after === '|' || after === '&')) { stream.next(); stream.next(); return 'operator'; }
        stream.next();
        stream.match(/^\d+/);
        return 'meta';
      }
      if (oracleLike && ch === '&' && stream.match(SUBSTITUTION)) return 'meta';

      // ── Words ─────────────────────────────────────────────────────────────────────────────
      const word = stream.match(WORD) as RegExpMatchArray | null;
      if (word) {
        const text = word[0];
        const call = CALL_AHEAD.test(stream.string.slice(stream.pos));
        const lower = text.toLowerCase();
        const kind = classifyWord(text, {
          afterDot: wasDot, afterCast: wasCast, afterWord: wasWord,
          nameMode: s.nameMode, nameIsType: s.nameIsType, call,
        }, dialect);

        // A dotted name keeps the mode alive until its last segment has been read.
        const dotted = stream.peek() === '.';
        const keepsLooking = kind === 'keyword' && NAME_MODIFIERS.has(lower);
        if (s.nameMode && !keepsLooking && (!dotted || kind === 'keyword')) { s.nameMode = false; s.nameIsType = false; }
        if (!s.nameMode && !wasDot && kind === 'keyword') {
          // `for update`, `do update set`, `key update` are clauses, not the start of a table name.
          const clause = lower === 'update' && ['for', 'do', 'key'].includes(wasPrev);
          if (NAME_INTRODUCERS.has(lower) && !clause) s.nameMode = true;
          else if (TYPE_INTRODUCERS.has(lower)) { s.nameMode = true; s.nameIsType = true; }
        }
        settle(s, { afterWord: kind === 'variable' || kind === 'property', prev: lower });
        return kind;
      }

      // ── Operators and punctuation ─────────────────────────────────────────────────────────
      if (ch === '.') { stream.next(); settle(s, { afterDot: true }); return 'punctuation'; }
      if (ch === ',' || ch === ';') {
        stream.next();
        // A statement ends the pending name; a comma continues a `from a, b` list.
        if (ch === ';') { s.nameMode = false; s.nameIsType = false; }
        return 'punctuation';
      }
      if (ch === '(' || ch === ')' || ch === '[' || ch === ']') {
        stream.next();
        s.nameMode = false;
        return 'bracket';
      }
      // An operator run stops short of a comment opener, so `a--b` and `x/*c*/` still see theirs.
      let consumed = 0;
      for (;;) {
        const c = stream.peek();
        if (c === undefined || !/[+\-*/%<>=!~^|&@#]/.test(c)) break;
        const two = stream.string.substr(stream.pos, 2);
        if (consumed > 0 && (two === '--' || two === '/*')) break;
        stream.next();
        consumed++;
      }
      if (consumed > 0) return 'operator';
      stream.next(); // a character the language does not use: skipped, never an infinite loop
      return null;
    },
  };
}

const PARSERS = new Map<SqlVocabularyDialect, StreamParser<SqlState>>();

/** The parser for a dialect, built once. */
export function sqlStream(dialect: SqlVocabularyDialect): StreamParser<SqlState> {
  let parser = PARSERS.get(dialect);
  if (!parser) {
    parser = createSqlStream(dialect);
    PARSERS.set(dialect, parser);
  }
  return parser;
}

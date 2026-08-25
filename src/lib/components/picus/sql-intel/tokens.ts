/**
 * A lightweight SQL scanner — the layer everything else in `sql-intel/` reads.
 *
 * This is deliberately **not** a parser. The real parse belongs to `picus-parse`
 * (Tree-sitter) in the backend; what the editor needs while you type is something
 * that never blocks, never throws, and is right about the only thing that really
 * matters for intelligence: **where the text is code and where it is not**. A
 * scanner that gets strings, comments and dollar-quoting right can answer "what is
 * the word before the dot" and "which tables are in scope" well enough to be
 * useful, and is honest about the rest.
 *
 * **Where the statements are is the one question it no longer answers on its own.**
 * That is a parse — a semicolon inside a PL/SQL block ends nothing — and the parser
 * is in the backend, which the Run path was already asking. `scanSql` takes the
 * boundaries from `statementSpanStore` and groups its own tokens by them, falling
 * back to the local `;` split only until the answer for this exact buffer lands. One
 * question, one authority; the tokens stay here, where they are free.
 *
 * Dialect matters even at this level: PostgreSQL has `$tag$ … $tag$` bodies and
 * nested block comments, Oracle allows `$` and `#` inside identifiers. Getting
 * either wrong turns a whole procedure body into "code" or a whole file into "one
 * string", and every feature downstream inherits the mistake.
 *
 * Offsets are UTF-16 (CodeMirror's coordinate). The byte offsets diagnostics need
 * are produced at the very end, by the diagnostics module.
 */

import type { Dialect } from '$lib/types/picus';
import { statementSpanStore } from '$lib/stores/picus/statement-spans.svelte';

export type TokenKind =
  /** A bare identifier or keyword. `value` is upper-cased — SQL folds case. */
  | 'word'
  /** A `"delimited identifier"`. `value` is the text between the quotes, as written. */
  | 'quoted'
  | 'string'
  | 'number'
  | 'comment'
  /** `:name`, `?`, `$1` — a placeholder, never an identifier. */
  | 'param'
  | 'punct';

export interface SqlToken {
  kind: TokenKind;
  /** Raw source slice. */
  text: string;
  /** Comparable form: upper-case for `word`, inner text for `quoted` and `string`. */
  value: string;
  from: number;
  to: number;
}

/** A construct still open when the scan reached the end of the buffer — which is
 *  the normal state of a buffer being typed into, not an error. */
export interface OpenConstruct {
  kind: 'string' | 'comment' | 'dollar' | 'quoted';
  /** For `dollar`, the full tag (`$$`, `$body$`). Empty otherwise. */
  tag: string;
  /** Offset just after the opening delimiter — where the body starts. */
  bodyFrom: number;
}

export interface TokenScan {
  tokens: SqlToken[];
  open: OpenConstruct | null;
}

const WS = /\s/;
const DIGIT = /[0-9]/;
const IDENT_START = /[A-Za-z_]/;

/** Identifier continuation. Oracle admits `$` and `#`; in PostgreSQL a `$` after a
 *  word is far more likely to be the start of a dollar-quoted body, and reading it
 *  as part of the identifier would swallow the whole routine. */
function identBody(dialect: Dialect): RegExp {
  return dialect === 'oracle' ? /[A-Za-z0-9_$#]/ : /[A-Za-z0-9_]/;
}

/** String literal prefixes that attach to the opening quote (`N'x'`, `E'x'`, `B'01'`). */
const STRING_PREFIX = /^[NnEeBbXx]$/;

/**
 * Scan `src` into tokens.
 *
 * Never throws and never gives up: an unterminated string, comment or dollar body
 * ends the scan and is reported through {@link TokenScan.open}, because "the user
 * is in the middle of typing a block" is the single most common state of a live
 * buffer and the features that care (ghost text, most of all) need to know.
 */
export function tokenize(src: string, dialect: Dialect): TokenScan {
  const tokens: SqlToken[] = [];
  const n = src.length;
  const body = identBody(dialect);
  const dollarQuotes = dialect === 'postgres';
  let i = 0;
  let open: OpenConstruct | null = null;

  const push = (kind: TokenKind, from: number, to: number, value?: string) => {
    tokens.push({ kind, text: src.slice(from, to), value: value ?? src.slice(from, to), from, to });
  };

  while (i < n) {
    const ch = src[i];

    if (WS.test(ch)) { i += 1; continue; }

    // ── Comments ──────────────────────────────────────────────────────────────
    if (ch === '-' && src[i + 1] === '-') {
      const nl = src.indexOf('\n', i);
      const end = nl < 0 ? n : nl;
      push('comment', i, end);
      i = end;
      continue;
    }
    if (ch === '/' && src[i + 1] === '*') {
      // PostgreSQL nests block comments; Oracle does not. Counting depth on Oracle
      // would make `/* … /* … */` swallow the rest of the file.
      const nests = dialect === 'postgres';
      let depth = 1;
      let j = i + 2;
      while (j < n && depth > 0) {
        if (src[j] === '*' && src[j + 1] === '/') { depth -= 1; j += 2; }
        else if (nests && src[j] === '/' && src[j + 1] === '*') { depth += 1; j += 2; }
        else j += 1;
      }
      if (depth > 0) { open = { kind: 'comment', tag: '', bodyFrom: i + 2 }; push('comment', i, n); break; }
      push('comment', i, j);
      i = j;
      continue;
    }

    // ── Dollar-quoted body (PostgreSQL) ───────────────────────────────────────
    if (dollarQuotes && ch === '$') {
      const tag = matchDollarTag(src, i);
      if (tag) {
        const close = src.indexOf(tag, i + tag.length);
        if (close < 0) {
          open = { kind: 'dollar', tag, bodyFrom: i + tag.length };
          push('string', i, n, src.slice(i + tag.length, n));
          break;
        }
        push('string', i, close + tag.length, src.slice(i + tag.length, close));
        i = close + tag.length;
        continue;
      }
      if (DIGIT.test(src[i + 1] ?? '')) {           // `$1` positional parameter
        let j = i + 1;
        while (j < n && DIGIT.test(src[j])) j += 1;
        push('param', i, j);
        i = j;
        continue;
      }
    }

    // ── String literals, with their optional prefix letter ────────────────────
    if (ch === "'" || (STRING_PREFIX.test(ch) && src[i + 1] === "'")) {
      const quoteAt = ch === "'" ? i : i + 1;
      let j = quoteAt + 1;
      let closed = false;
      while (j < n) {
        if (src[j] === "'") {
          if (src[j + 1] === "'") { j += 2; continue; }   // doubled quote = escape
          closed = true;
          j += 1;
          break;
        }
        j += 1;
      }
      if (!closed) {
        open = { kind: 'string', tag: "'", bodyFrom: quoteAt + 1 };
        push('string', i, n, src.slice(quoteAt + 1, n));
        break;
      }
      push('string', i, j, src.slice(quoteAt + 1, j - 1));
      i = j;
      continue;
    }

    // ── Delimited identifier ──────────────────────────────────────────────────
    if (ch === '"') {
      let j = i + 1;
      let closed = false;
      while (j < n) {
        if (src[j] === '"') {
          if (src[j + 1] === '"') { j += 2; continue; }
          closed = true;
          j += 1;
          break;
        }
        j += 1;
      }
      if (!closed) {
        open = { kind: 'quoted', tag: '"', bodyFrom: i + 1 };
        push('quoted', i, n, src.slice(i + 1, n));
        break;
      }
      push('quoted', i, j, src.slice(i + 1, j - 1).replace(/""/g, '"'));
      i = j;
      continue;
    }

    // ── Numbers ───────────────────────────────────────────────────────────────
    if (DIGIT.test(ch) || (ch === '.' && DIGIT.test(src[i + 1] ?? ''))) {
      let j = i;
      while (j < n && /[0-9.]/.test(src[j])) j += 1;
      if (src[j] === 'e' || src[j] === 'E') {
        let k = j + 1;
        if (src[k] === '+' || src[k] === '-') k += 1;
        if (DIGIT.test(src[k] ?? '')) { j = k; while (j < n && DIGIT.test(src[j])) j += 1; }
      }
      push('number', i, j);
      i = j;
      continue;
    }

    // ── Bind parameters ───────────────────────────────────────────────────────
    // `::` is PostgreSQL's cast operator, not a parameter — check it first.
    if (ch === ':' && src[i + 1] !== ':' && IDENT_START.test(src[i + 1] ?? '')) {
      let j = i + 1;
      while (j < n && body.test(src[j])) j += 1;
      push('param', i, j);
      i = j;
      continue;
    }
    if (ch === '?') { push('param', i, i + 1); i += 1; continue; }

    // ── Words ─────────────────────────────────────────────────────────────────
    if (IDENT_START.test(ch)) {
      let j = i;
      while (j < n && body.test(src[j])) j += 1;
      push('word', i, j, src.slice(i, j).toUpperCase());
      i = j;
      continue;
    }

    push('punct', i, i + 1);
    i += 1;
  }

  return { tokens, open };
}

/** `$$` or `$tag$` at `at`, or `null` when the `$` is something else. */
function matchDollarTag(src: string, at: number): string | null {
  const m = /^\$(?:[A-Za-z_][A-Za-z0-9_]*)?\$/.exec(src.slice(at, at + 64));
  return m ? m[0] : null;
}

// ── Statements ────────────────────────────────────────────────────────────────

export interface SqlStatement {
  /** Significant tokens only — comments are dropped here, not in the scanner,
   *  because "is the caret in a comment" is still a question worth asking. */
  tokens: SqlToken[];
  from: number;
  to: number;
}

/**
 * Split a scan into statements on top-level `;` — **the fallback**.
 *
 * Used until the backend has answered about this exact buffer, and whenever it
 * cannot; {@link statementsFromSpans} is the path that normally runs. It stays
 * because a round trip must never be what stands between a keystroke and a
 * completion popup.
 *
 * **The known limit, which is why it is no longer the primary.** A PL/SQL or
 * PL/pgSQL block contains semicolons of its own, so an Oracle
 * `CREATE PROCEDURE … BEGIN … END;` comes apart into fragments rather than staying
 * whole. The failure mode is benign in isolation — each fragment is analysed on its
 * own merits, so an `INSERT` inside a trigger body is still checked against the
 * schema, and the leftovers (`END`, `x := 1`) name no tables and produce nothing.
 * What is *not* benign is that the Run path never split it that way, so for the
 * fifth of a second this is in force the two sides can disagree about which
 * statement the caret is in. PostgreSQL bodies are dollar-quoted, so they arrive as
 * a single string token and are simply not analysed at all.
 */
export function splitStatements(scan: TokenScan): SqlStatement[] {
  const out: SqlStatement[] = [];
  let current: SqlToken[] = [];
  let depth = 0;

  const flush = (end: number) => {
    if (current.length === 0) return;
    out.push({ tokens: current, from: current[0].from, to: end });
    current = [];
  };

  for (const t of scan.tokens) {
    if (t.kind === 'comment') continue;
    if (t.kind === 'punct') {
      if (t.text === '(') depth += 1;
      else if (t.text === ')') depth = Math.max(0, depth - 1);
      else if (t.text === ';' && depth === 0) { flush(t.to); continue; }
    }
    current.push(t);
  }
  flush(current.length ? current[current.length - 1].to : 0);
  return out;
}

/**
 * The same grouping, from boundaries the **backend's parser** found.
 *
 * The preferred path — see {@link splitStatements} for the limit this removes: a
 * PL/SQL block's own semicolons no longer cut it into fragments, because the
 * boundaries come from something that knows what a block is.
 *
 * Only the boundaries come from over there. The tokens are still this side's, and
 * each statement takes the significant ones that fall inside its span. `from` is
 * kept as the first of those rather than as the span's own start, so that a leading
 * comment belongs to no statement exactly as it did before — the boundaries change,
 * the convention every consumer reads does not.
 *
 * A span with nothing significant in it (a stray `;`, a comment-only tail) produces
 * no statement, which is what the local split does with the same input.
 */
export function statementsFromSpans(
  scan: TokenScan,
  spans: readonly { start: number; end: number }[],
): SqlStatement[] {
  const out: SqlStatement[] = [];
  // One pass over the tokens for all spans: both lists are in document order, so
  // the cursor into the tokens only ever moves forward. A `find` per span would be
  // quadratic on the scripts this exists for.
  let i = 0;
  for (const span of spans) {
    while (i < scan.tokens.length && scan.tokens[i].from < span.start) i += 1;
    const tokens: SqlToken[] = [];
    let j = i;
    while (j < scan.tokens.length && scan.tokens[j].to <= span.end) {
      if (scan.tokens[j].kind !== 'comment') tokens.push(scan.tokens[j]);
      j += 1;
    }
    i = j;
    // The terminator is a boundary, not content — `splitStatements` drops it and
    // every consumer downstream has always read a token list without it. Only a
    // TRAILING one: the semicolons inside a PL/SQL block are part of the block, and
    // the whole reason these spans are worth asking for is that the block stays one
    // statement.
    while (tokens.length && tokens[tokens.length - 1].kind === 'punct'
           && tokens[tokens.length - 1].text === ';') {
      tokens.pop();
    }
    if (tokens.length) out.push({ tokens, from: tokens[0].from, to: span.end });
  }
  return out;
}

/** The statement the caret sits in — the one whose span contains `offset`, or the
 *  last one that starts before it (the caret trailing a statement still belongs to
 *  it, which is what `Ctrl+Enter` and every completion expects). */
export function statementAt(statements: SqlStatement[], offset: number): SqlStatement | null {
  let candidate: SqlStatement | null = null;
  for (const s of statements) {
    if (s.from > offset) break;
    candidate = s;
  }
  if (!candidate) return null;
  // A caret past a terminated statement, with another one already begun, belongs
  // to neither — better nothing than the wrong scope.
  return offset <= candidate.to || candidate === statements[statements.length - 1] ? candidate : null;
}

/**
 * Is `offset` inside a comment or a string? Everything that proposes text asks
 * this first: a completion popup inside a comment is pure noise.
 *
 * The two differ at their far edge on purpose. A caret at the end of a `--` comment
 * is still *in* the comment — that is where you keep typing. A caret just past the
 * closing quote of `'abc'` is back in code, and treating it as a literal would kill
 * every proposal after a value.
 */
export function inLiteral(scan: TokenScan, offset: number): boolean {
  for (const t of scan.tokens) {
    if (t.from >= offset) break;
    if (t.to < offset) continue;
    if (t.kind === 'comment') return true;
    if (t.kind === 'string' && offset < t.to) return true;
  }
  return false;
}

// ── A small cache ─────────────────────────────────────────────────────────────
//
// Completion, hover, diagnostics and ghost text all run against the same buffer,
// often within the same keystroke. Scanning once per buffer version instead of
// four times is the difference between free and noticeable on a large script.
//
// Three entries, not two: two editors can be alive at once in a tabbed window, and
// a buffer briefly has two entries of its own — the one grouped locally and the one
// regrouped when the backend's boundaries arrive. Two would evict the other editor
// every time an answer landed.

interface CacheEntry {
  src: string;
  dialect: Dialect;
  /** The boundaries this entry was grouped by — see `scanSql`. */
  spans: readonly { start: number; end: number }[] | null;
  scan: TokenScan;
  statements: SqlStatement[];
}
const cache: CacheEntry[] = [];

/**
 * {@link tokenize} plus the statement grouping, memoised on the exact buffer text.
 *
 * The grouping comes from `statementSpanStore` — the backend's parser — whenever it
 * has an answer for this exact text, and from {@link splitStatements} otherwise.
 * Every consumer of this function gets the better boundaries without knowing that
 * either source exists, which is the point: five modules asked the same question
 * and would have needed the same fix five times.
 *
 * The fallback is not a degraded mode to be ashamed of, it is the normal state for
 * the first fifth of a second after a keystroke. Completion cannot wait for a round
 * trip and must never be the thing that waits.
 *
 * The spans are part of the cache key — by identity, since the store hands back the
 * same array until it has a new answer. Without that, an entry computed before the
 * reply landed would be handed out for the rest of that buffer's life.
 */
export function scanSql(src: string, dialect: Dialect): { scan: TokenScan; statements: SqlStatement[] } {
  const spans = statementSpanStore.for(src);
  const hit = cache.find((e) => e.src === src && e.dialect === dialect && e.spans === spans);
  if (hit) return { scan: hit.scan, statements: hit.statements };
  // The lexing itself never depends on the spans, so a cached scan for this text is
  // reused when only the boundaries arrived.
  const scan = cache.find((e) => e.src === src && e.dialect === dialect)?.scan
    ?? tokenize(src, dialect);
  const statements = spans ? statementsFromSpans(scan, spans) : splitStatements(scan);
  cache.unshift({ src, dialect, spans, scan, statements });
  cache.length = Math.min(cache.length, 3);
  return { scan, statements };
}

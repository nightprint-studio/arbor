/**
 * Turning a `@Query(nativeQuery = true)` into something a database can prepare.
 *
 * The text is SQL with Spring Data placeholders — `:name` and `?1` — that no server understands
 * (PostgreSQL wants `$1`). Validation prepares the statement, never runs it, so the placeholder only
 * has to be *a parameter*; its value is never needed.
 */

import type { Dialect } from '$lib/types/picus';

export interface PreparedNative {
  sql: string;
  /** Placeholders found, for the report. */
  paramCount: number;
  /** Where an offset in `sql` (what the server reports) lands in the original text. */
  toOriginal: (offset: number) => number;
}

/** The placeholder every non-Oracle engine is handed. `NULL` and not `$1`: a bare parameter has no
 *  type, and the server refuses a statement whose parameter it cannot infer (`$1 IS NULL`,
 *  `SELECT $1`, `LIMIT $1 OFFSET $2` in some engines) — a rejection of the check, not of the query.
 *  Preparing never needs a value, so a typed-by-context `NULL` is the honest stand-in. */
const NO_VALUE = 'NULL';

/**
 * Rewrite placeholders for `dialect`. Strings and comments are skipped, so a literal `':x'` or a
 * PostgreSQL cast `a::int` is left alone. Oracle already speaks `:name`, so only the positional
 * form needs touching there. A Spring Data SpEL placeholder (`:#{#filter.name}`) is one value.
 */
export function prepareNative(text: string, dialect: Dialect): PreparedNative {
  const names = new Set<string>();
  let out = '';
  let i = 0;
  /** `[start in out, end in out, start in text, end in text]` of every replaced run. */
  const replaced: [number, number, number, number][] = [];

  const swap = (from: number, to: number, replacement: string) => {
    replaced.push([out.length, out.length + replacement.length, from, to]);
    out += replacement;
    i = to;
  };
  const slot = (key: string) => { names.add(key); };
  // A collection parameter is written bare after IN (`in :ids`) and the provider expands it to a list;
  // a lone NULL there is a syntax error, not a value. Parenthesised, it is the one-element list it stands for.
  const value = (rep: string) => (/\bin\s*$/i.test(out) ? `(${rep})` : rep);

  while (i < text.length) {
    const c = text[i];
    const next = text[i + 1];
    if (c === "'") {
      const end = skipQuoted(text, i, "'");
      out += text.slice(i, end);
      i = end;
    } else if (c === '-' && next === '-') {
      const end = text.indexOf('\n', i);
      const stop = end < 0 ? text.length : end;
      out += text.slice(i, stop);
      i = stop;
    } else if (c === '/' && next === '*') {
      const end = text.indexOf('*/', i + 2);
      const stop = end < 0 ? text.length : end + 2;
      out += text.slice(i, stop);
      i = stop;
    } else if (c === ':' && next === ':') {
      out += '::';
      i += 2;
    } else if ((c === ':' || c === '?') && next === '#' && text[i + 2] === '{') {
      // `:#{…}` / `?#{…}` — a SpEL expression, balanced to its closing brace.
      let depth = 0;
      let j = i + 2;
      for (; j < text.length; j++) {
        if (text[j] === '{') depth++;
        else if (text[j] === '}' && --depth === 0) break;
      }
      const end = Math.min(j + 1, text.length);
      slot(text.slice(i, end));
      swap(i, end, value(dialect === 'oracle' ? ':spel' : NO_VALUE));
    } else if (c === ':' && next !== undefined && /[A-Za-z_]/.test(next)) {
      const m = /^[A-Za-z_][A-Za-z0-9_]*/.exec(text.slice(i + 1))!;
      slot(m[0]);
      swap(i, i + 1 + m[0].length, value(dialect === 'oracle' ? `:${m[0]}` : NO_VALUE));
    } else if (c === '?' && next !== undefined && /\d/.test(next)) {
      const m = /^\d+/.exec(text.slice(i + 1))!;
      slot(`?${m[0]}`);
      swap(i, i + 1 + m[0].length, value(dialect === 'oracle' ? `:p${m[0]}` : NO_VALUE));
    } else {
      out += c;
      i += 1;
    }
  }

  const toOriginal = (offset: number): number => {
    let shift = 0;
    for (const [outStart, outEnd, from, to] of replaced) {
      if (offset < outStart) break;
      // Inside a replacement the server is pointing at the placeholder: the whole of it, as written.
      if (offset < outEnd) return from;
      shift = to - outEnd;
    }
    return offset + shift;
  };
  return { sql: out, paramCount: names.size, toOriginal };
}

/** Index just past the quoted run starting at `from` (a doubled quote is an escaped one). */
function skipQuoted(text: string, from: number, quote: string): number {
  let i = from + 1;
  while (i < text.length) {
    if (text[i] === quote) {
      if (text[i + 1] === quote) i += 2;
      else return i + 1;
    } else {
      i += 1;
    }
  }
  return text.length;
}

/**
 * String literals in a source file, for the three host languages that carry inner-language
 * injections (Java, JavaScript/TypeScript, Rust).
 *
 * A plain scanner and not a tree query, for the reason `java-lexical.ts` gives: it answers with no
 * parser loaded, so a regex is coloured from the first paint and never lags a keystroke behind the
 * buffer. It only has to be right about *where literals are*, not about what the code around them
 * means — comments, character literals and (for JS) regex literals are stepped over so their
 * quotes do not open a string.
 */

export type Host = 'java' | 'js' | 'rust';

/** One string literal. Offsets are UTF-16 offsets into the scanned text. */
export interface Literal {
  /** The opening delimiter (the first quote, the `r#`, the `"""`). */
  open: number;
  /** The first character of the contents. */
  start: number;
  /** One past the last character of the contents. */
  end: number;
  /** One past the closing delimiter. */
  close: number;
  /** `"""…"""` in Java. */
  textBlock: boolean;
  /** Rust `r"…"` / `r#"…"#`: no escapes inside. */
  raw: boolean;
  /** JS `` `…` `` without `${…}`. */
  template: boolean;
}

/** The host a file's extension implies, or null when injections do not apply to it. */
export function hostForPath(path: string | null): Host | null {
  if (!path) return null;
  const ext = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  if (ext === 'java') return 'java';
  if (ext === 'rs') return 'rust';
  if (['js', 'mjs', 'cjs', 'jsx', 'ts', 'mts', 'cts', 'tsx'].includes(ext)) return 'js';
  return null;
}

/** Every string literal of `src`, in order. */
export function scanLiterals(src: string, host: Host): Literal[] {
  switch (host) {
    case 'java': return scanJava(src);
    case 'js': return scanJs(src);
    case 'rust': return scanRust(src);
  }
}

const base = (open: number, start: number, end: number, close: number): Literal =>
  ({ open, start, end, close, textBlock: false, raw: false, template: false });

function scanJava(src: string): Literal[] {
  const out: Literal[] = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === '/' && src[i + 1] === '/') {
      while (i < n && src[i] !== '\n') i++;
    } else if (c === '/' && src[i + 1] === '*') {
      const e = src.indexOf('*/', i + 2);
      i = e < 0 ? n : e + 2;
    } else if (c === "'") {
      i = skipCharLiteral(src, i);
    } else if (c === '"' && src.startsWith('"""', i)) {
      const lit = javaTextBlock(src, i);
      if (!lit) break;
      out.push(lit);
      i = lit.close;
    } else if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"' && src[j] !== '\n') j += src[j] === '\\' ? 2 : 1;
      if (j >= n || src[j] !== '"') { i = j; continue; } // unterminated on its line: not a literal yet
      out.push(base(i, i + 1, j, j + 1));
      i = j + 1;
    } else {
      i++;
    }
  }
  return out;
}

function skipCharLiteral(src: string, i: number): number {
  let j = i + 1;
  if (src[j] === '\\') j += 2;
  else j += 1;
  while (j < src.length && src[j] !== "'" && src[j] !== '\n' && j - i < 12) j++;
  return src[j] === "'" ? j + 1 : i + 1;
}

function javaTextBlock(src: string, i: number): Literal | null {
  let start = i + 3;
  let k = start;
  while (src[k] === ' ' || src[k] === '\t') k++;
  if (src[k] === '\r' && src[k + 1] === '\n') start = k + 2;
  else if (src[k] === '\n') start = k + 1;
  let j = start;
  while (j < src.length) {
    if (src[j] === '\\') j += 2;
    else if (src.startsWith('"""', j)) {
      return { ...base(i, start, j, j + 3), textBlock: true };
    } else j++;
  }
  return null;
}

/** Whether a `/` at `i` begins a regex literal rather than dividing: decided by what precedes it. */
function startsJsRegex(src: string, i: number): boolean {
  let j = i - 1;
  while (j >= 0 && (src[j] === ' ' || src[j] === '\t')) j--;
  if (j < 0) return true;
  if ('(,=:[!&|?{};+-*%<>~^'.includes(src[j])) return true;
  const word = /([A-Za-z]+)$/.exec(src.slice(Math.max(0, j - 10), j + 1));
  return !!word && ['return', 'typeof', 'case', 'in', 'of', 'delete', 'void', 'throw', 'new', 'else'].includes(word[1]);
}

function scanJs(src: string): Literal[] {
  const out: Literal[] = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === '/' && src[i + 1] === '/') {
      while (i < n && src[i] !== '\n') i++;
    } else if (c === '/' && src[i + 1] === '*') {
      const e = src.indexOf('*/', i + 2);
      i = e < 0 ? n : e + 2;
    } else if (c === '/' && startsJsRegex(src, i)) {
      i = skipJsRegex(src, i);
    } else if (c === '"' || c === "'") {
      let j = i + 1;
      while (j < n && src[j] !== c && src[j] !== '\n') j += src[j] === '\\' ? 2 : 1;
      if (j >= n || src[j] !== c) { i = j; continue; }
      out.push(base(i, i + 1, j, j + 1));
      i = j + 1;
    } else if (c === '`') {
      let j = i + 1;
      let interpolated = false;
      while (j < n && src[j] !== '`') {
        if (src[j] === '\\') j += 2;
        else if (src[j] === '$' && src[j + 1] === '{') { interpolated = true; j = skipTemplateExpr(src, j + 2); }
        else j++;
      }
      if (j >= n) break;
      // A template with an interpolation is not one string the pattern could be read from.
      if (!interpolated) out.push({ ...base(i, i + 1, j, j + 1), template: true });
      i = j + 1;
    } else {
      i++;
    }
  }
  return out;
}

function skipTemplateExpr(src: string, i: number): number {
  let depth = 1;
  let j = i;
  while (j < src.length && depth > 0) {
    if (src[j] === '{') depth++;
    else if (src[j] === '}') depth--;
    j++;
  }
  return j;
}

function skipJsRegex(src: string, i: number): number {
  let j = i + 1;
  let inClass = false;
  while (j < src.length && src[j] !== '\n') {
    const c = src[j];
    if (c === '\\') { j += 2; continue; }
    if (c === '[') inClass = true;
    else if (c === ']') inClass = false;
    else if (c === '/' && !inClass) return j + 1;
    j++;
  }
  return i + 1;
}

function scanRust(src: string): Literal[] {
  const out: Literal[] = [];
  const n = src.length;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === '/' && src[i + 1] === '/') {
      while (i < n && src[i] !== '\n') i++;
    } else if (c === '/' && src[i + 1] === '*') {
      i = skipRustBlockComment(src, i);
    } else if (c === 'r' && (src[i + 1] === '"' || src[i + 1] === '#') && !isIdentChar(src[i - 1])) {
      const raw = rustRaw(src, i);
      if (raw) { out.push(raw); i = raw.close; } else i++;
    } else if (c === 'b' && src[i + 1] === '"' && !isIdentChar(src[i - 1])) {
      i++; // a byte string is read like a string, from its quote
    } else if (c === '"') {
      let j = i + 1;
      while (j < n && src[j] !== '"') j += src[j] === '\\' ? 2 : 1;
      if (j >= n) break;
      out.push(base(i, i + 1, j, j + 1));
      i = j + 1;
    } else if (c === "'") {
      i = skipRustChar(src, i);
    } else {
      i++;
    }
  }
  return out;
}

const isIdentChar = (c: string | undefined) => !!c && /[A-Za-z0-9_]/.test(c);

function skipRustBlockComment(src: string, i: number): number {
  let depth = 0;
  let j = i;
  while (j < src.length) {
    if (src.startsWith('/*', j)) { depth++; j += 2; }
    else if (src.startsWith('*/', j)) { depth--; j += 2; if (depth === 0) return j; }
    else j++;
  }
  return src.length;
}

function rustRaw(src: string, i: number): Literal | null {
  let j = i + 1;
  let hashes = 0;
  while (src[j] === '#') { hashes++; j++; }
  if (src[j] !== '"') return null;
  const closer = '"' + '#'.repeat(hashes);
  const end = src.indexOf(closer, j + 1);
  if (end < 0) return null;
  return { ...base(i, j + 1, end, end + closer.length), raw: true };
}

/** A `'x'` / `'\n'` is a char literal; `'a` (no closing quote close by) is a lifetime. */
function skipRustChar(src: string, i: number): number {
  if (src[i + 1] === '\\') {
    const e = src.indexOf("'", i + 3);
    return e > 0 && e - i <= 12 ? e + 1 : i + 1;
  }
  const cp = src.codePointAt(i + 1);
  if (cp === undefined) return i + 1;
  const len = cp > 0xffff ? 2 : 1;
  return src[i + 1 + len] === "'" ? i + 2 + len : i + 1;
}

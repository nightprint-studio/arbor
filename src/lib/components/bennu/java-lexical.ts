/**
 * What a position in a Java buffer is lexically inside: code, a comment, or a literal.
 *
 * Shared by the two editing behaviours that have to know — an escaped paste into a string
 * (`java-string-paste.ts`) and the quote typing rules (`java-typing.ts`). Getting the answer wrong
 * is worse than doing nothing in both, so this is a plain scanner over the text rather than a tree
 * query: it answers with no parser loaded, no wasm and no live tree, which is the state the editor
 * is in for the first moments of every file, and it cannot lag a keystroke behind the buffer.
 *
 * The scan stops at the position asked about, so its cost is the text *before* it. A caller that
 * only needs the answer near a caret can pass the text up to the end of that line.
 */

/** A literal enclosing a position. */
export interface JavaLiteral {
  kind: 'string' | 'text-block' | 'char';
  /** Offset of the opening delimiter. */
  from: number;
  /** Offset just past the closing delimiter — or, unterminated, where the scan gave up. */
  to: number;
  terminated: boolean;
}

/** The lexical context of a position. */
export type JavaContext = { kind: 'code' } | { kind: 'comment' } | JavaLiteral;

const CODE: JavaContext = { kind: 'code' };
const COMMENT: JavaContext = { kind: 'comment' };

interface Span {
  /** Offset just past the closing delimiter, or where the scan gave up. */
  to: number;
  terminated: boolean;
}

/**
 * The lexical context enclosing `offset`.
 *
 * A terminated literal ends **exclusively**: a caret just past the closing quote of `"abc"` is
 * back in code. An unterminated one ends inclusively, because `String s = "` with the caret after
 * the quote is a literal you have just opened in order to put something in it.
 */
export function javaContextAt(src: string, offset: number): JavaContext {
  const pos = Math.max(0, Math.min(offset, src.length));
  const n = src.length;
  let i = 0;

  // Every token starting past the caret is irrelevant, so the scan stops there rather than
  // tokenizing the rest of the file.
  while (i < n && i <= pos) {
    const c = src[i];

    if (c === '/' && src[i + 1] === '/') {
      const nl = src.indexOf('\n', i + 2);
      const to = nl < 0 ? n : nl;
      // A quote inside a comment is text, not a delimiter.
      if (pos > i && pos <= to) return COMMENT;
      i = to + 1;
      continue;
    }
    if (c === '/' && src[i + 1] === '*') {
      const close = src.indexOf('*/', i + 2);
      const to = close < 0 ? n : close + 2;
      if (pos > i && pos < to) return COMMENT;
      i = to;
      continue;
    }
    // `"""` first: it also starts with the character a plain string starts with.
    if (c === '"' && src[i + 1] === '"' && src[i + 2] === '"') {
      const span = scanTextBlock(src, i);
      if (encloses(i, span, pos)) return { kind: 'text-block', from: i, ...span };
      i = span.to;
      continue;
    }
    if (c === '"' || c === "'") {
      const span = scanQuoted(src, i, c);
      if (encloses(i, span, pos)) return { kind: c === '"' ? 'string' : 'char', from: i, ...span };
      i = span.to;
      continue;
    }
    i++;
  }
  return CODE;
}

/** The literal enclosing `offset`, or `null` when it is in code or a comment. */
export function javaLiteralAt(src: string, offset: number): JavaLiteral | null {
  const ctx = javaContextAt(src, offset);
  return ctx.kind === 'code' || ctx.kind === 'comment' ? null : ctx;
}

function encloses(from: number, span: Span, pos: number): boolean {
  if (pos <= from) return false;
  return span.terminated ? pos < span.to : pos <= span.to;
}

/** Scan a `"…"` / `'…'` from its opening quote. A newline ends it unterminated — Java has no line
 *  continuation inside one, so an unclosed literal stops at the end of its line and everything
 *  after is code again. */
function scanQuoted(src: string, open: number, quote: string): Span {
  const n = src.length;
  let i = open + 1;
  while (i < n) {
    const c = src[i];
    if (c === '\\') { i += 2; continue; }
    if (c === quote) return { to: i + 1, terminated: true };
    if (c === '\n') return { to: i, terminated: false };
    i++;
  }
  return { to: n, terminated: false };
}

/** Scan a `"""…"""` from its opening delimiter. Newlines are part of it. */
function scanTextBlock(src: string, open: number): Span {
  const n = src.length;
  let i = open + 3;
  while (i < n) {
    if (src[i] === '\\') { i += 2; continue; }
    if (src[i] === '"' && src[i + 1] === '"' && src[i + 2] === '"') {
      return { to: i + 3, terminated: true };
    }
    i++;
  }
  return { to: n, terminated: false };
}

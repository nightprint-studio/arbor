/**
 * Pasting into a Java string literal — escaping, and the line break Java has no
 * syntax for.
 *
 * Two things go wrong when clipboard text meets a `"…"`. A quote or a backslash in
 * it closes or reinterprets the literal, which is a compile error and therefore the
 * harmless half. A **newline** is worse: `"` cannot span lines in Java, so a pasted
 * XML fragment or SQL statement lands as a wall of errors that has to be turned into
 * concatenated literals by hand, one line at a time. This does both at the moment of
 * the paste:
 *
 *     String q = "|";                 ← paste `SELECT *\nFROM "user"`
 *     String q = "SELECT *\n" +
 *                "FROM \"user\"";
 *
 * The rules differ by which literal the caret is in, and getting that wrong is worse
 * than doing nothing — so {@link javaLiteralAt} is a plain scanner over the buffer
 * rather than a tree query: it answers with no parser loaded, no wasm and no live
 * tree, which is the state the editor is in for the first moments of every file.
 *
 * A **text block** takes newlines as they are, so it gets indentation instead of
 * concatenation, and only what would actually close it (`"""`) is escaped — a block
 * is usually chosen precisely to avoid escaping every quote in a pasted HTML
 * fragment, and escaping them anyway would defeat the reason it is there.
 *
 * A **character literal** is escaped and never wrapped: a multi-line paste into one
 * is already meaningless, and inventing a shape for it would only hide that.
 *
 * The CodeMirror wiring is shared (`shared/ui/code-editor/paste-literal.ts`); what
 * lives here is only what is true of Java.
 */

import { pasteIntoLiteral, type LiteralPasteRefusal } from '$lib/components/shared/ui/code-editor';
import type { Extension } from '@codemirror/state';

// ── Where the caret is ─────────────────────────────────────────────────────────

/** A literal enclosing a position. `from` is the offset of its opening delimiter. */
export interface JavaLiteral {
  kind: 'string' | 'text-block' | 'char';
  from: number;
}

interface Span {
  /** Offset just past the closing delimiter, or where the scan gave up. */
  to: number;
  terminated: boolean;
}

/**
 * The literal enclosing `offset`, or `null` when it is in code or a comment.
 *
 * A terminated literal ends **exclusively**: a caret just past the closing quote of
 * `"abc"` is back in code, and pasting there is an ordinary paste. An unterminated
 * one ends inclusively, because `String s = "` with the caret after the quote is one
 * of the positions this exists for — a literal you have just opened in order to put
 * something in it.
 */
export function javaLiteralAt(src: string, offset: number): JavaLiteral | null {
  const pos = Math.max(0, Math.min(offset, src.length));
  const n = src.length;
  let i = 0;

  // Every token starting past the caret is irrelevant, so the scan stops there
  // rather than tokenizing the rest of the file.
  while (i < n && i <= pos) {
    const c = src[i];

    if (c === '/' && src[i + 1] === '/') {
      const nl = src.indexOf('\n', i + 2);
      const to = nl < 0 ? n : nl;
      // A quote inside a comment is text, not a delimiter — and a paste there is
      // an ordinary paste.
      if (pos > i && pos <= to) return null;
      i = to + 1;
      continue;
    }
    if (c === '/' && src[i + 1] === '*') {
      const close = src.indexOf('*/', i + 2);
      const to = close < 0 ? n : close + 2;
      if (pos > i && pos < to) return null;
      i = to;
      continue;
    }
    // `"""` first: it also starts with the character a plain string starts with.
    if (c === '"' && src[i + 1] === '"' && src[i + 2] === '"') {
      const span = scanTextBlock(src, i);
      if (encloses(i, span, pos)) return { kind: 'text-block', from: i };
      i = span.to;
      continue;
    }
    if (c === '"' || c === "'") {
      const span = scanQuoted(src, i, c);
      if (encloses(i, span, pos)) return { kind: c === '"' ? 'string' : 'char', from: i };
      i = span.to;
      continue;
    }
    i++;
  }
  return null;
}

function encloses(from: number, span: Span, pos: number): boolean {
  if (pos <= from) return false;
  return span.terminated ? pos < span.to : pos <= span.to;
}

/** Scan a `"…"` / `'…'` from its opening quote. A newline ends it unterminated —
 *  Java has no line continuation inside one, so an unclosed literal stops at the end
 *  of its line and everything after is code again. */
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

// ── Escaping ───────────────────────────────────────────────────────────────────

const CONTROL_ESCAPES: Record<string, string> = {
  '\b': '\\b',
  '\f': '\\f',
  '\n': '\\n',
  '\r': '\\r',
  '\t': '\\t',
};

/** Escape one run of text for a `"…"` or `'…'` literal. */
function escapeRun(raw: string, quote: '"' | "'"): string {
  let out = '';
  for (const ch of raw) {
    if (ch === '\\') { out += '\\\\'; continue; }
    if (ch === quote) { out += '\\' + quote; continue; }
    const known = CONTROL_ESCAPES[ch];
    if (known) { out += known; continue; }
    const code = ch.codePointAt(0) ?? 0;
    // The remaining control characters have no readable escape, and octal is the
    // only safe encoding for them. A unicode escape is not: Java resolves those in
    // an earlier phase than tokenizing, so the one standing for a line feed becomes
    // a real newline and splits the literal it was meant to sit inside.
    if (code < 0x20 || code === 0x7f) { out += '\\' + code.toString(8).padStart(3, '0'); continue; }
    out += ch;
  }
  return out;
}

/** Escape one line for a text block: only a backslash, and the quotes that would
 *  close the block. Every third consecutive quote is escaped (three in a row is the
 *  delimiter), as is a quote at the very end of the paste — it would otherwise lean
 *  against whatever follows the caret and make a fourth. */
function escapeTextBlockLine(raw: string): string {
  const chars = [...raw];
  let out = '';
  let run = 0;
  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i];
    if (ch === '\\') { out += '\\\\'; run = 0; continue; }
    if (ch === '"') {
      run++;
      if (run === 3 || i === chars.length - 1) { out += '\\"'; run = 0; } else out += '"';
      continue;
    }
    run = 0;
    out += ch;
  }
  return out;
}

// ── Indentation ────────────────────────────────────────────────────────────────

/** Whitespace that puts a continuation line under `offset`'s column. Tabs are kept
 *  as tabs so the alignment survives in a file indented with them. */
function alignUnder(doc: string, offset: number): string {
  const start = doc.lastIndexOf('\n', offset - 1) + 1;
  let pad = '';
  for (let i = start; i < offset; i++) pad += doc[i] === '\t' ? '\t' : ' ';
  return pad;
}

/** The leading whitespace of the line `offset` sits on. */
function lineIndentAt(doc: string, offset: number): string {
  const start = doc.lastIndexOf('\n', Math.max(0, offset - 1)) + 1;
  let i = start;
  while (i < doc.length && (doc[i] === ' ' || doc[i] === '\t')) i++;
  return doc.slice(start, i);
}

// ── Limits ─────────────────────────────────────────────────────────────────────

/**
 * How many concatenated literals a paste may become.
 *
 * `a + b + c + …` parses as a chain nested one level per operand, and everything
 * that reads Java walks that chain by recursing — the highlighter here, the symbol
 * and inference passes in the backend. Thousands of operands is thousands of stack
 * frames, and in the backend a stack overflow is not a panic that can be caught: the
 * process aborts, taking every other file's diagnostics with it.
 *
 * So the split is capped. Past this a paste still gets escaped, just into **one**
 * literal with its newlines inline — same value, same validity, a tree one level
 * deep instead of a thousand. 500 is far above any paste that wants to be read as
 * code and far below anything that troubles a stack.
 */
const MAX_SEGMENTS = 500;

/**
 * What a Java string constant can hold: 65535 **UTF-8 bytes**, the width of the
 * length field in a class file's constant pool.
 *
 * Above this there is no correct answer to give. Splitting does not help — javac
 * folds `"a" + "b"` into one constant before the limit applies — so the paste is
 * refused and says so, rather than producing something that looks right in the
 * editor and fails at `javac` with a message about a constant pool.
 */
const MAX_CONSTANT_BYTES = 65535;

/** UTF-8 length, computed only when it could possibly matter: a UTF-16 code unit
 *  encodes to at most 3 bytes, so anything shorter than a third of the limit is
 *  under it whatever it contains, and the common paste costs one multiplication. */
function exceedsConstantLimit(text: string): boolean {
  if (text.length * 3 <= MAX_CONSTANT_BYTES) return false;
  return new TextEncoder().encode(text).length > MAX_CONSTANT_BYTES;
}

/** `65535` → `64 KB`, for a message a person reads. */
function humanSize(text: string): string {
  const kb = new TextEncoder().encode(text).length / 1024;
  return kb >= 1024 ? `${(kb / 1024).toFixed(1)} MB` : `${Math.round(kb)} KB`;
}

// ── The renderer ───────────────────────────────────────────────────────────────

/**
 * What to insert for a paste of `text` at `offset`, or `null` when the caret is not
 * inside a literal, or a refusal when no valid result exists.
 *
 * Exported for its own sake: this is the whole behaviour, and it is a pure function
 * of three strings, so it can be reasoned about (and driven) without an editor.
 */
export function renderJavaPaste(
  doc: string,
  offset: number,
  text: string,
): string | LiteralPasteRefusal | null {
  const literal = javaLiteralAt(doc, offset);
  if (!literal) return null;

  // Refuse before doing any work: past the constant limit every possible result is
  // wrong, and the honest ones are "nothing happened" or "here is why".
  if (exceedsConstantLimit(text)) {
    return {
      refuse: `Not pasted — ${humanSize(text)} is more than a Java string can hold. `
        + 'A compiled string constant is capped at 64 KB, however it is written.',
    };
  }

  if (literal.kind === 'char') return escapeRun(text, "'");

  const lines = text.split(/\r\n|\r|\n/);
  // A paste that ends in a newline would otherwise open a final empty segment. The
  // newline is real and has to survive; the empty segment does not.
  const trailing = lines.length > 1 && lines[lines.length - 1] === '';
  if (trailing) lines.pop();

  if (literal.kind === 'text-block') {
    // Newlines are legal here, so the only work is keeping the body under the
    // block's own indentation — Java strips the *common* leading whitespace, so
    // pasting flush-left lines would silently de-indent every other line in the
    // block instead of just looking untidy.
    const indent = lineIndentAt(doc, offset);
    const body = lines.map(escapeTextBlockLine).join('\n' + indent);
    return trailing ? body + '\n' + indent : body;
  }

  // `"…"` cannot span lines, so one literal per line, concatenated and aligned under
  // the opening quote — the shape the code would have been written in by hand. Past
  // MAX_SEGMENTS that shape is a liability rather than a courtesy, and the newlines
  // stay inline in a single literal.
  const join = lines.length > MAX_SEGMENTS ? '\\n' : `\\n" +\n${alignUnder(doc, literal.from)}"`;
  const out = lines.map((line) => escapeRun(line, '"')).join(join);
  return trailing ? out + '\\n' : out;
}

/** The editor extension, for the Java {@link import('./java-lang').javaLanguage} descriptor. */
export const javaStringPaste: Extension = pasteIntoLiteral(renderJavaPaste);

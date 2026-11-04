/**
 * A string literal's *value*, with every character of it tied back to the source characters that
 * wrote it.
 *
 * `"\\d+\n"` is the five characters `\d+` + line feed, and a highlighter, a regex parser or a
 * diagnostics pass must see exactly those. But what the editor paints and where a squiggle lands is
 * in *source* coordinates, so the unescaped text travels as {@link Unit}s — one per UTF-16 unit of the
 * value, each remembering the source span it came from. Mapping a span of the value back to the
 * buffer is then a lookup ({@link sourceRanges}), and it is the same lookup whether the value came
 * from an escape (`\\` → one unit, two source characters), a Java text block (indentation stripped,
 * so the units are not contiguous) or a concatenation of several literals.
 */

import type { Host, Literal } from './literals';

export interface Unit {
  /** One UTF-16 unit of the value. */
  ch: string;
  /** The source span that wrote it. Several units can share one (an astral `\u{…}` is two). */
  from: number;
  to: number;
}

/** The value of a literal as units. */
export function literalUnits(src: string, lit: Literal, host: Host): Unit[] {
  const raw = lit.textBlock ? textBlockRaw(src, lit) : identityUnits(src, lit.start, lit.end);
  return lit.raw ? raw : processEscapes(raw, host);
}

function identityUnits(src: string, from: number, to: number): Unit[] {
  const out: Unit[] = [];
  for (let i = from; i < to; i++) out.push({ ch: src[i], from: i, to: i + 1 });
  return out;
}

// ── Java text blocks ──────────────────────────────────────────────────────────────

interface Line { from: number; to: number; eolFrom: number; eolTo: number }

/** Split `[start, end)` into lines, each with its terminator. */
function splitLines(src: string, start: number, end: number): Line[] {
  const lines: Line[] = [];
  let from = start;
  let i = start;
  while (i < end) {
    const c = src[i];
    if (c === '\n' || c === '\r') {
      const eolTo = c === '\r' && src[i + 1] === '\n' ? i + 2 : i + 1;
      lines.push({ from, to: i, eolFrom: i, eolTo });
      from = eolTo;
      i = eolTo;
    } else i++;
  }
  lines.push({ from, to: end, eolFrom: end, eolTo: end });
  return lines;
}

const isBlank = (s: string) => /^[ \t\f]*$/.test(s);

function indentOf(s: string): number {
  let n = 0;
  while (n < s.length && (s[n] === ' ' || s[n] === '\t' || s[n] === '\f')) n++;
  return n;
}

/**
 * A text block's incidental-whitespace rules (JLS 3.10.6): strip the common indentation, strip
 * trailing spaces, normalise line endings to `\n`. Escapes are still undone afterwards, which is why
 * `\s` and `\040` survive the stripping.
 */
function textBlockRaw(src: string, lit: Literal): Unit[] {
  const lines = splitLines(src, lit.start, lit.end);
  const last = lines.length - 1;
  // Significant lines: the non-blank ones, plus the closing-delimiter line even when it is blank.
  let min = Infinity;
  lines.forEach((l, i) => {
    const text = src.slice(l.from, l.to);
    if (i === last || !isBlank(text)) min = Math.min(min, indentOf(text));
  });
  if (!Number.isFinite(min)) min = 0;

  const out: Unit[] = [];
  lines.forEach((l, i) => {
    const text = src.slice(l.from, l.to);
    const stripped = isBlank(text) ? 0 : min;
    let to = l.to;
    while (to > l.from + stripped && /[ \t\f]/.test(src[to - 1])) to--;
    if (!isBlank(text)) for (let p = l.from + min; p < to; p++) out.push({ ch: src[p], from: p, to: p + 1 });
    if (i < last) out.push({ ch: '\n', from: l.eolFrom, to: l.eolTo });
  });
  return out;
}

/** The indentation a text block's lines carry in the source — what a rewritten body must keep. */
export function textBlockIndent(src: string, lit: Literal): string {
  const lines = splitLines(src, lit.start, lit.end);
  const last = lines.length - 1;
  let min = Infinity;
  let sample = '';
  lines.forEach((l, i) => {
    const text = src.slice(l.from, l.to);
    if (i === last || !isBlank(text)) {
      const n = indentOf(text);
      if (n < min) { min = n; sample = text.slice(0, n); }
    }
  });
  return sample;
}

// ── Escapes ───────────────────────────────────────────────────────────────────────

const JAVA_SIMPLE: Record<string, string> = {
  b: '\b', t: '\t', n: '\n', f: '\f', r: '\r', s: ' ', '"': '"', "'": "'", '\\': '\\',
};
const JS_SIMPLE: Record<string, string> = {
  b: '\b', t: '\t', n: '\n', f: '\f', r: '\r', v: '\v',
};
const RUST_SIMPLE: Record<string, string> = {
  n: '\n', r: '\r', t: '\t', '\\': '\\', '0': '\0', "'": "'", '"': '"',
};

interface Escape { value: string; length: number }

function hexRun(raw: Unit[], at: number, max: number): string {
  let s = '';
  while (s.length < max && at + s.length < raw.length && /[0-9a-fA-F]/.test(raw[at + s.length].ch)) s += raw[at + s.length].ch;
  return s;
}

/** The escape starting at `raw[i]` (a backslash), or null when it is not a valid one. */
function readEscape(raw: Unit[], i: number, host: Host): Escape | null {
  const next = raw[i + 1]?.ch;
  if (next === undefined) return null;
  if (next === '\n') return { value: '', length: 2 }; // a line continuation, text blocks and JS/Rust

  if (host === 'java') {
    if (next in JAVA_SIMPLE) return { value: JAVA_SIMPLE[next], length: 2 };
    if (next >= '0' && next <= '7') {
      let digits = next;
      const limit = next <= '3' ? 3 : 2;
      while (digits.length < limit && raw[i + 1 + digits.length]?.ch >= '0' && raw[i + 1 + digits.length]?.ch <= '7') digits += raw[i + 1 + digits.length].ch;
      return { value: String.fromCharCode(parseInt(digits, 8)), length: 1 + digits.length };
    }
    if (next === 'u') {
      let j = i + 1;
      while (raw[j]?.ch === 'u') j++;
      const hex = hexRun(raw, j, 4);
      if (hex.length === 4) return { value: String.fromCharCode(parseInt(hex, 16)), length: j + 4 - i };
    }
    return null;
  }

  if (host === 'js') {
    if (next in JS_SIMPLE) return { value: JS_SIMPLE[next], length: 2 };
    if (next === '0' && !/[0-9]/.test(raw[i + 2]?.ch ?? '')) return { value: '\0', length: 2 };
    if (next === 'x') {
      const hex = hexRun(raw, i + 2, 2);
      if (hex.length === 2) return { value: String.fromCharCode(parseInt(hex, 16)), length: 4 };
      return null;
    }
    if (next === 'u') return unicodeBraced(raw, i) ?? fixedUnicode(raw, i);
    return { value: next, length: 2 }; // `\d` in a JS string is just `d` — and the pattern will show it
  }

  // rust
  if (next in RUST_SIMPLE) return { value: RUST_SIMPLE[next], length: 2 };
  if (next === 'x') {
    const hex = hexRun(raw, i + 2, 2);
    return hex.length === 2 ? { value: String.fromCharCode(parseInt(hex, 16)), length: 4 } : null;
  }
  if (next === 'u') return unicodeBraced(raw, i);
  return null;
}

function unicodeBraced(raw: Unit[], i: number): Escape | null {
  if (raw[i + 2]?.ch !== '{') return null;
  const hex = hexRun(raw, i + 3, 6);
  if (!hex || raw[i + 3 + hex.length]?.ch !== '}') return null;
  const cp = parseInt(hex, 16);
  if (cp > 0x10ffff) return null;
  return { value: String.fromCodePoint(cp), length: 4 + hex.length + 1 - 1 };
}

function fixedUnicode(raw: Unit[], i: number): Escape | null {
  const hex = hexRun(raw, i + 2, 4);
  return hex.length === 4 ? { value: String.fromCharCode(parseInt(hex, 16)), length: 6 } : null;
}

/** Undo the escapes of `raw` (a literal's characters, one per unit). An invalid escape stays as written. */
export function processEscapes(raw: Unit[], host: Host): Unit[] {
  const out: Unit[] = [];
  for (let i = 0; i < raw.length; i++) {
    const u = raw[i];
    if (u.ch !== '\\') { out.push(u); continue; }
    const esc = readEscape(raw, i, host);
    if (!esc) { out.push(u); continue; }
    const from = u.from;
    const to = raw[i + esc.length - 1].to;
    for (const ch of esc.value) for (const unit of ch.length === 2 ? [ch[0], ch[1]] : [ch]) out.push({ ch: unit, from, to });
    // A Rust continuation also swallows the whitespace that opens the next line.
    i += esc.length - 1;
    if (host === 'rust' && esc.value === '') while (/[ \t\n\r]/.test(raw[i + 1]?.ch ?? 'x')) i++;
  }
  return out;
}

/** The text of a list of units. */
export const unitsText = (units: Unit[]): string => units.map((u) => u.ch).join('');

/**
 * The source ranges a span `[a, b)` of the value covers. Units that are not adjacent in the source
 * (a stripped text-block indent, the gap between two concatenated literals) split the span, so a
 * mark never paints over code that is not part of the value.
 */
export function sourceRanges(units: Unit[], a: number, b: number): { from: number; to: number }[] {
  const out: { from: number; to: number }[] = [];
  let cur: { from: number; to: number } | null = null;
  for (let i = Math.max(0, a); i < Math.min(units.length, b); i++) {
    const u = units[i];
    if (cur && u.from <= cur.to && u.to >= cur.to) cur.to = u.to;
    else if (cur && u.from >= cur.from && u.to <= cur.to) continue; // same escape, second unit
    else { cur = { from: u.from, to: u.to }; out.push(cur); }
  }
  return out;
}

// ── Writing a value back ──────────────────────────────────────────────────────────

const CONTROL: Record<string, string> = { '\b': '\\b', '\f': '\\f', '\n': '\\n', '\r': '\\r', '\t': '\\t' };

/** The contents to put between a literal's delimiters so that it reads back as `value`.
 *  Returns null when the value cannot be written into this kind of literal (a raw string cannot
 *  contain its own closing delimiter). */
export function escapeForLiteral(
  value: string, host: Host, lit: Pick<Literal, 'textBlock' | 'raw' | 'template'>,
  opts: { quote?: string; indent?: string; hashes?: number } = {},
): string | null {
  if (lit.raw) {
    const closer = '"' + '#'.repeat(opts.hashes ?? 0);
    return value.includes(closer) ? null : value;
  }
  if (host === 'java' && lit.textBlock) {
    const indent = opts.indent ?? '';
    const escaped = value
      .replace(/\\/g, '\\\\')
      .replace(/"""/g, '""\\"')
      .replace(/[ \t]+$/gm, (m) => m.replace(/ /g, '\\s').replace(/\t/g, '\\t'))
      .replace(/\r/g, '\\r');
    return escaped.split('\n').map((l) => (l === '' ? l : indent + l)).join('\n');
  }
  const quote = lit.template ? '`' : (opts.quote ?? '"');
  let out = '';
  for (const ch of value) {
    if (ch === '\\') out += '\\\\';
    else if (ch === quote) out += '\\' + quote;
    else if (lit.template && ch === '$') out += '\\$';
    else if (ch in CONTROL && !(lit.template && ch === '\n')) out += CONTROL[ch];
    else if (ch < ' ' || ch === '\x7f') out += host === 'java' ? '\\' + ch.charCodeAt(0).toString(8).padStart(3, '0') : '\\x' + ch.charCodeAt(0).toString(16).padStart(2, '0');
    else out += ch;
  }
  return out;
}

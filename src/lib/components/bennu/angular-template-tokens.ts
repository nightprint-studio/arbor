/**
 * A tokenizer for the Angular-only syntax of a component template — pure text in, spans out, no
 * CodeMirror in sight so it can be tested on its own.
 *
 * `lang-html` knows HTML and nothing of what Angular puts on top of it: control-flow blocks
 * (`@if (…) {`), `{{ interpolation | pipe }}`, and the binding attributes (`[prop]`, `(event)`,
 * `[(model)]`, `*ngIf`, `#ref`). What those carry is a *TypeScript-ish expression*, and the
 * expression is what this colours: strings, numbers, operators, `?.`, calls, members, and the name
 * after a pipe's `|`.
 *
 * ## Conservative on purpose
 *
 * There is no file extension that says "Angular" (it is `.html`), so instead of guessing per file
 * the tokenizer only ever emits spans for syntax that means something *only* in Angular — and
 * says nothing about anything else. An ordinary page produces no spans (bar `{{ }}`, which every
 * mustache-style template shares and which is coloured the same way). In particular:
 *
 *  - `@name` is a block only at a block start (line start, after whitespace, `}`, `>`) and only
 *    when the shape is complete: `@if (…) {`, `@else {`. `me@if.com` and `@media` are left alone.
 *  - `[x]`, `(x)`, `*x`, `#x` are read only as *attribute names* inside a tag, never in text.
 *  - Nothing is emitted for a construct that does not close (an unterminated `{{`, a header with
 *    no `)`): half-typed input must not repaint the rest of the file.
 *
 * Spans may sit inside what `lang-html` already coloured (an attribute value is a string to it);
 * the host draws them on top.
 */

export type NgClass =
  | 'kw' // `@if`, `@for`, `let`, `of`, `track`, `typeof`
  | 'punct' // `{`, `}`, `{{`, `}}`, brackets around a binding name, `,` `;` `(` `)`
  | 'op' // `&&`, `!=`, `?.`, `|`, `!`, …
  | 'str'
  | 'num'
  | 'lit' // `true` `false` `null` `undefined` `this`
  | 'var' // a name read from the component
  | 'prop' // `.name`
  | 'call' // `name(` and `.name(`
  | 'pipe' // the name after `|`
  | 'bind' // `prop` of `[prop]`
  | 'event' // `click` of `(click)`
  | 'dir' // `*ngIf`
  | 'ref'; // `#name`

export interface NgSpan { from: number; to: number; cls: NgClass }

/** Templates larger than this are not scanned: a generated megabyte of markup is not worth a pass per keystroke. */
export const MAX_SCAN_LENGTH = 400_000;

const BLOCK_NAME = /@(else if|if|else|for|empty|switch|case|default|defer|placeholder|loading|error|let)\b/y;
/** Blocks whose header is mandatory — `@if` without `(` is text, not a block. */
const NEEDS_PAREN = new Set(['if', 'else if', 'for', 'switch', 'case']);

const BASE_KW = new Set(['typeof', 'void', 'in', 'instanceof', 'new']);
const BLOCK_KW = new Set([...BASE_KW, 'of', 'track', 'let', 'as']);
const DEFER_KW = new Set([
  ...BLOCK_KW, 'on', 'when', 'prefetch', 'hydrate', 'never',
  'idle', 'viewport', 'interaction', 'hover', 'immediate', 'timer', 'minimum', 'after',
]);
const MICRO_KW = new Set([...BASE_KW, 'let', 'as', 'of', 'trackBy']);
const LITERALS = new Set(['true', 'false', 'null', 'undefined', 'this', 'NaN', 'Infinity']);

const IDENT_START = /[A-Za-z_$]/;
const IDENT = /[\w$]*/y;
const NUMBER = /(?:\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?|\.\d+)/y;
const OPERATOR = /[+\-*/%=!<>&|?^~]+/y;

type ExprMode = 'expr' | 'stmt' | 'block' | 'defer' | 'micro';

function keywordsOf(mode: ExprMode): Set<string> {
  return mode === 'block' ? BLOCK_KW : mode === 'defer' ? DEFER_KW : mode === 'micro' ? MICRO_KW : BASE_KW;
}

/** The end (exclusive) of the string literal opening at `i`, or `limit` when it never closes. */
function stringEnd(src: string, i: number, limit: number): number {
  const quote = src[i];
  for (let j = i + 1; j < limit; j++) {
    if (src[j] === '\\') j++;
    else if (src[j] === quote) return j + 1;
  }
  return limit;
}

/** The index of the `)` matching the `(` at `open`, skipping strings; -1 if it never closes. */
function matchingParen(src: string, open: number): number {
  let depth = 0;
  for (let j = open; j < src.length; j++) {
    const c = src[j];
    if (c === '"' || c === "'" || c === '`') j = stringEnd(src, j, src.length) - 1;
    else if (c === '(') depth++;
    else if (c === ')' && --depth === 0) return j;
  }
  return -1;
}

/** The first non-space index at or after `i`, bounded by `limit`. */
function skipSpace(src: string, i: number, limit = src.length): number {
  while (i < limit && /\s/.test(src[i])) i++;
  return i;
}

/** Colour `[from, to)` as an expression of the given kind. */
function scanExpr(src: string, from: number, to: number, mode: ExprMode, out: NgSpan[]): void {
  const keywords = keywordsOf(mode);
  let afterDot = false;
  let pipePending = false;
  let i = from;
  while (i < to) {
    const c = src[i];
    if (/\s/.test(c)) { i++; continue; }
    if (c === '"' || c === "'" || c === '`') {
      const end = stringEnd(src, i, to);
      out.push({ from: i, to: end, cls: 'str' });
      i = end; afterDot = false;
      continue;
    }
    if (/\d/.test(c) || (c === '.' && /\d/.test(src[i + 1] ?? ''))) {
      NUMBER.lastIndex = i;
      const m = NUMBER.exec(src);
      if (m && m.index === i) {
        out.push({ from: i, to: i + m[0].length, cls: 'num' });
        i += m[0].length; afterDot = false;
        continue;
      }
    }
    if (IDENT_START.test(c)) {
      IDENT.lastIndex = i;
      const end = i + IDENT.exec(src)![0].length;
      const word = src.slice(i, end);
      const next = skipSpace(src, end, to);
      const isCall = src[next] === '(';
      let cls: NgClass;
      if (afterDot) cls = isCall ? 'call' : 'prop';
      else if (pipePending) { cls = 'pipe'; pipePending = false; }
      else if (keywords.has(word)) cls = 'kw';
      else if (LITERALS.has(word)) cls = 'lit';
      else cls = isCall ? 'call' : 'var';
      out.push({ from: i, to: end, cls });
      i = end; afterDot = false;
      continue;
    }
    if (c === '?' && src[i + 1] === '.' && !/\d/.test(src[i + 2] ?? '')) {
      out.push({ from: i, to: i + 2, cls: 'op' });
      i += 2; afterDot = true;
      continue;
    }
    if (c === '.') {
      out.push({ from: i, to: i + 1, cls: 'punct' });
      i++; afterDot = true;
      continue;
    }
    if (c === '|' && src[i + 1] !== '|') {
      out.push({ from: i, to: i + 1, cls: 'op' });
      i++; pipePending = true;
      continue;
    }
    OPERATOR.lastIndex = i;
    const op = OPERATOR.exec(src);
    if (op && op.index === i) {
      // Stop a run at a pipe bar, so `a ||b | pipe` keeps `||` and the single `|` apart.
      let len = op[0].length;
      const bar = op[0].search(/(?<!\|)\|(?!\|)/);
      if (bar > 0) len = bar;
      out.push({ from: i, to: i + len, cls: 'op' });
      i += len; afterDot = false;
      continue;
    }
    out.push({ from: i, to: i + 1, cls: 'punct' });
    i++; afterDot = false;
  }
}

/** Find the `}}` closing the interpolation opened at `open`, skipping strings; -1 if it never closes. */
function interpolationEnd(src: string, open: number, limit: number): number {
  for (let j = open + 2; j < limit; j++) {
    const c = src[j];
    if (c === '"' || c === "'" || c === '`') j = stringEnd(src, j, limit) - 1;
    else if (c === '}' && src[j + 1] === '}') return j;
  }
  return -1;
}

/** Colour every `{{ … }}` inside `[from, to)`. */
function scanInterpolations(src: string, from: number, to: number, out: NgSpan[]): void {
  let i = src.indexOf('{{', from);
  while (i !== -1 && i < to) {
    const close = interpolationEnd(src, i, to);
    if (close === -1) return;
    out.push({ from: i, to: i + 2, cls: 'punct' });
    scanExpr(src, i + 2, close, 'expr', out);
    out.push({ from: close, to: close + 2, cls: 'punct' });
    i = src.indexOf('{{', close + 2);
  }
}

/** Where a block starts: the `@` must not sit inside a word (`me@if.com`). */
function atBlockStart(src: string, i: number): boolean {
  return i === 0 || /[\s}>{]/.test(src[i - 1]);
}

/**
 * Try to read a control-flow block header at `i` (an `@`). Returns the index to resume from and
 * whether it opened a `{` block, or null when this is not a block after all.
 */
function scanBlock(src: string, i: number, out: NgSpan[]): { next: number; opened: boolean } | null {
  if (!atBlockStart(src, i)) return null;
  BLOCK_NAME.lastIndex = i;
  const m = BLOCK_NAME.exec(src);
  if (!m) return null;
  const name = m[1];
  const nameEnd = i + m[0].length;
  const header: NgSpan[] = [{ from: i, to: nameEnd, cls: 'kw' }];

  if (name === 'let') {
    const decl = /\s+([A-Za-z_$][\w$]*)\s*=/y;
    decl.lastIndex = nameEnd;
    const d = decl.exec(src);
    if (!d) return null;
    const nameFrom = nameEnd + d[0].indexOf(d[1]);
    header.push({ from: nameFrom, to: nameFrom + d[1].length, cls: 'var' });
    const eq = nameEnd + d[0].length - 1;
    header.push({ from: eq, to: eq + 1, cls: 'op' });
    let end = eq + 1;
    while (end < src.length && src[end] !== ';') {
      const c = src[end];
      end = c === '"' || c === "'" || c === '`' ? stringEnd(src, end, src.length) : end + 1;
    }
    if (src[end] !== ';') return null;
    scanExpr(src, eq + 1, end, 'expr', header);
    header.push({ from: end, to: end + 1, cls: 'punct' });
    out.push(...header);
    return { next: end + 1, opened: false };
  }

  let pos = skipSpace(src, nameEnd);
  if (src[pos] === '(') {
    const close = matchingParen(src, pos);
    if (close === -1) return null;
    header.push({ from: pos, to: pos + 1, cls: 'punct' });
    scanExpr(src, pos + 1, close, name === 'defer' || name === 'placeholder' || name === 'loading' ? 'defer' : 'block', header);
    header.push({ from: close, to: close + 1, cls: 'punct' });
    pos = skipSpace(src, close + 1);
  } else if (NEEDS_PAREN.has(name)) {
    return null;
  }
  if (src[pos] !== '{') return null;
  header.push({ from: pos, to: pos + 1, cls: 'punct' });
  out.push(...header);
  return { next: pos + 1, opened: true };
}

/** Attribute-name shapes that carry an expression, and how the value reads. */
function scanAttributeName(src: string, from: number, to: number, out: NgSpan[]): ExprMode | 'plain' | null {
  const name = src.slice(from, to);
  const wrapped = /^(\[\(|\[|\()(.+?)(\)\]|\]|\))$/.exec(name);
  if (wrapped) {
    const [, open, inner, close] = wrapped;
    const isEvent = open === '(';
    const matched = (open === '[(' && close === ')]') || (open === '[' && close === ']') || (isEvent && close === ')');
    if (!matched) return 'plain';
    out.push({ from, to: from + open.length, cls: 'punct' });
    out.push({ from: from + open.length, to: to - close.length, cls: isEvent ? 'event' : 'bind' });
    out.push({ from: to - close.length, to, cls: 'punct' });
    return isEvent ? 'stmt' : 'expr';
  }
  if (/^\*[A-Za-z]/.test(name)) { out.push({ from, to, cls: 'dir' }); return 'micro'; }
  if (/^#[\w-]+$/.test(name)) { out.push({ from, to, cls: 'ref' }); return null; }
  return 'plain';
}

/** Read the tag opening at `i` (`<name`); returns the index just past its `>`. */
function scanTag(src: string, i: number, out: NgSpan[]): number {
  const nameMatch = /<([A-Za-z][\w:-]*)/y;
  nameMatch.lastIndex = i;
  const tag = nameMatch.exec(src)!;
  let pos = i + tag[0].length;
  const n = src.length;
  while (pos < n) {
    pos = skipSpace(src, pos);
    if (src[pos] === '>') { pos++; break; }
    if (src[pos] === '/' && src[pos + 1] === '>') { pos += 2; break; }
    const nameRe = /[^\s=>/"']+/y;
    nameRe.lastIndex = pos;
    const attr = nameRe.exec(src);
    if (!attr) { pos++; continue; }
    const nameFrom = pos;
    pos += attr[0].length;
    const mode = scanAttributeName(src, nameFrom, pos, out);
    const afterName = skipSpace(src, pos);
    if (src[afterName] !== '=') continue;
    const valueStart = skipSpace(src, afterName + 1);
    const quote = src[valueStart];
    if (quote !== '"' && quote !== "'") {
      pos = valueStart;
      continue;
    }
    const close = src.indexOf(quote, valueStart + 1);
    if (close === -1) return n; // an unterminated value: leave the rest of the file alone
    if (mode === 'plain') scanInterpolations(src, valueStart + 1, close, out);
    else if (mode) scanExpr(src, valueStart + 1, close, mode, out);
    pos = close + 1;
  }
  const lower = tag[1].toLowerCase();
  if (lower === 'script' || lower === 'style') {
    const end = src.toLowerCase().indexOf(`</${lower}`, pos);
    return end === -1 ? n : end;
  }
  return pos;
}

/** Every Angular-specific span in `src`, in document order. */
export function tokenizeAngular(src: string): NgSpan[] {
  if (src.length > MAX_SCAN_LENGTH) return [];
  const out: NgSpan[] = [];
  const n = src.length;
  let depth = 0;
  let i = 0;
  while (i < n) {
    const c = src[i];
    if (c === '<') {
      if (src.startsWith('<!--', i)) {
        const end = src.indexOf('-->', i + 4);
        i = end === -1 ? n : end + 3;
      } else if (/[A-Za-z]/.test(src[i + 1] ?? '')) {
        i = scanTag(src, i, out);
      } else i++;
    } else if (c === '{' && src[i + 1] === '{') {
      const close = interpolationEnd(src, i, n);
      if (close === -1) { i += 2; continue; }
      scanInterpolations(src, i, close + 2, out);
      i = close + 2;
    } else if (c === '@') {
      const block = scanBlock(src, i, out);
      if (block) { if (block.opened) depth++; i = block.next; } else i++;
    } else if (c === '}' && depth > 0) {
      out.push({ from: i, to: i + 1, cls: 'punct' });
      depth--; i++;
    } else i++;
  }
  return out;
}

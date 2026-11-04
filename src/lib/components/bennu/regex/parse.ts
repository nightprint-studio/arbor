/**
 * A regex parser for three dialects (Java, JavaScript, Rust/PCRE) producing one positioned AST.
 *
 * It is written for *reading* a pattern — highlighting it, explaining it, diagnosing it and running
 * it — rather than for compiling one, so it never gives up: a problem becomes a diagnostic with the
 * offending span and the parse carries on, which is what lets the editor colour half a pattern that
 * is still being typed. Messages follow `java.util.regex.PatternSyntaxException`'s wording where
 * Java has one, because that is what the programmer would otherwise meet at run time.
 *
 * What differs by dialect is gated on `flavour` at the few places it matters (possessive
 * quantifiers, atomic groups, look-behind, inline flags, `\Q…\E`, named-group syntax, anchors); the
 * grammar itself is the common core.
 */

import { propertyTester } from './charsets';
import {
  NO_FLAGS,
  type AnchorKind, type ClassItem, type ClassKind, type ClassNode, type FlagSet, type Flavour,
  type GroupKind, type ParsedRegex, type RegexDiagnostic, type RegexNode, type Span,
} from './types';

const SHORTS = 'dDwWsShHvV';
const SIMPLE_ESCAPES: Record<string, number> = { t: 9, n: 10, r: 13, f: 12, a: 7, e: 27 };
const JAVA_NAME = /^[A-Za-z][A-Za-z0-9]*$/;
const JS_NAME = /^[A-Za-z_$][\w$]*$/;
const FLAG_LETTERS = 'idmsuxU';

/** The flags a flavour's flag string switches on. */
export function flagsFromString(flags: string, flavour: Flavour): FlagSet {
  const f = { ...NO_FLAGS };
  for (const c of flags) {
    if (c === 'i') f.i = true;
    else if (c === 'm') f.m = true;
    else if (c === 's') f.s = true;
    else if (c === 'x') f.x = true;
    else if (c === 'u') { f.u = true; if (flavour !== 'java') f.U = true; }
    else if (c === 'U' && flavour !== 'js') f.U = true;
    else if (c === 'd' && flavour === 'java') f.d = true;
  }
  return f;
}

class Parser {
  pos = 0;
  fl: FlagSet;
  groupCount = 0;
  readonly names: (string | null)[] = [null];
  readonly errors: RegexDiagnostic[] = [];
  private readonly refs: { node: Extract<RegexNode, { t: 'backref' }> }[] = [];

  constructor(readonly src: string, readonly flavour: Flavour, flags: string) {
    this.fl = flagsFromString(flags, flavour);
  }

  err(from: number, to: number, message: string) {
    this.errors.push({ from, to: Math.max(to, from + 1), message });
  }

  get done() { return this.pos >= this.src.length; }
  /** The character `o` ahead of the cursor, or '' past the end — which compares false to everything. */
  peek(o = 0): string { return this.src[this.pos + o] ?? ''; }
  private cpAtPos() { return this.src.codePointAt(this.pos)!; }

  // ── Structure ───────────────────────────────────────────────────────────────────

  parse(): ParsedRegex {
    let root = this.parseAlt(false);
    // A `)` that closed nothing ends the alternation early: report it, then read on past it.
    if (!this.done) {
      const items: RegexNode[] = [root];
      while (!this.done) {
        this.err(this.pos, this.pos + 1, "Unmatched closing ')'");
        this.pos++;
        items.push(this.parseAlt(false));
      }
      root = { t: 'seq', items, from: 0, to: this.pos };
    }
    this.checkRefs();
    this.checkLookbehinds(root);
    return { root, groupCount: this.groupCount, groupNames: this.names, errors: this.errors, flavour: this.flavour };
  }

  /** Branches separated by `|`, up to the `)` that closes the enclosing group (or the end). */
  private parseAlt(inGroup: boolean): RegexNode {
    const start = this.pos;
    const branches: RegexNode[] = [];
    const pipes: number[] = [];
    for (;;) {
      branches.push(this.parseSeq());
      if (this.peek() === '|') { pipes.push(this.pos); this.pos++; continue; }
      break;
    }
    void inGroup;
    if (branches.length === 1) return branches[0];
    return { t: 'alt', branches, pipes, from: start, to: this.pos };
  }

  private parseSeq(): RegexNode {
    const start = this.pos;
    const items: RegexNode[] = [];
    while (!this.done && this.peek() !== '|' && this.peek() !== ')') {
      if (this.fl.x && this.skipFreeSpacing(items)) continue;
      const atom = this.parseAtom();
      if (!atom) continue;
      items.push(this.parseQuantifiers(atom));
    }
    return { t: 'seq', items, from: start, to: this.pos };
  }

  /** In free-spacing mode, whitespace and `#…` comments are not part of the pattern. */
  private skipFreeSpacing(items: RegexNode[]): boolean {
    const c = this.peek();
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r' || c === '\f' || c === '\v') { this.pos++; return true; }
    if (c === '#') {
      const from = this.pos;
      while (!this.done && this.peek() !== '\n') this.pos++;
      items.push({ t: 'comment', from, to: this.pos });
      return true;
    }
    return false;
  }

  // ── Quantifiers ─────────────────────────────────────────────────────────────────

  private parseQuantifiers(atom: RegexNode): RegexNode {
    const q = this.readQuantifier();
    if (!q) return atom;
    const tokenStart = q.from;
    let mode: 'greedy' | 'lazy' | 'possessive' = 'greedy';
    if (this.peek() === '?') { mode = 'lazy'; this.pos++; }
    else if (this.peek() === '+') {
      if (this.flavour === 'java') { mode = 'possessive'; this.pos++; }
      else this.err(this.pos, this.pos + 1, this.flavour === 'js' ? 'Nothing to repeat' : 'Possessive quantifiers are not supported');
    }
    if (atom.t === 'anchor' && (atom.kind === '^' || atom.kind === '$') && this.flavour === 'js') {
      this.err(atom.from, this.pos, 'Nothing to repeat');
    }
    return {
      t: 'quant', body: atom, min: q.min, max: q.max, mode,
      token: { from: tokenStart, to: this.pos }, from: atom.from, to: this.pos,
    };
  }

  /** `*`, `+`, `?` or a valid `{n,m}` at the cursor; consumes it. */
  private readQuantifier(): { min: number; max: number; from: number } | null {
    const c = this.peek();
    const from = this.pos;
    if (c === '*') { this.pos++; return { min: 0, max: Infinity, from }; }
    if (c === '+') { this.pos++; return { min: 1, max: Infinity, from }; }
    if (c === '?') { this.pos++; return { min: 0, max: 1, from }; }
    if (c !== '{') return null;
    const m = /^\{(\d+)(?:(,)(\d*))?\}/.exec(this.src.slice(this.pos, this.pos + 40));
    if (!m) return null;
    const min = Number(m[1]);
    const max = m[2] ? (m[3] === '' ? Infinity : Number(m[3])) : min;
    this.pos += m[0].length;
    if (max < min) this.err(from, this.pos, 'Illegal repetition range');
    return { min, max, from };
  }

  // ── Atoms ───────────────────────────────────────────────────────────────────────

  private parseAtom(): RegexNode | null {
    const from = this.pos;
    const c = this.peek();
    switch (c) {
      case '(': return this.parseGroup();
      case '[': return this.parseClass();
      case '\\': return this.parseEscape();
      case '.': this.pos++; return { t: 'any', from, to: this.pos, fl: this.fl };
      case '^': case '$':
        this.pos++;
        return { t: 'anchor', kind: c as AnchorKind, from, to: this.pos, fl: this.fl };
      case '*': case '+': case '?':
        this.pos++;
        this.err(from, this.pos, `Dangling meta character '${c}'`);
        return { t: 'lit', cp: c.charCodeAt(0), raw: c, from, to: this.pos, fl: this.fl };
      case '{': {
        const save = this.pos;
        if (this.readQuantifier()) {
          this.err(from, this.pos, "Dangling meta character '{'");
          return { t: 'lit', cp: 123, raw: this.src.slice(from, this.pos), from, to: this.pos, fl: this.fl };
        }
        this.pos = save + 1;
        if (this.flavour !== 'js') this.err(from, this.pos, 'Illegal repetition');
        return { t: 'lit', cp: 123, raw: '{', from, to: this.pos, fl: this.fl };
      }
      default: {
        const cp = this.cpAtPos();
        const raw = String.fromCodePoint(cp);
        this.pos += raw.length;
        return { t: 'lit', cp, raw, from, to: this.pos, fl: this.fl };
      }
    }
  }

  // ── Groups ──────────────────────────────────────────────────────────────────────

  private parseGroup(): RegexNode {
    const from = this.pos;
    this.pos++; // (
    const saved = this.fl;
    let kind: GroupKind = 'capture';
    let name: string | null = null;
    let on: string | undefined;
    let off: string | undefined;

    if (this.peek() === '?') {
      const r = this.readGroupPrefix(from);
      if (r.kind === 'flags') {
        // `(?i)` — the flags apply from here to the end of the enclosing group.
        this.fl = this.applyFlags(r.on ?? '', r.off ?? '');
        return { t: 'flags', on: r.on ?? '', off: r.off ?? '', from, to: this.pos };
      }
      kind = r.kind;
      name = r.name ?? null;
      on = r.on; off = r.off;
    }

    const open: Span = { from, to: this.pos };
    let index: number | null = null;
    if (kind === 'capture' || kind === 'named') {
      index = ++this.groupCount;
      this.names[index] = name;
    }
    if (kind === 'flagged') this.fl = this.applyFlags(on ?? '', off ?? '');

    const body = this.parseAlt(true);
    let close: Span | null = null;
    if (this.peek() === ')') { close = { from: this.pos, to: this.pos + 1 }; this.pos++; }
    else this.err(from, from + 1, 'Unclosed group');
    this.fl = saved;

    return { t: 'group', kind, index, name, open, close, body, on, off, from, to: this.pos };
  }

  /** What follows `(?`. Leaves the cursor after the prefix (and, for flags, after the `)`). */
  private readGroupPrefix(groupStart: number): {
    kind: GroupKind | 'flags'; name?: string; on?: string; off?: string;
  } {
    this.pos++; // ?
    const c = this.peek();
    const j = this.flavour;
    const gate = (feature: string, ok: boolean) => {
      if (!ok) this.err(groupStart, this.pos + 1, `${feature} is not supported in ${j === 'js' ? 'JavaScript' : 'Rust regex'}`);
    };
    if (c === ':') { this.pos++; return { kind: 'noncapture' }; }
    if (c === '=') { this.pos++; gate('Look-ahead', j !== 'rust'); return { kind: 'lookahead' }; }
    if (c === '!') { this.pos++; gate('Look-ahead', j !== 'rust'); return { kind: 'neglookahead' }; }
    if (c === '>') { this.pos++; gate('Atomic groups', j === 'java'); return { kind: 'atomic' }; }
    if (c === '<' && (this.peek(1) === '=' || this.peek(1) === '!')) {
      const neg = this.peek(1) === '!';
      this.pos += 2;
      gate('Look-behind', j !== 'rust');
      return { kind: neg ? 'neglookbehind' : 'lookbehind' };
    }
    if (c === '<' || (c === 'P' && this.peek(1) === '<')) {
      if (c === 'P' && j !== 'rust') this.err(groupStart, this.pos + 2, 'Unknown group type: (?P<name>…) is Rust/PCRE syntax');
      this.pos += c === 'P' ? 2 : 1;
      return { kind: 'named', name: this.readGroupName(groupStart) };
    }
    if (c === '#') {
      // A PCRE comment group: swallow it so it never becomes a capture.
      const end = this.src.indexOf(')', this.pos);
      this.pos = end < 0 ? this.src.length : end;
      return { kind: 'noncapture' };
    }
    // Inline flags: `(?i)` or `(?i-s:…)`.
    const m = /^([idmsuxU]*)(?:-([idmsuxU]*))?([:)])/.exec(this.src.slice(this.pos, this.pos + 16));
    if (m) {
      this.pos += m[0].length;
      if (j === 'js') this.err(groupStart, this.pos, 'Inline flags are not supported in JavaScript');
      for (const f of m[1] + (m[2] ?? '')) {
        if (j === 'rust' && (f === 'd')) this.err(groupStart, this.pos, `Unknown flag '${f}'`);
        if (!FLAG_LETTERS.includes(f)) this.err(groupStart, this.pos, `Unknown inline modifier '${f}'`);
      }
      if (m[3] === ')') return { kind: 'flags', on: m[1], off: m[2] ?? '' };
      return { kind: 'flagged', on: m[1], off: m[2] ?? '' };
    }
    this.err(groupStart, this.pos + 1, `Unknown group type: (?${c ?? ''}`);
    return { kind: 'noncapture' };
  }

  private readGroupName(groupStart: number): string {
    const end = this.src.indexOf('>', this.pos);
    if (end < 0) {
      this.err(groupStart, this.src.length, "Named capturing group is missing trailing '>'");
      const rest = this.src.slice(this.pos);
      this.pos = this.src.length;
      return rest;
    }
    const name = this.src.slice(this.pos, end);
    const ok = this.flavour === 'java' ? JAVA_NAME.test(name) : JS_NAME.test(name);
    if (!ok) {
      this.err(this.pos, end, name === ''
        ? 'Named capturing group needs a name'
        : /^\d/.test(name) ? `Capturing group name ${name} starts with digit character` : `Invalid group name '${name}'`);
    } else if (this.names.includes(name)) {
      this.err(this.pos, end, `Named capturing group <${name}> is already defined`);
    }
    this.pos = end + 1;
    return name;
  }

  private applyFlags(on: string, off: string): FlagSet {
    const next = { ...this.fl };
    for (const f of on) if (f in next) (next as unknown as Record<string, boolean>)[f] = true;
    for (const f of off) if (f in next) (next as unknown as Record<string, boolean>)[f] = false;
    return next;
  }

  // ── Escapes ─────────────────────────────────────────────────────────────────────

  private parseEscape(): RegexNode {
    const from = this.pos;
    this.pos++; // backslash
    if (this.done) {
      this.err(from, from + 1, this.flavour === 'js' ? '\\ at end of pattern' : 'Unexpected internal error since pattern is malformed (trailing backslash)');
      return { t: 'lit', cp: 92, raw: '\\', from, to: this.pos, fl: this.fl };
    }
    const c = this.peek();
    const lit = (cp: number): RegexNode => ({ t: 'lit', cp, raw: this.src.slice(from, this.pos), from, to: this.pos, fl: this.fl });

    if (SHORTS.includes(c)) return this.shortEscape(from, c as ClassKind);
    if (c === 'b' || c === 'B') return this.anchorEscape(from, c);
    if (c === 'A' || c === 'Z' || c === 'z' || c === 'G') return this.anchorEscape(from, c);
    if (c === 'R' || c === 'X') {
      this.pos++;
      if (this.flavour !== 'java' || c === 'X') this.err(from, this.pos, `Unsupported escape sequence: \\${c}`);
      return { t: 'linebreak', from, to: this.pos };
    }
    if (c === 'Q' || c === 'E') return this.quoteEscape(from, c);
    if (c === 'p' || c === 'P') return this.propertyEscape(from, c === 'P');
    if (c === 'k') return this.namedRef(from);
    if (c >= '1' && c <= '9') return this.numberedRef(from);
    const cp = this.readCharEscape(from);
    if (cp !== null) return lit(cp);
    return lit(this.cpAtPos());
  }

  private shortEscape(from: number, kind: ClassKind): RegexNode {
    this.pos++;
    if (this.flavour === 'js' && (kind === 'h' || kind === 'H')) this.err(from, this.pos, `Unsupported escape sequence: \\${kind}`);
    if (this.flavour === 'js' && (kind === 'v')) {
      return { t: 'lit', cp: 11, raw: '\\v', from, to: this.pos, fl: this.fl };
    }
    if (this.flavour === 'rust' && 'hHvV'.includes(kind)) this.err(from, this.pos, `Unsupported escape sequence: \\${kind}`);
    return { t: 'short', kind, from, to: this.pos, fl: this.fl };
  }

  private anchorEscape(from: number, c: string): RegexNode {
    this.pos++;
    const allowed: Record<Flavour, string> = { java: 'bBAZzG', js: 'bB', rust: 'bBAz' };
    if (!allowed[this.flavour].includes(c)) this.err(from, this.pos, `Unsupported escape sequence: \\${c}`);
    return { t: 'anchor', kind: c as AnchorKind, from, to: this.pos, fl: this.fl };
  }

  private quoteEscape(from: number, c: string): RegexNode {
    this.pos++;
    if (this.flavour !== 'java') this.err(from, this.pos, `\\${c} quoting is only supported in Java`);
    if (c === 'E') return { t: 'comment', from, to: this.pos };
    const end = this.src.indexOf('\\E', this.pos);
    const text = this.src.slice(this.pos, end < 0 ? this.src.length : end);
    this.pos = end < 0 ? this.src.length : end + 2;
    return { t: 'quote', text, from, to: this.pos, fl: this.fl };
  }

  private propertyEscape(from: number, negate: boolean): RegexNode {
    this.pos++; // p / P
    const r = this.readPropertyName(from);
    const neg = r.negate ? !negate : negate;
    return { t: 'prop', name: r.name, negate: neg, from, to: this.pos, fl: this.fl };
  }

  private readPropertyName(from: number): { name: string; negate: boolean } {
    let name: string;
    let negate = false;
    if (this.peek() === '{') {
      const end = this.src.indexOf('}', this.pos);
      if (end < 0) {
        this.err(from, this.src.length, 'Unclosed character family');
        name = this.src.slice(this.pos + 1);
        this.pos = this.src.length;
      } else {
        name = this.src.slice(this.pos + 1, end);
        this.pos = end + 1;
      }
      if (name.startsWith('^')) { negate = true; name = name.slice(1); }
    } else if (this.done) {
      this.err(from, this.pos, 'Illegal character property');
      name = '';
    } else {
      name = this.peek();
      this.pos++;
    }
    const eq = name.indexOf('=');
    const lookup = eq >= 0 ? name.slice(eq + 1) : name;
    if (!propertyTester(lookup)) this.err(from, this.pos, `Unknown character property name {${name}}`);
    return { name, negate };
  }

  private namedRef(from: number): RegexNode {
    this.pos++; // k
    if (this.flavour === 'rust') this.err(from, this.pos, 'Back-references are not supported in Rust regex');
    if (this.peek() !== '<') {
      this.err(from, this.pos, "\\k is not followed by '<' for named capturing group");
      return { t: 'lit', cp: 107, raw: '\\k', from, to: this.pos, fl: this.fl };
    }
    const end = this.src.indexOf('>', this.pos);
    const name = this.src.slice(this.pos + 1, end < 0 ? this.src.length : end);
    this.pos = end < 0 ? this.src.length : end + 1;
    if (end < 0) this.err(from, this.pos, "Named capturing group is missing trailing '>'");
    const node = { t: 'backref' as const, index: null, name, from, to: this.pos, fl: this.fl };
    this.refs.push({ node });
    return node;
  }

  private numberedRef(from: number): RegexNode {
    // Java reads further digits only while they still name an existing group.
    let n = Number(this.peek());
    this.pos++;
    while (this.peek() >= '0' && this.peek() <= '9') {
      const next = n * 10 + Number(this.peek());
      if (this.flavour === 'java' && next > this.groupCount) break;
      n = next;
      this.pos++;
    }
    if (this.flavour === 'rust') this.err(from, this.pos, 'Back-references are not supported in Rust regex');
    const node = { t: 'backref' as const, index: n, name: null, from, to: this.pos, fl: this.fl };
    this.refs.push({ node });
    return node;
  }

  /** The single code point a character escape stands for, or `null` when `\c` is not one.
   *  The cursor is on the escape letter; it is left after the escape. */
  private readCharEscape(from: number): number | null {
    const c = this.peek();
    if (c in SIMPLE_ESCAPES) {
      this.pos++;
      if (this.flavour !== 'java' && (c === 'a' || c === 'e') ) {
        if (this.flavour === 'rust' && c === 'a') return 7;
        if (this.flavour === 'js') return c.charCodeAt(0);
      }
      return SIMPLE_ESCAPES[c];
    }
    if (c === '0') return this.octalEscape(from);
    if (c === 'x') return this.hexEscape(from);
    if (c === 'u') return this.unicodeEscape(from);
    if (c === 'c') {
      this.pos++;
      if (this.done) { this.err(from, this.pos, 'Illegal control escape sequence'); return 99; }
      const ch = this.peek();
      this.pos++;
      return ch.charCodeAt(0) ^ 64;
    }
    if (/[A-Za-z]/.test(c)) {
      this.pos++;
      if (this.flavour !== 'js') this.err(from, this.pos, 'Illegal/unsupported escape sequence');
      return c.charCodeAt(0);
    }
    const cp = this.cpAtPos();
    this.pos += String.fromCodePoint(cp).length;
    return cp;
  }

  private octalEscape(from: number): number {
    this.pos++; // 0
    const m = /^[0-7]{1,3}/.exec(this.src.slice(this.pos, this.pos + 3));
    if (!m) {
      if (this.flavour === 'java') this.err(from, this.pos, 'Illegal octal escape sequence');
      return 0;
    }
    let digits = m[0];
    if (digits.length === 3 && digits[0] > '3') digits = digits.slice(0, 2);
    this.pos += digits.length;
    return parseInt(digits, 8);
  }

  private hexEscape(from: number): number {
    this.pos++; // x
    if (this.peek() === '{') {
      const end = this.src.indexOf('}', this.pos);
      const hex = this.src.slice(this.pos + 1, end < 0 ? this.pos + 1 : end);
      if (end < 0 || !/^[0-9a-fA-F]{1,6}$/.test(hex) || parseInt(hex, 16) > 0x10ffff) {
        this.err(from, this.pos + 1, 'Illegal hexadecimal escape sequence');
        this.pos = end < 0 ? this.pos + 1 : end + 1;
        return 0;
      }
      this.pos = end + 1;
      return parseInt(hex, 16);
    }
    const hex = this.src.slice(this.pos, this.pos + 2);
    if (!/^[0-9a-fA-F]{2}$/.test(hex)) {
      this.err(from, this.pos + 1, 'Illegal hexadecimal escape sequence');
      return 0;
    }
    this.pos += 2;
    return parseInt(hex, 16);
  }

  private unicodeEscape(from: number): number {
    this.pos++; // u
    if (this.peek() === '{') {
      const end = this.src.indexOf('}', this.pos);
      const hex = end < 0 ? '' : this.src.slice(this.pos + 1, end);
      if (!/^[0-9a-fA-F]{1,6}$/.test(hex) || parseInt(hex, 16) > 0x10ffff) {
        this.err(from, this.pos + 1, 'Illegal Unicode escape sequence');
        this.pos = end < 0 ? this.pos + 1 : end + 1;
        return 0;
      }
      this.pos = end + 1;
      return parseInt(hex, 16);
    }
    const hex = this.src.slice(this.pos, this.pos + 4);
    if (!/^[0-9a-fA-F]{4}$/.test(hex)) {
      this.err(from, this.pos + 1, 'Illegal Unicode escape sequence');
      return 0;
    }
    this.pos += 4;
    let cp = parseInt(hex, 16);
    // A surrogate pair written as two escapes is one character.
    if (cp >= 0xd800 && cp <= 0xdbff) {
      const low = /^\\u([dD][c-fC-F][0-9a-fA-F]{2})/.exec(this.src.slice(this.pos, this.pos + 6));
      if (low) { cp = 0x10000 + ((cp - 0xd800) << 10) + (parseInt(low[1], 16) - 0xdc00); this.pos += 6; }
    }
    return cp;
  }

  // ── Character classes ───────────────────────────────────────────────────────────

  private parseClass(): ClassNode {
    const from = this.pos;
    this.pos++; // [
    let negate = false;
    if (this.peek() === '^') { negate = true; this.pos++; }
    const operands: ClassItem[][] = [[]];
    const cur = () => operands[operands.length - 1];
    let first = true;
    let closed = false;

    while (!this.done) {
      const c = this.peek();
      if (c === ']' && !(first && this.flavour !== 'js')) { this.pos++; closed = true; break; }
      first = false;
      if (c === '&' && this.peek(1) === '&' && this.flavour !== 'js') {
        this.pos += 2;
        operands.push([]);
        continue;
      }
      if (c === '[' && this.flavour !== 'js') {
        const nested = this.parseClass();
        cur().push({ k: 'nested', cls: nested, from: nested.from, to: nested.to });
        continue;
      }
      if (this.fl.x && /\s/.test(c)) { this.pos++; continue; }
      const item = this.parseClassAtom();
      if (!item) continue;
      cur().push(this.maybeRange(item));
    }
    if (!closed) this.err(from, from + 1, 'Unclosed character class');
    return { t: 'class', negate, operands, from, to: this.pos, fl: this.fl };
  }

  /** `a-z` — if `lo` is followed by `-` and another single character. */
  private maybeRange(lo: ClassItem): ClassItem {
    if (lo.k !== 'char' || this.peek() !== '-' || this.peek(1) === ']' || this.peek(1) === '') return lo;
    const dash = this.pos;
    const save = this.pos;
    this.pos++;
    if (this.peek() === '[' && this.flavour !== 'js') { this.pos = save; return lo; }
    const hi = this.parseClassAtom();
    if (!hi) return lo;
    if (hi.k !== 'char') {
      // `[a-\d]`: Java refuses, JS in Unicode mode refuses, and the permissive reading is `a`, `-`, `\d`.
      if (this.flavour !== 'js') this.err(lo.from, hi.to, 'Illegal character range');
      return lo;
    }
    if (hi.cp < lo.cp) this.err(lo.from, hi.to, 'Illegal character range');
    void dash;
    return { k: 'range', lo: lo.cp, hi: hi.cp, from: lo.from, to: hi.to };
  }

  private parseClassAtom(): ClassItem | null {
    const from = this.pos;
    const c = this.peek();
    if (c !== '\\') {
      const cp = this.cpAtPos();
      this.pos += String.fromCodePoint(cp).length;
      return { k: 'char', cp, from, to: this.pos };
    }
    this.pos++;
    if (this.done) { this.err(from, this.pos, 'Unclosed character class'); return null; }
    const e = this.peek();
    if (SHORTS.includes(e)) {
      this.pos++;
      if (this.flavour === 'js' && e === 'v') return { k: 'char', cp: 11, from, to: this.pos };
      return { k: 'short', kind: e as ClassKind, from, to: this.pos };
    }
    if (e === 'p' || e === 'P') {
      this.pos++;
      const r = this.readPropertyName(from);
      return { k: 'prop', name: r.name, negate: r.negate ? e === 'p' : e === 'P', from, to: this.pos };
    }
    if (e === 'b' && this.flavour === 'js') { this.pos++; return { k: 'char', cp: 8, from, to: this.pos }; }
    if (e === 'Q' && this.flavour === 'java') {
      const q = this.quoteEscape(from, 'Q');
      // A quoted run inside a class is a set of characters; the first one stands in for the item.
      const text = q.t === 'quote' ? q.text : '';
      return text ? { k: 'char', cp: text.codePointAt(0)!, from, to: this.pos } : null;
    }
    if (e === 'E' && this.flavour === 'java') { this.pos++; return null; }
    if (e >= '1' && e <= '9') { this.pos++; return { k: 'char', cp: e.charCodeAt(0), from, to: this.pos }; }
    const cp = this.readCharEscape(from);
    return { k: 'char', cp: cp ?? e.charCodeAt(0), from, to: this.pos };
  }

  // ── Whole-pattern checks ────────────────────────────────────────────────────────

  private checkRefs() {
    for (const { node } of this.refs) {
      if (node.name !== null && !this.names.includes(node.name)) {
        this.err(node.from, node.to, `Named capturing group <${node.name}> does not exist`);
      } else if (node.index !== null && node.index > this.groupCount && this.flavour !== 'java') {
        this.err(node.from, node.to, `Reference to group ${node.index} that does not exist`);
      }
    }
  }

  private checkLookbehinds(root: RegexNode) {
    if (this.flavour !== 'java') return;
    walk(root, (n) => {
      if (n.t === 'group' && (n.kind === 'lookbehind' || n.kind === 'neglookbehind') && !Number.isFinite(maxLength(n.body))) {
        this.err(n.from, n.open.to, 'Look-behind group does not have an obvious maximum length');
      }
    });
  }
}

/** Visit a node and everything under it. */
export function walk(node: RegexNode, visit: (n: RegexNode) => void): void {
  visit(node);
  switch (node.t) {
    case 'group': walk(node.body, visit); break;
    case 'quant': walk(node.body, visit); break;
    case 'alt': node.branches.forEach((b) => walk(b, visit)); break;
    case 'seq': node.items.forEach((b) => walk(b, visit)); break;
    default: break;
  }
}

/** The longest text a node can match, in UTF-16 units (`Infinity` when unbounded). */
export function maxLength(node: RegexNode): number {
  switch (node.t) {
    case 'lit': return node.cp > 0xffff ? 2 : 1;
    case 'quote': return node.text.length;
    case 'any': case 'class': case 'short': case 'prop': return 2;
    case 'linebreak': return 2;
    case 'backref': return Infinity;
    case 'anchor': case 'flags': case 'comment': return 0;
    case 'group':
      return node.kind === 'lookahead' || node.kind === 'neglookahead' || node.kind === 'lookbehind' || node.kind === 'neglookbehind'
        ? 0 : maxLength(node.body);
    case 'quant': {
      const inner = maxLength(node.body);
      return inner === 0 ? 0 : node.max * inner;
    }
    case 'alt': return Math.max(...node.branches.map(maxLength));
    case 'seq': return node.items.reduce((a, b) => a + maxLength(b), 0);
  }
}

/** Parse `pattern` as `flavour`, with the flavour's flag letters `flags` switched on. */
export function parseRegex(pattern: string, flavour: Flavour, flags = ''): ParsedRegex {
  return new Parser(pattern, flavour, flags).parse();
}

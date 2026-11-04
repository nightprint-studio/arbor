/**
 * A backtracking matcher over the {@link parseRegex} AST, following `java.util.regex` semantics:
 * leftmost match, greedy / lazy / possessive quantifiers, atomic groups, look-around (bounded
 * look-behind), back-references, the inline-flag scoping rules and Java's definition of a line
 * terminator for `.`, `^` and `$`.
 *
 * It exists so a Java pattern can be tried without a JVM and without a crate that approximates
 * Java's dialect: Rust's `regex` has no look-around or back-references, JavaScript's engine reads
 * `\Q…\E`, possessive quantifiers and `(?>…)` differently. The same engine serves the Rust/PCRE
 * flavour (parsed under that flavour's stricter rules); the JavaScript flavour runs on the host's
 * own `RegExp` instead (see `run.ts`), which *is* the reference for it.
 *
 * Pure and synchronous. A catastrophic pattern is bounded two ways: a step budget here, and the
 * caller running it in a worker it can terminate.
 */

import { fold, isLineTerminator, isWordChar, propertyTester, shortTester } from './charsets';
import { maxLength } from './parse';
import type {
  ClassItem, ClassNode, FlagSet, GroupMatch, MatchMode, ParsedRegex, RegexMatch, RegexNode,
} from './types';

type K = (i: number) => boolean;
type M = (i: number, k: K) => boolean;
type Pred = (cp: number) => boolean;

/** Thrown when the step budget runs out. */
class StepLimit extends Error {}

interface Ctx {
  s: string;
  /** Start/end per group, flat: `[start0, end0, start1, end1, …]`; −1 for unset. */
  caps: number[];
  steps: number;
  limit: number;
  /** Where the previous match ended — what `\G` anchors to. */
  lastEnd: number;
}

export interface ExecOptions {
  maxMatches: number;
  /** Matcher steps before giving up. */
  stepLimit: number;
}

export interface ExecResult {
  matches: RegexMatch[];
  aborted: boolean;
  truncated: boolean;
}

const DEFAULT_STEPS = 5_000_000;

/** Every match of the pattern in `text`, under `mode`. */
export function executeRegex(
  parsed: ParsedRegex,
  text: string,
  mode: MatchMode,
  opts: Partial<ExecOptions> = {},
): ExecResult {
  const ctx: Ctx = { s: text, caps: [], steps: 0, limit: opts.stepLimit ?? DEFAULT_STEPS, lastEnd: 0 };
  const root = compile(parsed.root, ctx, parsed);
  const maxMatches = opts.maxMatches ?? 10_000;
  const matches: RegexMatch[] = [];
  const nCaps = (parsed.groupCount + 1) * 2;
  const n = text.length;
  let truncated = false;
  let aborted = false;

  const attempt = (start: number): number => {
    ctx.caps = new Array<number>(nCaps).fill(-1);
    let end = -1;
    const ok = root(start, (j) => {
      if (mode === 'matches' && j !== n) return false;
      end = j;
      return true;
    });
    return ok ? end : -1;
  };

  try {
    let from = 0;
    while (from <= n) {
      let found = -1;
      let start = from;
      for (; start <= n; start++) {
        found = attempt(start);
        if (found >= 0 || mode !== 'find') break;
      }
      if (found < 0) break;
      ctx.caps[0] = start;
      ctx.caps[1] = found;
      matches.push(toMatch(ctx.caps, text));
      if (matches.length >= maxMatches) { truncated = true; break; }
      if (mode !== 'find') break;
      ctx.lastEnd = found;
      // After an empty match the next search starts one code point on, or it would find the same one.
      from = found === start ? found + (isHighSurrogateAt(text, found) ? 2 : 1) : found;
    }
  } catch (e) {
    if (e instanceof StepLimit) aborted = true;
    else throw e;
  }
  return { matches, aborted, truncated };
}

function isHighSurrogateAt(s: string, i: number): boolean {
  const c = s.charCodeAt(i);
  return c >= 0xd800 && c <= 0xdbff && i + 1 < s.length;
}

function toMatch(caps: number[], text: string): RegexMatch {
  const groups: (GroupMatch | null)[] = [];
  for (let g = 0; g * 2 < caps.length; g++) {
    const a = caps[g * 2];
    const b = caps[g * 2 + 1];
    groups.push(a >= 0 && b >= 0 ? { start: a, end: b, text: text.slice(a, b) } : null);
  }
  return { start: caps[0], end: caps[1], groups };
}

// ── Compilation ───────────────────────────────────────────────────────────────────

const tick = (ctx: Ctx) => { if (++ctx.steps > ctx.limit) throw new StepLimit(); };

function compile(node: RegexNode, ctx: Ctx, parsed: ParsedRegex): M {
  switch (node.t) {
    case 'seq': return compileSeq(node.items.map((n) => compile(n, ctx, parsed)));
    case 'alt': {
      const branches = node.branches.map((b) => compile(b, ctx, parsed));
      return (i, k) => { for (const b of branches) if (b(i, k)) return true; return false; };
    }
    case 'comment': case 'flags': return (i, k) => k(i);
    case 'group': return compileGroup(node, ctx, parsed);
    case 'quant': return compileQuant(node, ctx, parsed);
    case 'anchor': return compileAnchor(node.kind, node.fl, ctx);
    case 'backref': return compileBackref(node, ctx, parsed);
    case 'linebreak':
      return (i, k) => {
        const s = ctx.s;
        if (s[i] === '\r' && s[i + 1] === '\n') return k(i + 2);
        const c = s.charCodeAt(i);
        return (c >= 10 && c <= 13) || c === 0x85 || c === 0x2028 || c === 0x2029 ? k(i + 1) : false;
      };
    case 'quote': {
      const cps = [...node.text].map((ch) => ch.codePointAt(0)!);
      const steps = cps.map((cp) => singleStep(ctx, literalPred(cp, node.fl)));
      return compileSeq(steps.map((st) => (i, k) => { const j = st(i); return j >= 0 && k(j); }));
    }
    default: {
      const step = singleStep(ctx, predicateOf(node));
      return (i, k) => { const j = step(i); return j >= 0 && k(j); };
    }
  }
}

function compileSeq(items: M[]): M {
  if (items.length === 0) return (i, k) => k(i);
  if (items.length === 1) return items[0];
  return items.reduceRight<M>((next, item) => (i, k) => item(i, (j) => next(j, k)));
}

/** A node that consumes exactly one code point, as a predicate. */
function predicateOf(node: RegexNode): Pred {
  switch (node.t) {
    case 'lit': return literalPred(node.cp, node.fl);
    case 'any': return (cp) => node.fl.s || !isLineTerminator(cp, node.fl);
    case 'short': return shortTester(node.kind, node.fl);
    case 'prop': {
      const test = propertyTester(node.name.includes('=') ? node.name.slice(node.name.indexOf('=') + 1) : node.name);
      return (cp) => (test ? test(cp) !== node.negate : false);
    }
    case 'class': return classPred(node);
    default: return () => false;
  }
}

function literalPred(cp: number, fl: FlagSet): Pred {
  if (!fl.i) return (c) => c === cp;
  const want = fold(cp, fl);
  return (c) => c === cp || fold(c, fl) === want;
}

function classPred(node: ClassNode): Pred {
  const operands = node.operands.map((items) => unionPred(items, node.fl));
  const all: Pred = (cp) => operands.every((p) => p(cp));
  const withCase: Pred = node.fl.i
    ? (cp) => all(cp) || caseVariants(cp, node.fl).some(all)
    : all;
  return node.negate ? (cp) => !withCase(cp) : withCase;
}

function caseVariants(cp: number, fl: FlagSet): number[] {
  if (cp >= 128 && !fl.u) return [];
  const ch = String.fromCodePoint(cp);
  const out: number[] = [];
  const lo = ch.toLowerCase();
  const up = ch.toUpperCase();
  if (lo.length === ch.length) out.push(lo.codePointAt(0)!);
  if (up.length === ch.length) out.push(up.codePointAt(0)!);
  return out;
}

function unionPred(items: ClassItem[], fl: FlagSet): Pred {
  const preds = items.map((it): Pred => {
    switch (it.k) {
      case 'char': return (cp) => cp === it.cp;
      case 'range': return (cp) => cp >= it.lo && cp <= it.hi;
      case 'short': return shortTester(it.kind, fl);
      case 'prop': {
        const name = it.name.includes('=') ? it.name.slice(it.name.indexOf('=') + 1) : it.name;
        const test = propertyTester(name);
        return (cp) => (test ? test(cp) !== it.negate : false);
      }
      case 'nested': return classPred(it.cls);
    }
  });
  return (cp) => preds.some((p) => p(cp));
}

/** Wrap a predicate as a step: the index after one code point it accepts, or −1. */
function singleStep(ctx: Ctx, pred: Pred): (i: number) => number {
  return (i) => {
    const s = ctx.s;
    if (i >= s.length) return -1;
    const cp = s.codePointAt(i)!;
    return pred(cp) ? i + (cp > 0xffff ? 2 : 1) : -1;
  };
}

function singleOf(node: RegexNode, ctx: Ctx): ((i: number) => number) | null {
  switch (node.t) {
    case 'lit': case 'any': case 'short': case 'prop': case 'class':
      return singleStep(ctx, predicateOf(node));
    default: return null;
  }
}

function compileGroup(node: Extract<RegexNode, { t: 'group' }>, ctx: Ctx, parsed: ParsedRegex): M {
  const body = compile(node.body, ctx, parsed);
  switch (node.kind) {
    case 'capture': case 'named': {
      const g = node.index!;
      return (i, k) => body(i, (j) => {
        const ps = ctx.caps[g * 2];
        const pe = ctx.caps[g * 2 + 1];
        ctx.caps[g * 2] = i;
        ctx.caps[g * 2 + 1] = j;
        if (k(j)) return true;
        ctx.caps[g * 2] = ps;
        ctx.caps[g * 2 + 1] = pe;
        return false;
      });
    }
    case 'noncapture': case 'flagged': return body;
    case 'atomic': return atomic(body, ctx);
    case 'lookahead': return (i, k) => {
      const saved = ctx.caps.slice();
      if (!body(i, () => true)) return false;
      if (k(i)) return true;
      ctx.caps = saved;
      return false;
    };
    case 'neglookahead': return (i, k) => {
      const saved = ctx.caps.slice();
      const hit = body(i, () => true);
      ctx.caps = saved;
      return !hit && k(i);
    };
    case 'lookbehind': case 'neglookbehind': {
      const reach = maxLength(node.body);
      const negative = node.kind === 'neglookbehind';
      return (i, k) => {
        const saved = ctx.caps.slice();
        let hit = false;
        const floor = Number.isFinite(reach) ? Math.max(0, i - reach) : 0;
        for (let j = i; j >= floor && !hit; j--) hit = body(j, (e) => e === i);
        if (negative) { ctx.caps = saved; return !hit && k(i); }
        if (!hit) return false;
        if (k(i)) return true;
        ctx.caps = saved;
        return false;
      };
    }
  }
}

/** Match `body` once, keep the first way it matches, and never come back to try another. */
function atomic(body: M, ctx: Ctx): M {
  return (i, k) => {
    const saved = ctx.caps.slice();
    let end = -1;
    if (!body(i, (j) => { end = j; return true; })) return false;
    if (k(end)) return true;
    ctx.caps = saved;
    return false;
  };
}

function compileQuant(node: Extract<RegexNode, { t: 'quant' }>, ctx: Ctx, parsed: ParsedRegex): M {
  const { min, max, mode } = node;
  const step = singleOf(node.body, ctx);
  if (step) return singleQuant(step, min, max, mode, ctx);

  const body = compile(node.body, ctx, parsed);
  const greedy: M = (i0, k) => {
    const loop = (count: number, i: number): boolean => {
      tick(ctx);
      if (count < max && body(i, (j) => (j === i && count >= min ? false : loop(count + 1, j)))) return true;
      return count >= min && k(i);
    };
    return loop(0, i0);
  };
  if (mode === 'possessive') return atomic(greedy, ctx);
  if (mode === 'greedy') return greedy;
  return (i0, k) => {
    const loop = (count: number, i: number): boolean => {
      tick(ctx);
      if (count >= min && k(i)) return true;
      return count < max && body(i, (j) => (j === i && count >= min ? false : loop(count + 1, j)));
    };
    return loop(0, i0);
  };
}

/** A quantifier over one code point at a time: iterative, so a long input costs no stack. */
function singleQuant(
  step: (i: number) => number, min: number, max: number,
  mode: 'greedy' | 'lazy' | 'possessive', ctx: Ctx,
): M {
  if (mode === 'lazy') {
    return (i, k) => {
      let cur = i;
      let count = 0;
      for (;;) {
        tick(ctx);
        if (count >= min && k(cur)) return true;
        if (count >= max) return false;
        const nx = step(cur);
        if (nx < 0) return false;
        cur = nx;
        count++;
      }
    };
  }
  return (i, k) => {
    const at = [i];
    let cur = i;
    while (at.length - 1 < max) {
      tick(ctx);
      const nx = step(cur);
      if (nx < 0) break;
      cur = nx;
      at.push(cur);
    }
    if (mode === 'possessive') return at.length - 1 >= min && k(at[at.length - 1]);
    for (let c = at.length - 1; c >= min; c--) {
      tick(ctx);
      if (k(at[c])) return true;
    }
    return false;
  };
}

function compileAnchor(kind: string, fl: FlagSet, ctx: Ctx): M {
  const atLineStart = (i: number): boolean => {
    if (i === 0) return true;
    const s = ctx.s;
    const prev = s.charCodeAt(i - 1);
    if (!isLineTerminator(prev, fl)) return false;
    // Between the `\r` and `\n` of a CRLF is not a line start; neither is the very end of input.
    if (prev === 13 && s.charCodeAt(i) === 10 && !fl.d) return false;
    return i < s.length;
  };
  const atLineEnd = (i: number): boolean => {
    const s = ctx.s;
    if (i >= s.length) return true;
    const c = s.charCodeAt(i);
    if (!isLineTerminator(c, fl)) return false;
    if (c === 10 && i > 0 && s.charCodeAt(i - 1) === 13 && !fl.d) return false;
    return true;
  };
  const beforeFinalTerminator = (i: number): boolean => {
    const s = ctx.s;
    if (i >= s.length) return true;
    const rest = s.slice(i);
    return fl.d ? rest === '\n' : /^(?:\r\n|[\n\r\u0085\u2028\u2029])$/.test(rest);
  };
  const word = (i: number): boolean => {
    if (i < 0 || i >= ctx.s.length) return false;
    return isWordChar(ctx.s.codePointAt(i)!, fl);
  };
  const boundary = (i: number) => word(i - 1) !== word(i);
  const tests: Record<string, (i: number) => boolean> = {
    '^': fl.m ? atLineStart : (i) => i === 0,
    '$': fl.m ? atLineEnd : beforeFinalTerminator,
    A: (i) => i === 0,
    Z: beforeFinalTerminator,
    z: (i) => i >= ctx.s.length,
    b: boundary,
    B: (i) => !boundary(i),
    G: (i) => i === ctx.lastEnd,
  };
  const test = tests[kind];
  return (i, k) => test(i) && k(i);
}

function compileBackref(
  node: Extract<RegexNode, { t: 'backref' }>, ctx: Ctx, parsed: ParsedRegex,
): M {
  const index = node.index ?? parsed.groupNames.indexOf(node.name);
  return (i, k) => {
    if (index < 1) return false;
    const a = ctx.caps[index * 2];
    const b = ctx.caps[index * 2 + 1];
    if (a < 0 || b < 0) return false;
    const want = ctx.s.slice(a, b);
    const got = ctx.s.slice(i, i + want.length);
    const same = node.fl.i ? got.toLowerCase() === want.toLowerCase() : got === want;
    return got.length === want.length && same && k(i + want.length);
  };
}

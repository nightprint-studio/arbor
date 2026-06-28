/**
 * merula — surgical literal edits over the Tree-sitter CST (pure, CodeMirror-free).
 *
 * The "code-first" round-trip from `design/merula/editing-model.md §1`: where a
 * mixer/inspector control maps to a **literal** in the source, the value is
 * invertible — locate the node by span and rewrite the text. This module is the
 * locate-and-rewrite half; {@link buildControlEdits} returns CodeMirror change
 * specs the editor applies as one undoable transaction, and the eval that
 * follows re-baselines the live overrides (so a committed value becomes the new
 * source-of-truth).
 *
 * Identity: a track is addressed by its **index** = the order of its
 * `track("name", pattern)` call in the source (the same convention the mixer /
 * arrangement use). The edited control lives in that track's pattern method chain
 * (`pattern.gain(0.8).pan(0.3)`); an absent control is **injected** at the chain
 * tail. A control whose argument is *calculated* (`.gain(rand(0,1))`) is not a
 * literal — it is reported as skipped, never silently mangled.
 *
 * Offsets are UTF-16 (tree-sitter `startIndex`/`endIndex`), which is exactly
 * CodeMirror's document coordinate — change specs drop straight into a dispatch.
 */

import type { Tree, Node } from './merula-lang';

// ── Public shapes ──────────────────────────────────────────────────────────────

/** The three delay parameters (`delay(t, fb, mix)`), committed as a unit. */
export interface DelayValues {
  /** Delay time in fractions of a cycle. */
  t: number;
  /** Feedback `0..1`. */
  fb: number;
  /** Wet send `0..1`. */
  mix: number;
}

/** Parametric-EQ band kind, mirroring the lang `.eq(kind, …)` keyword. */
export type EqKind = 'peak' | 'low' | 'high' | 'hpf' | 'lpf';

/** One parametric-EQ band (`.eq(kind, freq, gainDb, q)`). Bands are addressed by
 *  their index in source order (the k-th `.eq(...)` call in the chain). */
export interface EqBandValue {
  kind: EqKind;
  /** Centre / corner frequency in Hz. */
  freq: number;
  /** Boost / cut in dB (ignored for hpf/lpf). */
  gainDb: number;
  /** Bandwidth (peak) / resonance. */
  q: number;
  /** True when an argument is non-literal (the band is shown read-only). */
  calculated?: boolean;
}

/** The six compressor parameters (`comp(threshold, ratio, attack, release, makeup,
 *  knee)`), committed as a unit. */
export interface CompValues {
  threshold: number;
  ratio: number;
  attack: number;
  release: number;
  makeup: number;
  knee: number;
}

/** The literal values currently in a track's chain, when present *and* literal.
 *  A field is absent both when the method isn't in the chain and when its
 *  argument is calculated (use {@link TrackControls.calculated} to tell apart). */
export interface TrackControls {
  gain?: number;
  pan?: number;
  room?: number;
  delay?: DelayValues;
  /** Parametric-EQ bands in source order (the `.eq(...)` chain), or absent. */
  eq?: EqBandValue[];
  /** Compressor parameters (`.comp(...)`), or absent. */
  comp?: CompValues;
  /** Controls present in the chain but with a non-literal (calculated) argument
   *  — they can't be committed surgically, so the UI shows them as read-only. */
  calculated: Set<'gain' | 'pan' | 'room' | 'delay' | 'comp'>;
}

/** A single control edit to commit. EQ is edited per band (the k-th `.eq(...)`
 *  call): rewrite one band, add a band at the chain tail, or remove a band. */
export type ControlEdit =
  | { kind: 'gain' | 'pan' | 'room'; value: number }
  | { kind: 'delay'; t: number; fb: number; mix: number }
  | { kind: 'comp'; value: CompValues }
  | { kind: 'compRemove' }
  | { kind: 'eqBand'; band: number; value: EqBandValue }
  | { kind: 'eqAdd'; value: EqBandValue }
  | { kind: 'eqRemove'; band: number };

/** A CodeMirror change spec (UTF-16 offsets). */
export interface EditChange {
  from: number;
  to: number;
  insert: string;
}

/** Result of {@link buildControlEdits}: the (sorted, non-overlapping) changes to
 *  dispatch, plus the controls skipped because their argument was calculated. */
export interface ControlEditPlan {
  changes: EditChange[];
  skipped: ('gain' | 'pan' | 'room' | 'delay' | 'comp')[];
}

/** Pattern node types looser than a postfix method — appending `.gain(x)` to
 *  them would bind to a sub-expression, so the injection wraps them in parens. */
const LOW_PREC = new Set(['binary_expression', 'range_expression', 'lambda', 'unary_expression']);

// ── Tree navigation ────────────────────────────────────────────────────────────

/** Every `track("name", …)` call in document (declaration) order — the index
 *  into this list is the track's stable identity, matching the mixer. */
function trackCalls(tree: Tree): Node[] {
  const out: Node[] = [];
  const visit = (n: Node) => {
    if (n.type === 'call_expression') {
      const fn = n.childForFieldName('function');
      if (fn?.type === 'identifier' && fn.text === 'track') out.push(n);
    }
    for (const c of n.namedChildren) if (c) visit(c);
  };
  visit(tree.rootNode);
  return out;
}

/** A track call's pattern argument (the 2nd argument; the 1st is the name). */
function patternArg(trackCall: Node): Node | null {
  const args = trackCall.childForFieldName('arguments');
  if (!args) return null;
  const exprs = args.namedChildren.filter((c): c is Node => !!c);
  return exprs[1] ?? null;
}

/** Walk the method-chain spine of `pattern`, returning the `.name(...)` call
 *  node, or null when the chain doesn't include it. */
function findMethod(pattern: Node, name: string): Node | null {
  let node: Node | null = pattern;
  while (node && node.type === 'method_call') {
    if (node.childForFieldName('method')?.text === name) return node;
    node = node.childForFieldName('receiver');
  }
  return null;
}

/** Every `.name(...)` call in the chain spine, in **source order** (left to right
 *  as written). Used for EQ, where a chain holds several `.eq(...)` bands. */
function findMethods(pattern: Node, name: string): Node[] {
  const out: Node[] = [];
  let node: Node | null = pattern;
  // Spine walks outermost (last-written) → innermost (first-written); reverse for
  // source order so band index k is the k-th `.eq(...)` as authored.
  while (node && node.type === 'method_call') {
    if (node.childForFieldName('method')?.text === name) out.push(node);
    node = node.childForFieldName('receiver');
  }
  return out.reverse();
}

/** The string value of a string-literal node, or null when it isn't one. */
function stringValue(e: Node): string | null {
  if (e.type !== 'string') return null;
  // Strip the surrounding quotes; the CST keeps them in `text`.
  const t = e.text;
  return t.length >= 2 ? t.slice(1, -1) : t;
}

/** The numeric value of an expression node, or null when it isn't a literal
 *  (an identifier, a `rand(...)` call, … — i.e. a *calculated* argument). */
function numericValue(e: Node): number | null {
  if (e.type === 'number' || e.type === 'integer' || e.type === 'float') {
    const v = parseFloat(e.text);
    return Number.isFinite(v) ? v : null;
  }
  if (e.type === 'unary_expression' && e.childForFieldName('operator')?.text === '-') {
    const operand = e.childForFieldName('operand');
    const v = operand ? numericValue(operand) : null;
    return v == null ? null : -v;
  }
  return null;
}

/** The k-th expression argument of an `arguments` node (named children, in
 *  order), or null. */
function argAt(args: Node, k: number): Node | null {
  return args.namedChildren.filter((c): c is Node => !!c)[k] ?? null;
}

// ── Read current literal values (knob seeding) ────────────────────────────────

/** Extract each track's current literal controls from a parsed tree — used to
 *  seed the room/delay knobs (which have no live override, so they reflect the
 *  source). Keyed by track index; missing tracks return an empty map entry. */
export function extractTrackControls(tree: Tree): Map<number, TrackControls> {
  const map = new Map<number, TrackControls>();
  trackCalls(tree).forEach((tc, i) => {
    const ctl: TrackControls = { calculated: new Set() };
    const pat = patternArg(tc);
    if (pat) {
      for (const name of ['gain', 'pan', 'room'] as const) {
        const mc = findMethod(pat, name);
        if (!mc) continue;
        const a = mc.childForFieldName('arguments');
        const arg0 = a ? argAt(a, 0) : null;
        const v = arg0 ? numericValue(arg0) : null;
        if (v != null) ctl[name] = v;
        else ctl.calculated.add(name);
      }
      const dl = findMethod(pat, 'delay');
      if (dl) {
        const a = dl.childForFieldName('arguments');
        const t = a ? (argAt(a, 0) && numericValue(argAt(a, 0)!)) : null;
        if (t != null) {
          const fb = a && argAt(a, 1) ? numericValue(argAt(a, 1)!) : null;
          const mix = a && argAt(a, 2) ? numericValue(argAt(a, 2)!) : null;
          ctl.delay = { t, fb: fb ?? DELAY_DEFAULT_FB, mix: mix ?? DELAY_DEFAULT_MIX };
        } else {
          ctl.calculated.add('delay');
        }
      }

      // Parametric EQ: every `.eq(...)` band in the chain, in source order.
      const eqCalls = findMethods(pat, 'eq');
      if (eqCalls.length) {
        ctl.eq = eqCalls.map((mc) => {
          const a = mc.childForFieldName('arguments');
          const kindArg = a ? argAt(a, 0) : null;
          const kind = kindArg && stringValue(kindArg) ? normalizeEqKind(stringValue(kindArg)!) : null;
          const freq = a && argAt(a, 1) ? numericValue(argAt(a, 1)!) : null;
          const gainDb = a && argAt(a, 2) ? numericValue(argAt(a, 2)!) : null;
          const q = a && argAt(a, 3) ? numericValue(argAt(a, 3)!) : EQ_DEFAULT_Q;
          if (kind != null && freq != null && gainDb != null && q != null) {
            return { kind, freq, gainDb, q };
          }
          // A non-literal argument → keep the band's slot (so band indices stay
          // aligned to the source) but mark it read-only.
          return { kind: kind ?? 'peak', freq: freq ?? 1000, gainDb: gainDb ?? 0, q: q ?? EQ_DEFAULT_Q, calculated: true };
        });
      }

      // Compressor: the single `.comp(...)` call (threshold + ratio mandatory).
      const cp = findMethod(pat, 'comp');
      if (cp) {
        const a = cp.childForFieldName('arguments');
        const thr = a && argAt(a, 0) ? numericValue(argAt(a, 0)!) : null;
        const ratio = a && argAt(a, 1) ? numericValue(argAt(a, 1)!) : null;
        if (thr != null && ratio != null) {
          ctl.comp = {
            threshold: thr,
            ratio,
            attack: (a && argAt(a, 2) ? numericValue(argAt(a, 2)!) : null) ?? COMP_DEFAULTS.attack,
            release: (a && argAt(a, 3) ? numericValue(argAt(a, 3)!) : null) ?? COMP_DEFAULTS.release,
            makeup: (a && argAt(a, 4) ? numericValue(argAt(a, 4)!) : null) ?? COMP_DEFAULTS.makeup,
            knee: (a && argAt(a, 5) ? numericValue(argAt(a, 5)!) : null) ?? COMP_DEFAULTS.knee,
          };
        } else {
          ctl.calculated.add('comp');
        }
      }
    }
    map.set(i, ctl);
  });
  return map;
}

/** Defaults mirroring `make_delay` in the lang crate, so an injected delay reads
 *  back the same as a hand-written `delay(t)`. */
export const DELAY_DEFAULT_FB = 0.3;
export const DELAY_DEFAULT_MIX = 0.5;

/** Default Q for a new / q-less EQ band, mirroring `make_eq` in the lang crate. */
export const EQ_DEFAULT_Q = 0.7;
/** A fresh EQ band (knob seed when adding one): a gentle peak at 1 kHz. */
export const EQ_DEFAULT_BAND: EqBandValue = { kind: 'peak', freq: 1000, gainDb: 0, q: EQ_DEFAULT_Q };
/** Compressor defaults mirroring `make_comp` in the lang crate, so an injected
 *  compressor reads back the same as a hand-written `comp(thr, ratio)`. */
export const COMP_DEFAULTS: CompValues = {
  threshold: -18, ratio: 4, attack: 0.005, release: 0.1, makeup: 0, knee: 6,
};

/** Normalise a source `.eq(kind, …)` keyword to an {@link EqKind}, or null when
 *  unknown (the band is then treated as calculated / read-only). */
function normalizeEqKind(s: string): EqKind | null {
  switch (s.toLowerCase()) {
    case 'peak': case 'bell': return 'peak';
    case 'low': case 'lowshelf': return 'low';
    case 'high': case 'highshelf': return 'high';
    case 'hpf': case 'highpass': return 'hpf';
    case 'lpf': case 'lowpass': return 'lpf';
    default: return null;
  }
}

// ── Build edits ────────────────────────────────────────────────────────────────

/** Shortest round-trip number, rounded to 3 decimals (knob precision). Matches
 *  the lang emitter's style: integers print without `.0`, fractions without
 *  trailing zeros. */
export function fmtControl(n: number): string {
  return (Math.round(n * 1000) / 1000).toString();
}

/** Format an EQ band as the `.eq(...)` argument list `("kind", freq, gainDb, q)`. */
function eqArgs(b: EqBandValue): string {
  return `("${b.kind}", ${fmtControl(b.freq)}, ${fmtControl(b.gainDb)}, ${fmtControl(b.q)})`;
}

/** Plan the CodeMirror changes for committing `edits` on track `index`. Rewrites
 *  in-place where the literal exists, injects absent methods at the chain tail
 *  (one combined insert, paren-wrapped for a loose pattern), and reports controls
 *  skipped because their argument is calculated. Empty when the track isn't found. */
export function buildControlEdits(
  tree: Tree,
  src: string,
  index: number,
  edits: ControlEdit[],
): ControlEditPlan {
  const plan: ControlEditPlan = { changes: [], skipped: [] };
  const tc = trackCalls(tree)[index];
  const pat = tc ? patternArg(tc) : null;
  if (!pat) return plan;

  const rewrites: EditChange[] = [];
  const injects: string[] = []; // `.gain(0.8)` fragments to append

  for (const e of edits) {
    const mc = findMethod(pat, e.kind);
    if (e.kind === 'delay') {
      const body = `${fmtControl(e.t)}, ${fmtControl(e.fb)}, ${fmtControl(e.mix)}`;
      if (mc) {
        const a = mc.childForFieldName('arguments')!;
        rewrites.push({ from: a.startIndex, to: a.endIndex, insert: `(${body})` });
      } else {
        injects.push(`.delay(${body})`);
      }
      continue;
    }
    if (e.kind === 'comp') {
      const c = e.value;
      const body = `${fmtControl(c.threshold)}, ${fmtControl(c.ratio)}, ${fmtControl(c.attack)}, ${fmtControl(c.release)}, ${fmtControl(c.makeup)}, ${fmtControl(c.knee)}`;
      if (mc) {
        const a = mc.childForFieldName('arguments')!;
        rewrites.push({ from: a.startIndex, to: a.endIndex, insert: `(${body})` });
      } else {
        injects.push(`.comp(${body})`);
      }
      continue;
    }
    if (e.kind === 'eqBand') {
      // Rewrite the k-th `.eq(...)` band's arguments in place.
      const mcEq = findMethods(pat, 'eq')[e.band];
      if (mcEq) {
        const a = mcEq.childForFieldName('arguments')!;
        rewrites.push({ from: a.startIndex, to: a.endIndex, insert: eqArgs(e.value) });
      }
      continue;
    }
    if (e.kind === 'eqAdd') {
      injects.push(`.eq${eqArgs(e.value)}`);
      continue;
    }
    if (e.kind === 'eqRemove') {
      // Delete the k-th `.eq(...)` segment, keeping its receiver (the rest of the
      // chain). A single, non-overlapping delete — structural eq edits commit one
      // at a time, so this never co-occurs with a same-band rewrite.
      const mcEq = findMethods(pat, 'eq')[e.band];
      const recv = mcEq?.childForFieldName('receiver');
      if (mcEq && recv) {
        rewrites.push({ from: recv.endIndex, to: mcEq.endIndex, insert: '' });
      }
      continue;
    }
    if (e.kind === 'compRemove') {
      const cp = findMethod(pat, 'comp');
      const recv = cp?.childForFieldName('receiver');
      if (cp && recv) {
        rewrites.push({ from: recv.endIndex, to: cp.endIndex, insert: '' });
      }
      continue;
    }
    const val = fmtControl(e.value);
    if (mc) {
      const a = mc.childForFieldName('arguments')!;
      const arg0 = argAt(a, 0);
      if (!arg0) {
        rewrites.push({ from: a.startIndex, to: a.endIndex, insert: `(${val})` });
      } else if (numericValue(arg0) == null) {
        plan.skipped.push(e.kind); // calculated argument — leave it alone
      } else {
        rewrites.push({ from: arg0.startIndex, to: arg0.endIndex, insert: val });
      }
    } else {
      injects.push(`.${e.kind}(${val})`);
    }
  }

  plan.changes.push(...rewrites);
  if (injects.length) {
    const suffix = injects.join('');
    // A loose pattern (`a & b` doesn't reach here, but `par(a,b) + x` could) must
    // be parenthesised so the chain binds to the whole expression. Rewrites and a
    // low-prec inject are mutually exclusive: a low-prec pattern has no methods.
    if (LOW_PREC.has(pat.type)) {
      const text = src.slice(pat.startIndex, pat.endIndex);
      plan.changes.push({ from: pat.startIndex, to: pat.endIndex, insert: `(${text})${suffix}` });
    } else {
      plan.changes.push({ from: pat.endIndex, to: pat.endIndex, insert: suffix });
    }
  }

  plan.changes.sort((a, b) => a.from - b.from || a.to - b.to);
  return plan;
}

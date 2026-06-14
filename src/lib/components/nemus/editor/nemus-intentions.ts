/**
 * nemus intentions ("Alt+Enter" quick-fixes / context actions) — pure, CM-free.
 *
 * Given the caret + selection + the live tree, it returns the actions that apply
 * right there: surface the structural refactors contextually (rename / inline /
 * extract from `nemus-refactor`), fix an unresolved `inst("…")` / `s("…")` by
 * offering the closest installed instruments, and transpose the note(s) under the
 * caret (or in the selection) in place. Each item is either a set of ready
 * {@link EditChange}s or a `ui` flag that re-opens an input flow (rename/extract).
 *
 * Deliberately client-side + syntactic: it never needs the evaluator. The
 * semantic intentions the design also wants — "snap a note to the nearest scale
 * degree", "change `.scale(…)` and rewrite the degrees coherently" — need the
 * engine's scale model and are left for a backend scale-reference (a follow-up).
 */

import {
  identifierAt, extractSymbols, stringArgCallAt, type Tree, type Node,
} from './nemus-lang';
import { extractTarget, inlinePlan } from './nemus-refactor';
import type { EditChange } from './nemus-edit';
import type { NemusScaleMode } from '$lib/ipc/nemus';

/** One context action offered in the Alt+Enter popup. */
export interface IntentionItem {
  id: string;
  label: string;
  /** Ready-to-apply edits (an "edit" intention). */
  edits?: EditChange[];
  /** Toast shown on success. */
  note?: string;
  /** A host-UI intention: re-open an input flow instead of editing now (rename /
   *  extract need a name; change-scale needs the new scale spec), or a host-async
   *  one (`freeze` evaluates the selection in the backend and splices the result). */
  ui?: 'rename' | 'extract' | 'scale' | 'freeze';
  /** For a `freeze` intention: the source range to replace with the materialized
   *  literal notes (the selected pattern expression). */
  freeze?: { from: number; to: number };
}

/** What the editor hands the planner: the live tree + caret/selection + the set
 *  of resolvable instrument names + the scale catalogue (for the scale fixes). */
export interface IntentionContext {
  tree: Tree;
  src: string;
  head: number;
  from: number;
  to: number;
  instruments: string[];
  scales: NemusScaleMode[];
}

// ── Note literal ↔ MIDI (transpose) ─────────────────────────────────────────────

const LETTER_SEMITONE: Record<string, number> = { c: 0, d: 2, e: 4, f: 5, g: 7, a: 9, b: 11 };
const PC_NAMES = ['c', 'cs', 'd', 'ds', 'e', 'f', 'fs', 'g', 'gs', 'a', 'as', 'b'];
/** A note literal: letter + optional `s`(harp)/`f`(lat) + octave (`c4`, `fs4`, `a3`). */
const NOTE_RE = /^([a-gA-G])([sf]?)(-?\d+)$/;

/** Note name → MIDI (`c4` = 60), or null when not a note literal. */
function noteToMidi(text: string): number | null {
  const m = NOTE_RE.exec(text);
  if (!m) return null;
  const semi = LETTER_SEMITONE[m[1].toLowerCase()];
  if (semi == null) return null;
  const acc = m[2] === 's' ? 1 : m[2] === 'f' ? -1 : 0;
  return (parseInt(m[3], 10) + 1) * 12 + semi + acc;
}

/** MIDI → canonical note name (sharps), the inverse of {@link noteToMidi}. */
function midiToNote(midi: number): string {
  const pc = ((midi % 12) + 12) % 12;
  return `${PC_NAMES[pc]}${Math.floor(midi / 12) - 1}`;
}

/** Every note-literal node within `[from, to)` (source order). */
function noteNodesIn(tree: Tree, from: number, to: number): Node[] {
  const out: Node[] = [];
  const walk = (n: Node) => {
    if ((n.type === 'note_name' || n.type === 'note_literal')
      && n.startIndex >= from && n.endIndex <= to) out.push(n);
    for (let i = 0; i < n.childCount; i++) { const c = n.child(i); if (c) walk(c); }
  };
  walk(tree.rootNode);
  return out;
}

/** The note-literal node at `offset` (climbing to it), or null. */
function noteNodeAt(tree: Tree, offset: number): Node | null {
  const probe = (o: number): Node | null => {
    let n: Node | null = tree.rootNode.descendantForIndex(o);
    while (n && n.type !== 'note_name' && n.type !== 'note_literal') n = n.parent;
    return n;
  };
  return probe(offset) ?? (offset > 0 ? probe(offset - 1) : null);
}

/** Replace each note in `nodes` with itself transposed by `delta` semitones. */
function transposeEdits(nodes: Node[], delta: number): EditChange[] {
  const edits: EditChange[] = [];
  for (const n of nodes) {
    const midi = noteToMidi(n.text);
    if (midi != null) edits.push({ from: n.startIndex, to: n.endIndex, insert: midiToNote(midi + delta) });
  }
  return edits;
}

// ── Fuzzy instrument suggestions (unknown-instrument fix) ────────────────────────

function levenshtein(a: string, b: string): number {
  const m = a.length, n = b.length;
  if (!m) return n;
  if (!n) return m;
  let prev = Array.from({ length: n + 1 }, (_, i) => i);
  let cur = new Array<number>(n + 1);
  for (let i = 1; i <= m; i++) {
    cur[0] = i;
    for (let j = 1; j <= n; j++) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      cur[j] = Math.min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + cost);
    }
    [prev, cur] = [cur, prev];
  }
  return prev[n];
}

function similarity(q: string, c: string): number {
  if (c === q) return 100;
  if (c.startsWith(q) || q.startsWith(c)) return 80;
  if (c.includes(q) || q.includes(c)) return 60;
  const max = Math.max(q.length, c.length);
  return max ? Math.max(0, 40 - (levenshtein(q, c) / max) * 40) : 0;
}

/** The `k` installed instruments most like `name`, best first (score > 0 only). */
function closestInstruments(name: string, list: string[], k: number): string[] {
  const q = name.toLowerCase();
  return list
    .map((cand) => ({ cand, s: similarity(q, cand.toLowerCase()) }))
    .filter((x) => x.s > 0)
    .sort((a, b) => b.s - a.s)
    .slice(0, k)
    .map((x) => x.cand);
}

// ── Scales (snap-to-scale · change-scale) ───────────────────────────────────────

interface ParsedScale { rootPc: number; intervals: number[]; }

/** Parse a `"root:mode"` spec against the catalogue, or null. */
function parseScaleSpec(spec: string, modes: NemusScaleMode[]): ParsedScale | null {
  const i = spec.indexOf(':');
  if (i < 0) return null;
  const rm = /^([a-g])([sf]?)$/.exec(spec.slice(0, i).trim().toLowerCase());
  if (!rm) return null;
  const acc = rm[2] === 's' ? 1 : rm[2] === 'f' ? -1 : 0;
  const rootPc = (((LETTER_SEMITONE[rm[1]] + acc) % 12) + 12) % 12;
  const m = spec.slice(i + 1).trim().toLowerCase();
  const mode = modes.find((x) => x.name === m || x.aliases.includes(m));
  return mode ? { rootPc, intervals: mode.intervals } : null;
}

const pc = (midi: number) => (((midi % 12) + 12) % 12);

/** The pitch classes (0..11) the scale admits. */
function scalePcs(s: ParsedScale): Set<number> {
  return new Set(s.intervals.map((iv) => pc(s.rootPc + iv)));
}

/** Nearest MIDI to `midi` whose pitch class is in the scale (ties → downward). */
function snapToScale(midi: number, s: ParsedScale): number {
  const pcs = scalePcs(s);
  if (pcs.has(pc(midi))) return midi;
  for (let d = 1; d <= 6; d++) {
    if (pcs.has(pc(midi - d))) return midi - d;
    if (pcs.has(pc(midi + d))) return midi + d;
  }
  return midi;
}

/** The scale-degree index (0-based) of an in-scale `midi`, else null. */
function degreeIndex(midi: number, s: ParsedScale): number | null {
  const p = pc(midi);
  for (let i = 0; i < s.intervals.length; i++) if (pc(s.rootPc + s.intervals[i]) === p) return i;
  return null;
}

/** Re-spell `midi` (in scale `from`) at the same degree of scale `to`, preserving
 *  octave register. Null when `midi` isn't a degree of `from`. */
function remapDegree(midi: number, from: ParsedScale, to: ParsedScale): number | null {
  const i = degreeIndex(midi, from);
  if (i == null) return null;
  const oldPitch = from.rootPc + from.intervals[i];
  const k = Math.round((midi - oldPitch) / 12);
  const idx = Math.min(i, to.intervals.length - 1); // clamp across cardinalities
  return to.rootPc + to.intervals[idx] + 12 * k;
}

function unquote(s: string): string {
  return s.length >= 2 && s.startsWith('"') && s.endsWith('"') ? s.slice(1, -1) : s;
}

/** The `"root:mode"` spec of the nearest enclosing `.scale("…")` of `node`, or null. */
function enclosingScaleSpec(node: Node): string | null {
  for (let cur = node.parent; cur; cur = cur.parent) {
    if (cur.type === 'method_call' && cur.childForFieldName('method')?.text === 'scale') {
      const args = cur.childForFieldName('arguments');
      const str = args?.namedChildren.find((c) => c?.type === 'string');
      if (str) return unquote(str.text);
    }
  }
  return null;
}

/** The `.scale("…")` method call whose string contains `offset`, or null. */
function scaleCallAt(tree: Tree, offset: number): Node | null {
  const at = tree.rootNode.descendantForIndex(offset);
  for (let cur: Node | null = at; cur; cur = cur.parent) {
    if (cur.type === 'method_call' && cur.childForFieldName('method')?.text === 'scale') return cur;
  }
  return null;
}

/** Plan a "change scale" edit: replace the `.scale("…")` string with `newSpec`
 *  AND re-spell the note literals in its receiver so they keep the same scale
 *  degree (only when the old scale parses). Rejects an unknown new scale or a
 *  caret not on a scale call. */
export function changeScalePlan(
  tree: Tree, src: string, offset: number, newSpec: string, modes: NemusScaleMode[],
): { changes: EditChange[]; error?: string; note?: string } {
  const spec = newSpec.trim();
  const call = scaleCallAt(tree, offset);
  if (!call) return { changes: [], error: 'Place the caret on a .scale("…") call' };
  const args = call.childForFieldName('arguments');
  const str = args?.namedChildren.find((c) => c?.type === 'string');
  if (!str) return { changes: [], error: 'No scale string here' };
  const newS = parseScaleSpec(spec, modes);
  if (!newS) return { changes: [], error: `Unknown scale "${spec}"` };

  const changes: EditChange[] = [{ from: str.startIndex + 1, to: str.endIndex - 1, insert: spec }];

  // Re-spell the receiver's note literals to preserve their degree.
  let rewritten = 0;
  const oldS = parseScaleSpec(unquote(str.text), modes);
  const recv = call.childForFieldName('receiver');
  if (oldS && recv) {
    for (const n of noteNodesIn(tree, recv.startIndex, recv.endIndex)) {
      const m = noteToMidi(n.text);
      if (m == null) continue;
      const nm = remapDegree(m, oldS, newS);
      if (nm != null && nm !== m) { changes.push({ from: n.startIndex, to: n.endIndex, insert: midiToNote(nm) }); rewritten++; }
    }
  }
  changes.sort((a, b) => a.from - b.from || a.to - b.to);
  return {
    changes,
    note: rewritten ? `Scale → ${spec} (${rewritten} note${rewritten === 1 ? '' : 's'} re-spelled)` : `Scale → ${spec}`,
  };
}

// ── Collect ─────────────────────────────────────────────────────────────────────

/** The intentions available at the caret / selection, in offer order. */
export function collectIntentions(ctx: IntentionContext): IntentionItem[] {
  const { tree, src, head, from, to } = ctx;
  const items: IntentionItem[] = [];
  const hasSel = to > from;

  // Extract the selection into a named let / freeze it to concrete notes.
  const et = hasSel ? extractTarget(tree, src, from, to) : null;
  if (et) {
    items.push({ id: 'extract', label: 'Extract selection to let…', ui: 'extract' });
    items.push({ id: 'freeze', label: 'Freeze pattern to notes', ui: 'freeze', freeze: { from: et.from, to: et.to } });
  }

  // Symbol under the caret → rename + inline.
  const name = identifierAt(tree, head) ?? (head > 0 ? identifierAt(tree, head - 1) : null);
  if (name) {
    const { defs, imports } = extractSymbols(tree);
    if (defs.has(name) || imports.has(name)) {
      items.push({ id: 'rename', label: `Rename "${name}"…`, ui: 'rename' });
    }
    const inl = inlinePlan(tree, src, name);
    if (!inl.error) items.push({ id: 'inline', label: `Inline "${name}"`, edits: inl.changes, note: inl.note });
  }

  // Unresolved instrument → closest installed names.
  const sa = stringArgCallAt(tree, head);
  if (sa && (sa.fn === 'inst' || sa.fn === 's')) {
    const cur = src.slice(sa.from, sa.to);
    if (cur && !ctx.instruments.includes(cur)) {
      for (const cand of closestInstruments(cur, ctx.instruments, 4)) {
        items.push({
          id: `inst:${cand}`,
          label: `Change to "${cand}"`,
          edits: [{ from: sa.from, to: sa.to, insert: cand }],
          note: `Instrument → ${cand}`,
        });
      }
    }
  }

  // Change the scale under the caret (and re-spell its notes coherently).
  const sa2 = stringArgCallAt(tree, head);
  if (sa2 && sa2.fn === 'scale') {
    items.push({ id: 'change-scale', label: `Change scale "${src.slice(sa2.from, sa2.to)}"…`, ui: 'scale' });
  }

  // Transpose the note(s) in the selection, else the note under the caret.
  const notes = (hasSel ? noteNodesIn(tree, from, to) : [noteNodeAt(tree, head)].filter(Boolean) as Node[])
    .filter((n) => noteToMidi(n.text) != null);

  // Snap out-of-scale notes to the enclosing scale.
  if (notes.length && ctx.scales.length) {
    const spec = enclosingScaleSpec(notes[0]);
    const parsed = spec ? parseScaleSpec(spec, ctx.scales) : null;
    if (parsed) {
      const pcs = scalePcs(parsed);
      const outs = notes.filter((n) => !pcs.has(pc(noteToMidi(n.text)!)));
      if (outs.length) {
        const scope = outs.length > 1 ? `${outs.length} notes` : outs[0].text;
        items.push({
          id: 'snap',
          label: `Snap ${scope} to ${spec}`,
          edits: outs.map((n) => ({ from: n.startIndex, to: n.endIndex, insert: midiToNote(snapToScale(noteToMidi(n.text)!, parsed)) })),
          note: `Snapped to ${spec}`,
        });
      }
    }
  }

  if (notes.length) {
    const scope = notes.length > 1 ? `${notes.length} notes` : notes[0].text;
    items.push({ id: 'tr+1',  label: `Transpose ${scope} +1 semitone`, edits: transposeEdits(notes, 1),   note: 'Transposed +1' });
    items.push({ id: 'tr-1',  label: `Transpose ${scope} −1 semitone`, edits: transposeEdits(notes, -1),  note: 'Transposed −1' });
    items.push({ id: 'tr+12', label: `Transpose ${scope} +1 octave`,   edits: transposeEdits(notes, 12),  note: 'Transposed +1 octave' });
    items.push({ id: 'tr-12', label: `Transpose ${scope} −1 octave`,   edits: transposeEdits(notes, -12), note: 'Transposed −1 octave' });
  }

  return items;
}

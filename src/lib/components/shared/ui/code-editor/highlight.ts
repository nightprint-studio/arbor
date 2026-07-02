/**
 * Generic Tree-sitter → CodeMirror highlight ViewPlugin.
 *
 * Generalised from merula-cm's `MerulaHighlighter`: it owns the parser + the live
 * syntax tree for one editor, re-parses incrementally on single-change transactions
 * (the typing case) and from scratch otherwise, and rebuilds per-token mark
 * decorations (`cm-tok-<class>`) from one tree walk. The parser is created from the
 * {@link LanguageDescriptor} (`createParser` + `classify`); until the (async) grammar
 * wasm loads the document renders plain — a failed load leaves it plain forever, no
 * crash, exactly like merula.
 *
 * Also exports the byte(UTF-8)→UTF-16 offset converter the host uses to map backend
 * diagnostic spans onto CodeMirror coordinates.
 */

import { EditorView, ViewPlugin, Decoration } from '@codemirror/view';
import type { DecorationSet, ViewUpdate } from '@codemirror/view';
import { EditorState, StateEffect, RangeSetBuilder } from '@codemirror/state';

import type { LanguageDescriptor, TokenClass, Tree, Node, Parser } from './types';

/** Fired once the (async) grammar load completes, to kick the first full parse. */
export const parserReady = StateEffect.define<void>();

/** One cached mark decoration per token class (`cm-tok-<class>`). */
const TOKEN_MARKS = new Map<string, Decoration>();
function tokenMark(cls: TokenClass | string): Decoration {
  let m = TOKEN_MARKS.get(cls);
  if (!m) { m = Decoration.mark({ class: `cm-tok-${cls}` }); TOKEN_MARKS.set(cls, m); }
  return m;
}

function point(doc: EditorState['doc'], pos: number) {
  const line = doc.lineAt(pos);
  return { row: line.number - 1, column: pos - line.from };
}

/**
 * Owns the parser + the live syntax tree for one editor. Re-parses incrementally
 * (Tree-sitter `edit` + reparse with the previous tree) so typing stays cheap, and
 * rebuilds decorations from a single tree walk. The parser loads asynchronously;
 * until then the document renders plain (no crash, no flash).
 */
class TreeSitterHighlighter {
  decorations: DecorationSet = Decoration.none;
  tree: Tree | null = null;
  private parser: Parser | null = null;
  private destroyed = false;

  constructor(
    private view: EditorView,
    private lang: LanguageDescriptor,
  ) {
    // Async grammar load; once ready, do the first full parse via a self-effect.
    lang.createParser()
      .then((parser) => {
        if (this.destroyed) return; // editor torn down before the wasm loaded
        this.parser = parser;
        view.dispatch({ effects: parserReady.of() });
      })
      .catch(() => { /* grammar wasm missing — stays plain text */ });
  }

  destroy() { this.destroyed = true; }

  update(u: ViewUpdate) {
    if (!this.parser) return;
    const forced = u.transactions.some((tr) =>
      tr.effects.some((e) => e.is(parserReady)));

    if (u.docChanged) {
      // Incremental Tree-sitter editing is only coordinate-correct for a SINGLE
      // contiguous change (the typing case): there `fromA === fromB`, so the
      // old/new offsets map cleanly onto one `tree.edit`. A transaction with
      // *several* changes (a multi-span refactor / undo) mixes the old- and
      // new-document coordinate frames across edits and corrupts the incremental
      // tree — the highlight then reads garbage until the next full reparse. So for
      // multi-change transactions we reparse from scratch (rare + user-initiated,
      // so the cost is negligible), keeping per-keystroke typing incremental.
      let changeCount = 0;
      u.changes.iterChanges(() => { changeCount += 1; });
      const incremental = changeCount === 1 && this.tree !== null;
      if (incremental) {
        u.changes.iterChanges((fromA, toA, fromB, toB) => {
          this.tree!.edit({
            startIndex: fromA, oldEndIndex: toA, newEndIndex: toB,
            startPosition: point(u.startState.doc, fromA),
            oldEndPosition: point(u.startState.doc, toA),
            newEndPosition: point(u.state.doc, toB),
          });
        });
      }
      this.reparse(u.state, incremental);
    } else if (forced || (!this.tree && this.parser)) {
      this.reparse(u.state, false);
    }
  }

  private reparse(state: EditorState, incremental: boolean) {
    const text = state.doc.toString();
    try {
      const old = incremental ? (this.tree ?? undefined) : undefined;
      const next = this.parser!.parse(text, old);
      if (next) this.tree = next;
    } catch {
      // Incremental bookkeeping went inconsistent — recover with a fresh parse.
      const next = this.parser!.parse(text);
      if (next) this.tree = next;
    }
    this.decorations = this.buildDecorations();
  }

  private buildDecorations(): DecorationSet {
    if (!this.tree) return Decoration.none;
    const builder = new RangeSetBuilder<Decoration>();
    const classify = this.lang.classify;
    const visit = (node: Node, parentType: string | null, field: string | null) => {
      if (node.childCount === 0) {
        const cls = classify(node, node.isNamed, field, parentType);
        if (cls && node.endIndex > node.startIndex) {
          builder.add(node.startIndex, node.endIndex, tokenMark(cls));
        }
        return;
      }
      for (let i = 0; i < node.childCount; i++) {
        const child = node.child(i);
        if (child) visit(child, node.type, node.fieldNameForChild(i));
      }
    };
    visit(this.tree.rootNode, null, null);
    return builder.finish();
  }
}

/** Build the highlight ViewPlugin bound to a {@link LanguageDescriptor}. Also
 *  returns a `getTree(view)` reader for the live syntax tree (null until the grammar
 *  has loaded + the first parse ran) — used by go-to-decl. */
export function createHighlightPlugin(lang: LanguageDescriptor) {
  const plugin = ViewPlugin.define(
    (view) => new TreeSitterHighlighter(view, lang),
    { decorations: (v) => v.decorations },
  );

  /** Read the live syntax tree of an editor (null until ready). */
  function getTree(view: EditorView): Tree | null {
    return (view.plugin(plugin) as TreeSitterHighlighter | null)?.tree ?? null;
  }

  return { plugin, getTree };
}

// ── Byte (UTF-8) → UTF-16 offset mapping ───────────────────────────────────────

/** Build a converter from a UTF-8 byte offset (backend span coordinate) to a
 *  UTF-16 code-unit offset (CodeMirror/tree-sitter coordinate). Identity on
 *  pure-ASCII source (the common case); otherwise a binary search over code-point
 *  boundaries. Offsets are clamped into `[0, src.length]`. Mirrors merula-lang's
 *  `makeByteToU16`. */
export function makeByteToU16(src: string): (byte: number) => number {
  let ascii = true;
  for (let i = 0; i < src.length; i++) {
    if (src.charCodeAt(i) > 0x7f) { ascii = false; break; }
  }
  if (ascii) return (b) => (b < 0 ? 0 : b > src.length ? src.length : b);

  const bytePos: number[] = [0];
  const u16Pos: number[] = [0];
  let byte = 0;
  for (let i = 0; i < src.length; ) {
    const cp = src.codePointAt(i)!;
    const u16len = cp > 0xffff ? 2 : 1;
    const blen = cp <= 0x7f ? 1 : cp <= 0x7ff ? 2 : cp <= 0xffff ? 3 : 4;
    byte += blen;
    i += u16len;
    bytePos.push(byte);
    u16Pos.push(i);
  }
  const total = byte;
  return (b) => {
    if (b <= 0) return 0;
    if (b >= total) return src.length;
    // Largest index whose byte position is ≤ b.
    let lo = 0, hi = bytePos.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (bytePos[mid] <= b) lo = mid; else hi = mid - 1;
    }
    return u16Pos[lo];
  };
}

/** Build the inverse converter — a UTF-16 code-unit offset (CodeMirror/tree-sitter
 *  coordinate) to a UTF-8 byte offset (backend span coordinate). Identity on
 *  pure-ASCII source; otherwise it sums the UTF-8 byte lengths of the code points
 *  up to `u16`. Used when the host must hand the caret to a byte-indexed backend
 *  (e.g. a completion request). Offsets are clamped into `[0, byteTotal]`. */
export function makeU16ToByte(src: string): (u16: number) => number {
  let ascii = true;
  for (let i = 0; i < src.length; i++) {
    if (src.charCodeAt(i) > 0x7f) { ascii = false; break; }
  }
  if (ascii) return (u) => (u < 0 ? 0 : u > src.length ? src.length : u);

  const u16Pos: number[] = [0];
  const bytePos: number[] = [0];
  let byte = 0;
  for (let i = 0; i < src.length; ) {
    const cp = src.codePointAt(i)!;
    const u16len = cp > 0xffff ? 2 : 1;
    const blen = cp <= 0x7f ? 1 : cp <= 0x7ff ? 2 : cp <= 0xffff ? 3 : 4;
    byte += blen;
    i += u16len;
    u16Pos.push(i);
    bytePos.push(byte);
  }
  const total = byte;
  return (u) => {
    if (u <= 0) return 0;
    if (u >= src.length) return total;
    // Largest index whose UTF-16 position is ≤ u.
    let lo = 0, hi = u16Pos.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (u16Pos[mid] <= u) lo = mid; else hi = mid - 1;
    }
    return bytePos[lo];
  };
}

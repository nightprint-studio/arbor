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
import { StringStream } from '@codemirror/language';
import type { StreamParser } from '@codemirror/language';

import type { LanguageDescriptor, LeafSpan, TokenClass, Tree, Node, Parser } from './types';

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
  /** The document as the user typed it — what `leafHighlight` reads. The tree's own `node.text`
   *  comes from the (possibly `parseText`-masked) input, which is not the same string. */
  private source = '';

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
      // A `parseText` mask depends on text far from the edit (typing inside a `<script>` re-masks
      // it), so the previous tree describes a different input: always reparse from scratch then.
      const incremental = changeCount === 1 && this.tree !== null && !this.lang.parseText;
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
    this.source = state.doc.toString();
    const text = this.lang.parseText ? this.lang.parseText(this.source) : this.source;
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
    const injections = this.lang.injections;
    // Pre-order walk over an **explicit** stack rather than the call stack. Depth here
    // is the source's nesting depth, and some real code nests in the thousands — a
    // `a + b + c + …` chain is one level per operand — which recursion answers with a
    // blown stack and a dead window. Children are pushed in reverse so they pop in
    // index order, which is also what `RangeSetBuilder` requires (ascending positions).
    const stack: { node: Node; parentType: string | null; field: string | null }[] = [
      { node: this.tree.rootNode, parentType: null, field: null },
    ];
    while (stack.length > 0) {
      const { node, parentType, field } = stack.pop()!;
      if (node.childCount === 0) {
        // Embedded language (e.g. JS in a JSP `<script>` body): tokenize the leaf's raw
        // text with the injected StreamParser instead of a single `classify` colour.
        if (this.lang.leafHighlight && node.endIndex > node.startIndex) {
          const spans = this.lang.leafHighlight(node.type, this.source.slice(node.startIndex, node.endIndex), node);
          if (spans) {
            for (const sp of spans) builder.add(node.startIndex + sp.from, node.startIndex + sp.to, tokenMark(sp.cls));
            continue;
          }
        }
        const inj = injections?.[node.type];
        if (inj && node.endIndex > node.startIndex) {
          addInjectedTokens(builder, inj, node.text, node.startIndex);
          continue;
        }
        const cls = classify(node, node.isNamed, field, parentType);
        if (cls && node.endIndex > node.startIndex) {
          builder.add(node.startIndex, node.endIndex, tokenMark(cls));
        }
        continue;
      }
      for (let i = node.childCount - 1; i >= 0; i--) {
        const child = node.child(i);
        if (child) stack.push({ node: child, parentType: node.type, field: node.fieldNameForChild(i) });
      }
    }
    return builder.finish();
  }
}

// ── Embedded-language injection (CodeMirror StreamParser) ───────────────────────
//
// Run a legacy-mode StreamParser over a raw-text region (a JSP `<script>`/`<style>`
// body) and emit `cm-tok-*` marks for its tokens, so JS/CSS colours inside JSP without a
// nested tree-sitter grammar. Tokens are added in ascending order within the region, so
// the single global RangeSetBuilder stays sorted.

/** Map a legacy-mode (CM5-style) token type onto our {@link TokenClass}.
 *
 *  The last three are not CM5 names. The theme has classes CM5's vocabulary never had — a call
 *  site apart from a declaration, `this` apart from a keyword — and a mode written here
 *  (`js-mode.ts`) can say so. A legacy mode that never emits them is unaffected. */
function mapStreamToken(type: string): TokenClass | null {
  switch (type.split(' ')[0]) {
    case 'keyword': return 'keyword';
    case 'number': case 'unit': return 'number';
    case 'comment': return 'comment';
    case 'string': case 'string-2': return 'string';
    case 'atom': return 'constant';
    case 'def': return 'declaration';
    case 'variable-3': case 'type': case 'builtin': case 'qualifier': return 'type';
    case 'variable': case 'variable-2': return 'ident';
    case 'property': return 'field';
    case 'operator': return 'operator';
    case 'meta': return 'annotation';
    case 'tag': return 'keyword';
    case 'attribute': return 'field';
    case 'bracket': case 'punctuation': return 'punctuation';
    case 'callee': return 'function';
    case 'self': return 'self';
    case 'label': return 'label';
    default: return null;
  }
}

function addInjectedTokens(
  builder: RangeSetBuilder<Decoration>,
  parser: StreamParser<unknown>,
  text: string,
  base: number,
): void {
  for (const sp of streamTokenSpans(parser, text)) builder.add(base + sp.from, base + sp.to, tokenMark(sp.cls));
}

/** Run a legacy-mode StreamParser over `text` and return its coloured spans (offsets relative to
 *  `text`, ascending). Pure — also what a host's `leafHighlight` uses to colour the JS part of a
 *  region it has split into several languages. */
export function streamTokenSpans(parser: StreamParser<unknown>, text: string): LeafSpan[] {
  const out: LeafSpan[] = [];
  let state: unknown;
  try { state = parser.startState ? parser.startState(2) : {}; } catch { return out; }
  let pos = 0; // UTF-16 offset of the current line's start within `text`
  for (const line of text.split('\n')) {
    if (line.length === 0) {
      try { parser.blankLine?.(state, 2); } catch { /* ignore */ }
      pos += 1;
      continue;
    }
    const stream = new StringStream(line, 2, 2);
    let guard = 0;
    while (!stream.eol() && guard++ < 20000) {
      const start = stream.pos;
      // `StringStream.current()` is `slice(start, pos)`, and a stream parser reads the word it
      // just ate with it — every one of them does, it is how a keyword is recognised. CodeMirror's
      // own `StreamLanguage` sets this before each token; this loop is a second driver for the
      // same interface and has to do the same. Without it only the FIRST token of each line reads
      // correctly and every later one sees the whole line up to itself.
      stream.start = start;
      let tok: string | null = null;
      try { tok = parser.token(stream, state); } catch { break; }
      if (stream.pos <= start) { stream.pos = start + 1; continue; } // no-progress guard
      if (tok) {
        const cls = mapStreamToken(tok);
        if (cls) out.push({ from: pos + start, to: pos + stream.pos, cls });
      }
    }
    pos += line.length + 1; // +1 for the consumed newline
  }
  return out;
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

  // Non-ASCII source (a legacy Cp1252 tree is full of it). The lookup tables cost two arrays the size of
  // the file, and a completion or a ghost-text request asks for ONE offset per keystroke — so the first few
  // answers are a scan of the prefix, and the tables are built only for a caller that keeps asking (a
  // diagnostics pass converting hundreds of spans).
  let tables: { u16Pos: number[]; bytePos: number[]; total: number } | null = null;
  let scans = 0;
  const buildTables = () => {
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
    return { u16Pos, bytePos, total: byte };
  };
  return (u) => {
    if (u <= 0) return 0;
    if (!tables && scans < 3) {
      scans++;
      return utf8BytesBefore(src, u);
    }
    tables ??= buildTables();
    if (u >= src.length) return tables.total;
    // Largest index whose UTF-16 position is ≤ u.
    const { u16Pos, bytePos } = tables;
    let lo = 0, hi = u16Pos.length - 1;
    while (lo < hi) {
      const mid = (lo + hi + 1) >> 1;
      if (u16Pos[mid] <= u) lo = mid; else hi = mid - 1;
    }
    return bytePos[lo];
  };
}

/** UTF-8 byte length of `src` up to UTF-16 offset `u` — the same answer as the lookup tables of
 *  {@link makeU16ToByte}, for one offset, without building them. An offset inside a surrogate pair
 *  rounds down to the pair's start, as the tables do. */
function utf8BytesBefore(src: string, u: number): number {
  const end = u >= src.length ? src.length : u;
  let byte = 0;
  for (let i = 0; i < end; i++) {
    const c = src.charCodeAt(i);
    if (c < 0x80) byte += 1;
    else if (c < 0x800) byte += 2;
    else if (c >= 0xd800 && c <= 0xdbff) {
      const next = i + 1 < src.length ? src.charCodeAt(i + 1) : 0;
      if (next >= 0xdc00 && next <= 0xdfff) {
        if (i + 1 >= end) break; // the offset falls between the halves
        byte += 4;
        i++;
      } else {
        byte += 3; // a lone surrogate: counted as one code point of three bytes, as codePointAt does
      }
    } else byte += 3;
  }
  return byte;
}

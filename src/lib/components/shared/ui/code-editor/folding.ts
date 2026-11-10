/**
 * Client-side code folding for the shared code editor.
 *
 * Purely tree-sitter driven: there is NO backend involved. The highlight plugin
 * already owns a live syntax tree per editor; the fold service reads that same
 * tree (via the `getTree` reader threaded in from `createHighlightPlugin`) and
 * asks the {@link LanguageDescriptor.foldNode} hook whether a node at a given
 * line is foldable, returning the range to collapse.
 *
 * A `foldService` in CodeMirror is asked, for a given line, whether that line
 * starts a foldable region and, if so, what range to hide. We map that onto the
 * tree by finding the smallest node whose *start* is on the queried line and
 * whose end is on a later line, then handing it to `foldNode`. This yields the
 * IntelliJ-style behaviour where the head line (`class Foo {`, `/**`, …) stays
 * visible and its body collapses.
 */

import { foldGutter, foldService, codeFolding, foldEffect, unfoldEffect } from '@codemirror/language';
import { StateEffect, type Extension } from '@codemirror/state';
import type { EditorState } from '@codemirror/state';
import { EditorView, ViewPlugin, type ViewUpdate } from '@codemirror/view';

import type { LanguageDescriptor, Tree, Node } from './types';
import { foldAnimation } from './fold-animation';

/** Reader handed in from `createHighlightPlugin` — the live tree for a view. */
type GetTree = (view: EditorView) => Tree | null;

/**
 * Build the folding extension set (gutter + service + fold state) for a language
 * that provides {@link LanguageDescriptor.foldNode}. Returns an empty array when
 * the language doesn't opt into folding, so the caller can spread it
 * unconditionally.
 *
 * The fold service needs the live view to read the tree; CodeMirror's
 * `foldService` callback only gets `(state, lineStart, lineEnd)`, so we read the
 * tree from the state's field-less plugin via a small view lookup cached on the
 * state. To keep it simple and robust, we re-derive the tree from the highlight
 * plugin through the provided `getTree` bound to the *view* — obtained from the
 * `EditorView.findFromDOM`-free path: CodeMirror passes the `EditorState`, and
 * the plugin instance is reachable from the state's `.field`… but plugins aren't
 * state fields. Instead we resolve the view lazily via a WeakMap the plugin
 * populates. Simpler: we accept the `getTree` reader and look the view up from
 * the state using the shared registry below.
 */
export function createFoldingExtension(
  lang: LanguageDescriptor,
  getTree: GetTree,
): Extension {
  if (!lang.foldNode) return [];
  const foldNode = lang.foldNode;

  const service = foldService.of((state: EditorState, lineStart: number, lineEnd: number) => {
    const view = viewForState(state);
    if (!view) return null;
    const tree = getTree(view);
    if (!tree) return null;

    // Smallest node that *starts* within this line and spans past its end — the
    // fold owner for this line (e.g. the block whose `{` is on this line).
    const node = smallestFoldableOnLine(tree, foldNode, lineStart, lineEnd);
    if (!node) return null;
    const range = foldNode(node);
    if (!range) return null;
    // Clamp to the document + require the fold to actually cross the line end
    // (CodeMirror hides `from`→`to`; a fold that ends on the same line is a no-op).
    const from = Math.max(lineStart, Math.min(range.from, state.doc.length));
    const to = Math.max(from, Math.min(range.to, state.doc.length));
    if (to <= lineEnd) return null;
    return { from, to };
  });

  return [
    codeFolding(),
    // Must be listed before foldGutter: its ViewPlugin.update() populates stateToView
    // so the fold service (called from within the gutter's own update) finds the view.
    viewRegistry,
    importsService(getTree),
    service,
    foldGutter({
      // Compact chevrons that inherit the themed fold-gutter colours (theme.ts).
      markerDOM(open) {
        const el = document.createElement('span');
        el.className = open ? 'cm-foldMarker cm-foldMarker-open' : 'cm-foldMarker cm-foldMarker-closed';
        el.textContent = open ? '▾' : '▸'; // ▾ / ▸
        return el;
      },
      // Recomputed when the tree has caught up with the text, or when a fold/unfold action fires —
      // the gutter must redraw in both cases to show the correct ▾/▸ marker after a user toggle.
      foldingChanged: (u) => u.transactions.some((tr) => tr.effects.some((e) => e.is(treeCaughtUp) || e.is(foldEffect) || e.is(unfoldEffect))),
    }),
    treeCatchUp(getTree),
    foldAnimation(),
  ];
}

/** "The syntax tree changed under the fold gutter" — the only thing that makes it ask the fold service again. */
const treeCaughtUp = StateEffect.define<null>();

/**
 * Ask the fold gutter to recompute once the syntax tree is current.
 *
 * The gutter draws its arrows on the update that changed the document, and the tree is re-parsed by a
 * different plugin on that same update — in whichever order the two happen to run. When the gutter goes
 * first it asks the fold service about a tree that is the previous text (or null, before the grammar
 * loads), gets no answer for the blocks that moved or appeared, and never asks again: the arrows of
 * nested blocks just disappear until something else redraws the gutter. So this notices, after the
 * update, that the tree is not the one the arrows were drawn from, and sends one effect that makes the
 * gutter recompute. A microtask, because a view plugin may not dispatch while it is reacting to an update.
 */
function treeCatchUp(getTree: GetTree): Extension {
  return ViewPlugin.fromClass(
    class {
      private seen: Tree | null | undefined;
      private queued = false;
      private destroyed = false;

      constructor(private readonly view: EditorView) {}

      update(u: ViewUpdate) {
        // Its own effect is the end of the loop, not a reason to look again.
        if (u.transactions.some((tr) => tr.effects.some((e) => e.is(treeCaughtUp)))) return;
        if (this.queued) return;
        this.queued = true;
        queueMicrotask(() => {
          this.queued = false;
          if (this.destroyed) return;
          const tree = getTree(this.view);
          if (tree === this.seen) return;
          this.seen = tree;
          this.view.dispatch({ effects: treeCaughtUp.of(null) });
        });
      }

      destroy() {
        this.destroyed = true;
      }
    },
  );
}

/**
 * Collapse the file's **block comments** once, when it opens.
 *
 * IntelliJ's "fold block comments by default": the licence header and the long `/** … *\/`
 * above every method are the two things you have already read, and on a legacy file they can be
 * most of what is on screen. Folding them on open is different from folding them by hand —
 * afterwards the buffer is yours, and nothing re-folds what you unfold.
 *
 * A comment is recognised from what the descriptor already declares: a foldable node whose text
 * starts with the language's block-comment opener (`commentTokens.block.open`). No new hook, and
 * nothing to keep in sync — a language that has no block comment simply has nothing to fold.
 *
 * Runs on the first update that has a tree (the grammar wasm loads asynchronously, so there is
 * rarely one at mount) and then never again: this is an opening state, not a rule about how the
 * buffer must look. The fold is dispatched from a microtask because a view plugin may not
 * dispatch during the update it is reacting to.
 */
export function foldBlockCommentsOnLoad(lang: LanguageDescriptor, getTree: GetTree): Extension {
  const foldNode = lang.foldNode;
  const opener = lang.commentTokens?.block?.open;
  if (!foldNode || !opener) return [];

  return ViewPlugin.fromClass(
    class {
      /** One shot: set as soon as a tree has been seen, whether or not it held a comment. */
      private done = false;

      constructor(view: EditorView) {
        this.foldOnce(view);
      }

      update(u: ViewUpdate) {
        this.foldOnce(u.view);
      }

      private foldOnce(view: EditorView) {
        if (this.done) return;
        const tree = getTree(view);
        if (!tree) return;
        this.done = true;
        const ranges: { from: number; to: number }[] = [];
        collectCommentFolds(view.state, tree.rootNode, foldNode, opener, ranges);
        if (!ranges.length) return;
        // A view plugin may not dispatch during the update it is reacting to.
        queueMicrotask(() => view.dispatch({ effects: ranges.map((r) => foldEffect.of(r)) }));
      }
    },
  );
}

/** Walk the tree collecting the fold range of every block comment; a comment's subtree is not
 *  entered (a comment inside a comment is the same fold). */
function collectCommentFolds(
  state: EditorState,
  node: Node,
  foldNode: (n: Node) => { from: number; to: number } | null,
  opener: string,
  out: { from: number; to: number }[],
): void {
  const isComment =
    node.endIndex - node.startIndex >= opener.length &&
    state.doc.sliceString(node.startIndex, node.startIndex + opener.length) === opener;
  if (isComment) {
    const range = foldNode(node);
    // A single-line comment has nothing to hide — CodeMirror would draw a placeholder over
    // text that was already one line.
    if (range && state.doc.lineAt(range.from).number < state.doc.lineAt(range.to).number) {
      out.push({ from: range.from, to: Math.min(range.to, state.doc.length) });
    }
    return;
  }
  for (let i = 0; i < node.childCount; i++) {
    const child = node.child(i);
    if (child) collectCommentFolds(state, child, foldNode, opener, out);
  }
}

/** Find the smallest node starting on `[lineStart,lineEnd]` that `foldNode`
 *  accepts and that spans past the line end. Walks from the line-start position
 *  down to the leaf, then back up to the first accepted foldable ancestor. */
function smallestFoldableOnLine(
  tree: Tree,
  foldNode: (n: Node) => { from: number; to: number } | null,
  lineStart: number,
  lineEnd: number,
): Node | null {
  // Descend at the line END: a foldable block's opening marker (`{`) usually sits
  // at the end of the head line, so the node just before the newline is inside
  // the body. Climbing from there finds the body (`class_body`, `block`, …) as an
  // ancestor even though it's a *later child* on the head line — which a descent
  // at lineStart would miss (it lands in the declaration head instead).
  let cur: Node | null = tree.rootNode.descendantForIndex(Math.max(lineStart, lineEnd - 1));
  let best: Node | null = null;
  while (cur) {
    // The fold owner starts on THIS line and its collapsed range escapes it.
    if (cur.startIndex >= lineStart && cur.startIndex <= lineEnd && cur.endIndex > lineEnd) {
      const r = foldNode(cur);
      if (r && r.to > lineEnd) best = cur; // keep climbing → outermost head on this line
    }
    cur = cur.parent;
  }
  return best;
}

// ── The import block ────────────────────────────────────────────────────────

/** Node kinds that are one import / use statement, across the grammars that fold. */
const IMPORT_KINDS = new Set(['import_declaration', 'import_statement', 'use_declaration']);
/** Kinds that may sit between two imports without ending the block. */
const IN_BETWEEN_KINDS = new Set(['line_comment', 'block_comment', 'comment']);

/**
 * The range of the leading import block: from just after the first import's keyword to the end of the last
 * consecutive one, so a collapsed block reads `import …` on the line of the first import — what IntelliJ
 * shows. `null` when there is no block, or it is a single line (nothing to hide).
 *
 * "Consecutive" means siblings at the top of the file with only comments between them: the first
 * thing that is neither ends the block, so an import further down (Python, a generated file) does not
 * stretch the fold over code.
 */
export function importBlockRange(state: EditorState, tree: Tree): { from: number; to: number } | null {
  const root = tree.rootNode;
  let first: Node | null = null;
  let last: Node | null = null;
  for (let i = 0; i < root.childCount; i++) {
    const child = root.child(i);
    if (!child) continue;
    if (IMPORT_KINDS.has(child.type)) {
      first ??= child;
      last = child;
    } else if (first && !IN_BETWEEN_KINDS.has(child.type)) {
      break;
    }
  }
  if (!first || !last) return null;
  const keyword = /^\w+/.exec(state.doc.sliceString(first.startIndex, Math.min(first.startIndex + 12, state.doc.length)));
  const from = first.startIndex + (keyword ? keyword[0].length : 0);
  const to = Math.min(last.endIndex, state.doc.length);
  if (state.doc.lineAt(from).number >= state.doc.lineAt(to).number) return null;
  return { from, to };
}

/** Make the import block foldable: the fold belongs to the line of the first import. */
function importsService(getTree: GetTree): Extension {
  return foldService.of((state, lineStart, lineEnd) => {
    const view = viewForState(state);
    const tree = view ? getTree(view) : null;
    if (!tree) return null;
    const range = importBlockRange(state, tree);
    return range && range.from >= lineStart && range.from <= lineEnd ? range : null;
  });
}

/**
 * Collapse the import block once, when the file opens.
 *
 * The same one-shot shape as {@link foldBlockCommentsOnLoad}: an opening state, not a rule — whatever you
 * do with the fold afterwards stays done, and the host carries it from tab to tab (see the editor's view
 * snapshot). It is the host that says a tab already has a remembered state, by not installing this at all.
 */
export function foldImportsOnLoad(getTree: GetTree): Extension {
  return ViewPlugin.fromClass(
    class {
      private done = false;

      constructor(view: EditorView) {
        this.foldOnce(view);
      }

      update(u: ViewUpdate) {
        this.foldOnce(u.view);
      }

      private foldOnce(view: EditorView) {
        if (this.done) return;
        const tree = getTree(view);
        if (!tree) return;
        this.done = true;
        const range = importBlockRange(view.state, tree);
        if (!range) return;
        queueMicrotask(() => view.dispatch({ effects: foldEffect.of(range) }));
      }
    },
  );
}

/**
 * Come {@link foldImportsOnLoad}, ma per un linguaggio senza albero: il range lo calcola `range(state)` dal
 * testo. Una volta sola, al primo aggiornamento in cui il documento non e' vuoto (il file puo' arrivare dopo la vista).
 */
export function foldRangeOnLoad(range: (state: EditorState) => { from: number; to: number } | null): Extension {
  return ViewPlugin.fromClass(
    class {
      private done = false;

      constructor(view: EditorView) {
        this.foldOnce(view);
      }

      update(u: ViewUpdate) {
        this.foldOnce(u.view);
      }

      private foldOnce(view: EditorView) {
        if (this.done || view.state.doc.length === 0) return;
        this.done = true;
        const r = range(view.state);
        if (!r) return;
        queueMicrotask(() => view.dispatch({ effects: foldEffect.of(r) }));
      }
    },
  );
}

// ── View registry ──────────────────────────────────────────────────────────
//
// CodeMirror's foldService callback receives only the EditorState, but reading
// the tree needs the EditorView (the highlight plugin is view-scoped). We keep a
// tiny WeakMap from state → view, refreshed by a ViewPlugin so the mapping is
// populated BEFORE the fold gutter's own plugin runs and calls the fold service.
// An updateListener runs after all ViewPlugin updates, which is too late — the
// fold service would see the new state without a view entry and return null,
// causing the import-block chevron to vanish right after the user unfolds it.

const stateToView = new WeakMap<EditorState, EditorView>();
function viewForState(state: EditorState): EditorView | null {
  return stateToView.get(state) ?? null;
}
const viewRegistry: Extension = ViewPlugin.fromClass(
  class {
    constructor(view: EditorView) {
      stateToView.set(view.state, view);
    }
    update(u: ViewUpdate) {
      stateToView.set(u.state, u.view);
      if (u.startState !== u.state) stateToView.set(u.startState, u.view);
    }
  },
);

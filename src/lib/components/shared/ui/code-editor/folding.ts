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

import { foldGutter, foldService, codeFolding } from '@codemirror/language';
import type { Extension } from '@codemirror/state';
import type { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

import type { LanguageDescriptor, Tree, Node } from './types';

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
    service,
    foldGutter({
      // Compact chevrons that inherit the themed fold-gutter colours (theme.ts).
      markerDOM(open) {
        const el = document.createElement('span');
        el.className = open ? 'cm-foldMarker cm-foldMarker-open' : 'cm-foldMarker cm-foldMarker-closed';
        el.textContent = open ? '▾' : '▸'; // ▾ / ▸
        return el;
      },
    }),
    // Keep the view registry in sync so the (state-only) fold service can reach
    // the live tree through its view.
    viewRegistry,
  ];
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

// ── View registry ──────────────────────────────────────────────────────────
//
// CodeMirror's foldService callback receives only the EditorState, but reading
// the tree needs the EditorView (the highlight plugin is view-scoped). We keep a
// tiny WeakMap from state → view, refreshed by an updateListener bundled into
// the folding extension, so the service can resolve its view.

const stateToView = new WeakMap<EditorState, EditorView>();
function viewForState(state: EditorState): EditorView | null {
  return stateToView.get(state) ?? null;
}
const viewRegistry: Extension = EditorView.updateListener.of((u) => {
  stateToView.set(u.state, u.view);
  if (u.startState !== u.state) stateToView.set(u.startState, u.view);
});

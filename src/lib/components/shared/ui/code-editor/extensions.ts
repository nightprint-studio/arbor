/**
 * The generic CodeMirror 6 extension set for the shared code editor.
 *
 * `createCodeEditorExtensions(lang, opts)` assembles the standard editing surface
 * (theme + gutters + history + selection + bracket-matching + active-line +
 * selection-match highlight + search + the Tree-sitter highlight plugin + lint
 * gutter + a keymap) around a {@link LanguageDescriptor}. It is app-agnostic: no
 * Arbor domain imports, only `@codemirror/*`, `web-tree-sitter`, and the sibling
 * core files.
 *
 * As in merula, CodeMirror's search panel IS wired (so `Ctrl+F` searches the buffer
 * when the editor is focused), but its *open* binding is removed from the keymap so
 * the host owns `Ctrl+F` routing and calls `openSearch()` imperatively. The
 * in-panel navigation (next / previous / replace / close) stays, so search is fully
 * keyboard-driven once opened.
 */

import {
  EditorView, lineNumbers, keymap,
  highlightActiveLine, highlightActiveLineGutter, drawSelection,
} from '@codemirror/view';
import { EditorState, type Extension } from '@codemirror/state';
import { history, defaultKeymap, historyKeymap, indentWithTab } from '@codemirror/commands';
import { bracketMatching, indentOnInput, foldKeymap } from '@codemirror/language';
import { lintGutter, lintKeymap } from '@codemirror/lint';
import { search, searchKeymap, highlightSelectionMatches } from '@codemirror/search';
import { autocompletion, completionKeymap } from '@codemirror/autocomplete';

import type { LanguageDescriptor, Tree, Node } from './types';
import { createHighlightPlugin } from './highlight';
import { createFoldingExtension } from './folding';
import { codeEditorTheme } from './theme';

/** The search keymap minus its open binding: the host owns `Ctrl+F` so it can route
 *  it to the editor (when the pane is focused) or elsewhere, and calls `openSearch()`
 *  imperatively. The in-panel navigation stays so search is keyboard-complete. */
const searchKeymapNoOpen = searchKeymap.filter((b) => b.key !== 'Mod-f');

export interface CodeEditorExtensionsOptions {
  readOnly?: boolean;
  /** Ctrl/Cmd+Click on an identifier — the host resolves + jumps (go-to-decl). The
   *  descriptor's `resolveGoto` (when present) is tried first for a local jump; when
   *  it returns null (or is absent), the bare word is handed to `onGoto`. */
  onGoto?: (word: string, view: EditorView) => void;
  /** Language-intelligence hook bag (reserved; opaque to the core today). */
  intel?: unknown;
}

/** Build the full extension set for one editor bound to `lang`. Returns the
 *  extensions plus the `getTree` reader (so a host component can implement
 *  go-to-decl / structure against the same live tree the highlighter maintains). */
export function createCodeEditorExtensions(
  lang: LanguageDescriptor,
  opts: CodeEditorExtensionsOptions = {},
): { extensions: Extension; getTree: ReturnType<typeof createHighlightPlugin>['getTree'] } {
  const { plugin: highlight, getTree } = createHighlightPlugin(lang);

  // Client-side folding — installed only when the descriptor opts in via
  // `foldNode`. It reads the same live tree the highlighter maintains (no
  // backend), so folding is free once a grammar loads.
  const folding = createFoldingExtension(lang, getTree);

  const exts: Extension[] = [
    codeEditorTheme,
    lineNumbers(),
    history(),
    drawSelection(),
    indentOnInput(),
    bracketMatching(),
    folding,
    highlightActiveLine(),
    highlightActiveLineGutter(),
    highlightSelectionMatches(),
    search({ top: true }),
    highlight,
    lintGutter(),
  ];

  // Language intelligence (autocomplete) — only when the descriptor supplies a
  // completion source and the editor is editable. Added before the base keymap
  // so its completion keymap (Enter / ↑↓ / Esc while the popup is open) wins.
  const completionSource = lang.intel?.completion;
  if (completionSource && !opts.readOnly) {
    exts.push(autocompletion({ override: [completionSource], defaultKeymap: false }));
  }

  exts.push(
    keymap.of([
      ...defaultKeymap, ...historyKeymap, ...lintKeymap, ...foldKeymap,
      ...(completionSource && !opts.readOnly ? completionKeymap : []),
      ...searchKeymapNoOpen, indentWithTab,
    ]),
    EditorState.readOnly.of(!!opts.readOnly),
  );

  // Go-to-declaration (Ctrl/Cmd+Click). Try the descriptor's local resolver first;
  // otherwise report the identifier under the cursor to the host.
  if (opts.onGoto) {
    const { onGoto } = opts;
    exts.push(EditorView.domEventHandlers({
      mousedown(event, view) {
        if (!(event.ctrlKey || event.metaKey)) return false;
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos == null) return false;
        const tree = getTree(view);
        if (!tree) return false;
        // Local resolver wins when it can jump within the buffer.
        if (lang.resolveGoto) {
          const target = lang.resolveGoto(tree, pos);
          if (target) {
            event.preventDefault();
            view.dispatch({
              selection: { anchor: target.offset },
              effects: EditorView.scrollIntoView(target.offset, { y: 'center' }),
            });
            view.focus();
            return true;
          }
        }
        const word = identifierTextAt(tree, pos);
        if (word) {
          event.preventDefault();
          onGoto(word, view);
          return true;
        }
        return false;
      },
    }));
  }

  return { extensions: exts, getTree };
}

/** The smallest `identifier`-ish leaf covering `offset` (UTF-16), or null — the
 *  Ctrl+Click word. Generic: matches any leaf whose type contains `identifier`
 *  (covers `identifier`, `type_identifier`, `field_identifier`, …). */
function identifierTextAt(tree: Tree, offset: number): string | null {
  const node = tree.rootNode.descendantForIndex(offset);
  if (!node) return null;
  let cur: Node | null = node;
  while (cur) {
    if (cur.type.includes('identifier') && cur.childCount === 0) return cur.text;
    cur = cur.parent;
  }
  return null;
}
